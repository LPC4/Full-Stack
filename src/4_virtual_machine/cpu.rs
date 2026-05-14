pub mod alu;
pub mod csr;
pub mod decoder;
pub mod mmu;
pub mod pipeline;
pub mod pipelined;
pub mod registers;
pub mod syscall;
pub mod traps;
pub use cpu_impl::{Cpu, StepOutcome};
pub use pipelined::{PipelineStats, PipelinedCpu, TickOutcome};
pub use registers::PrivilegeMode;

mod cpu_impl {
    use crate::virtual_machine::bus::SystemBus;
    use crate::virtual_machine::cpu::csr::CsrFile;
    use crate::virtual_machine::cpu::decoder::DecodedInsn;
    use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
    use crate::virtual_machine::cpu::pipeline::{decode, execute, fetch, memory, writeback};
    use crate::virtual_machine::cpu::registers::Registers;
    use crate::virtual_machine::cpu::syscall;
    use crate::virtual_machine::cpu::traps;
    use crate::virtual_machine::error::VmError;

    // ---------------------------------------------------------------------------
    // Public types
    // ---------------------------------------------------------------------------

    #[derive(Debug)]
    pub enum StepOutcome {
        Continue,
        Halted(i64),
    }

    /// The CPU core that owns all processor state and executes the instruction pipeline.
    pub struct Cpu {
        regs: Registers,
        csrs: CsrFile,
        reservation: Option<u64>,
    }

    impl Cpu {
        /// Create a new CPU with the given start PC and stack pointer.
        pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
            let mut regs = Registers::new();
            regs.pc = start_pc;
            regs.write_x(2, stack_ptr); // sp

            Self {
                regs,
                csrs: CsrFile::new(),
                reservation: None,
            }
        }

        // -----------------------------------------------------------------------
        // Pipeline stage wrappers
        // -----------------------------------------------------------------------

        /// Fetch the next instruction from memory.
        fn fetch_instruction(&self, bus: &mut SystemBus) -> Result<u32, VmError> {
            fetch::fetch(bus, self.regs.pc, self.csrs.satp, self.regs.priv_mode)
        }

        /// Execute a decoded instruction.
        fn execute(&self, insn: &DecodedInsn) -> Result<ExecResult, VmError> {
            execute::execute(insn, &self.regs, &self.csrs, self.regs.pc)
        }

        /// Perform memory operations for Load/Store/Atomic instructions.
        fn memory_stage(
            &mut self,
            result: ExecResult,
            bus: &mut SystemBus,
        ) -> Result<memory::MemResult, VmError> {
            memory::memory_stage(
                result,
                bus,
                &mut self.reservation,
                self.csrs.satp,
                self.regs.priv_mode,
            )
        }

        /// Write back results to registers and CSRs.
        fn writeback(&mut self, result: memory::MemResult) -> Result<u64, VmError> {
            writeback::writeback(result, &mut self.regs, &mut self.csrs)
        }

        // -----------------------------------------------------------------------
        // Trap / interrupt handling
        // -----------------------------------------------------------------------

        /// Enter a trap: saves state, updates CSRs, jumps to handler.
        fn take_trap(&mut self, cause: u64, tval: u64) {
            let pc = self.regs.pc;
            let new_pc = traps::take_trap(
                &mut self.regs,
                &mut self.csrs,
                cause,
                tval,
                pc,
            );
            self.regs.pc = new_pc;
        }

        /// Map a `VmError` to a trap cause/tval pair, invoke the handler (if installed),
        /// or return the original error if no handler is available.
        fn dispatch_trap(&mut self, e: VmError) -> Result<StepOutcome, VmError> {
            let (cause, tval) = match traps::error_to_trap_cause(&e) {
                Some(ct) => ct,
                None => return Err(e),
            };

            // Always record diagnostic CSRs.
            self.csrs.mcause = cause;
            self.csrs.mepc = self.regs.pc & !0x3u64;
            self.csrs.mtval = tval;

            if self.csrs.mtvec != 0 {
                self.take_trap(cause, tval);
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                Ok(StepOutcome::Continue)
            } else {
                Err(e)
            }
        }

        /// Tick the CLINT timer, update MIP.MTIP, and take any pending interrupt
        /// whose enable bit is set and global MIE is active.
        /// Returns `Some(StepOutcome::Continue)` if an interrupt was taken.
        fn check_interrupts(&mut self, bus: &mut SystemBus) -> Option<StepOutcome> {
            // Advance CLINT time counter by one tick per instruction.
            bus.clint_mut().tick();
            if bus.clint_mut().timer_irq_pending() {
                self.csrs.mip |= 1u64 << 7; // set MTIP
            } else {
                self.csrs.mip &= !(1u64 << 7); // clear MTIP
            }

            // Only deliver if MIE (global interrupt enable in mstatus) is set.
            let mstatus_mie = (self.csrs.mstatus >> 3) & 1;
            if mstatus_mie == 0 {
                return None;
            }

            let pending = self.csrs.mip & self.csrs.mie;
            if pending == 0 {
                return None;
            }

            // Priority: MEI > MSI > MTI (per RISC-V privilege spec).
            let cause = if pending & (1 << 11) != 0 {
                traps::CAUSE_M_EXTERNAL_IRQ
            } else if pending & (1 << 3) != 0 {
                traps::CAUSE_M_SOFTWARE_IRQ
            } else if pending & (1 << 7) != 0 {
                traps::CAUSE_M_TIMER_IRQ
            } else {
                return None;
            };

            self.take_trap(cause, 0);
            self.csrs.increment_instret();
            self.csrs.increment_cycle();
            Some(StepOutcome::Continue)
        }

        /// Handle ecall syscall instructions.
        fn handle_ecall(&mut self, bus: &mut SystemBus) -> Result<StepOutcome, VmError> {
            use crate::virtual_machine::cpu::registers::PrivilegeMode;

            // In S-mode or U-mode, ecall should trap to M-mode
            if self.regs.priv_mode != PrivilegeMode::Machine {
                let cause = match self.regs.priv_mode {
                    PrivilegeMode::User => traps::CAUSE_ECALL_U,
                    PrivilegeMode::Supervisor => traps::CAUSE_ECALL_S,
                    PrivilegeMode::Machine => traps::CAUSE_ECALL_M, // shouldn't reach here
                };
                self.take_trap(cause, 0);
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                return Ok(StepOutcome::Continue);
            }

            // M-mode ecall - handle as syscall using the syscall module
            match syscall::handle_syscall(&mut self.regs, &mut self.csrs, bus)? {
                syscall::SyscallOutcome::Continue => Ok(StepOutcome::Continue),
                syscall::SyscallOutcome::Halted(code) => Ok(StepOutcome::Halted(code)),
            }
        }


        // -----------------------------------------------------------------------
        // Main step function
        // -----------------------------------------------------------------------

        /// Execute a single instruction cycle.
        pub fn step(&mut self, bus: &mut SystemBus) -> Result<StepOutcome, VmError> {
            // Check for pending interrupts before fetching the next instruction.
            if let Some(outcome) = self.check_interrupts(bus) {
                return Ok(outcome);
            }

            let raw = match self.fetch_instruction(bus) {
                Ok(r) => r,
                Err(e) => return self.dispatch_trap(e),
            };

            let insn = match decode::decode(raw) {
                Ok(i) => i,
                Err(e) => return self.dispatch_trap(e),
            };

            let exec_result = match self.execute(&insn) {
                Ok(r) => r,
                Err(e) => return self.dispatch_trap(e),
            };

            // Handle ecall/ebreak before the memory stage.
            match exec_result {
                ExecResult::Ecall => return self.handle_ecall(bus),
                ExecResult::Ebreak => {
                    self.csrs.increment_instret();
                    self.csrs.increment_cycle();
                    // EBREAK should trap, not halt
                    self.take_trap(traps::CAUSE_EBREAK, 0);
                    return Ok(StepOutcome::Continue);
                }
                _ => {}
            }

            let mem_result = match self.memory_stage(exec_result, bus) {
                Ok(r) => r,
                Err(e) => return self.dispatch_trap(e),
            };

            let next_pc = match self.writeback(mem_result) {
                Ok(pc) => pc,
                Err(VmError::Ecall) => return self.handle_ecall(bus),
                Err(VmError::Ebreak) => {
                    self.csrs.increment_instret();
                    self.csrs.increment_cycle();
                    // EBREAK should trap
                    self.take_trap(traps::CAUSE_EBREAK, 0);
                    return Ok(StepOutcome::Continue);
                }
                Err(VmError::Mret) => {
                    let new_pc = traps::handle_mret(&mut self.regs, &mut self.csrs);
                    self.regs.pc = new_pc;
                    self.csrs.increment_instret();
                    self.csrs.increment_cycle();
                    return Ok(StepOutcome::Continue);
                }
                Err(VmError::Sret) => {
                    match traps::handle_sret(&mut self.regs, &mut self.csrs) {
                        Ok(new_pc) => {
                            self.regs.pc = new_pc;
                            self.csrs.increment_instret();
                            self.csrs.increment_cycle();
                            return Ok(StepOutcome::Continue);
                        }
                        Err(e) => return self.dispatch_trap(e),
                    }
                }
                Err(e) => return self.dispatch_trap(e),
            };

            self.regs.pc = next_pc;
            self.csrs.increment_instret();
            self.csrs.increment_cycle();
            Ok(StepOutcome::Continue)
        }

        // -----------------------------------------------------------------------
        // Public accessor methods
        // -----------------------------------------------------------------------

        pub fn peek_reg(&self, r: usize) -> u64 {
            self.regs.read_x(r)
        }

        pub fn peek_fp_reg(&self, r: usize) -> u64 {
            self.regs.read_f_bits(r)
        }

        pub fn peek_pc(&self) -> u64 {
            self.regs.pc
        }

        pub fn peek_csr_mcause(&self) -> u64 {
            self.csrs.mcause
        }

        pub fn peek_csr_mtvec(&self) -> u64 {
            self.csrs.mtvec
        }

        pub fn write_csr_mtvec(&mut self, val: u64) {
            self.csrs.mtvec = val;
        }

        pub fn peek_all_xregs(&self) -> [u64; 32] {
            std::array::from_fn(|i| self.regs.read_x(i))
        }

        pub fn peek_all_fregs(&self) -> [u64; 32] {
            std::array::from_fn(|i| self.regs.read_f_bits(i))
        }

        pub fn peek_csrs(&self) -> crate::virtual_machine::cpu::csr::CsrSnapshot {
            self.csrs.snapshot()
        }

        /// Set the return-address register (x1 / ra).
        pub fn set_return_addr(&mut self, ra: u64) {
            self.regs.write_x(1, ra);
        }
    } // end of impl Cpu
} // end of cpu_impl module
