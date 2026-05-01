pub mod alu;
pub mod csr;
pub mod decoder;
pub mod mmu;
pub mod pipeline;
pub mod registers;

// Re-export Cpu, StepOutcome, and PrivilegeMode for external use
pub use cpu_impl::{Cpu, StepOutcome};
pub use registers::PrivilegeMode;

mod cpu_impl {
    use crate::virtual_machine::bus::SystemBus;
    use crate::virtual_machine::cpu::csr::CsrFile;
    use crate::virtual_machine::cpu::decoder::DecodedInsn;
    use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
    use crate::virtual_machine::cpu::pipeline::{decode, execute, fetch, memory, writeback};
    use crate::virtual_machine::cpu::registers::Registers;
    use crate::virtual_machine::error::VmError;

    // ---------------------------------------------------------------------------
    // mcause constants
    // ---------------------------------------------------------------------------

    const CAUSE_INSN_ACCESS_FAULT: u64 = 1;
    const CAUSE_ILLEGAL_INSN: u64 = 2;
    const CAUSE_EBREAK: u64 = 3;
    const CAUSE_LOAD_ACCESS_FAULT: u64 = 5;
    const CAUSE_STORE_ACCESS_FAULT: u64 = 7;
    const CAUSE_ECALL_U: u64 = 8;
    const CAUSE_ECALL_S: u64 = 9;
    const CAUSE_ECALL_M: u64 = 11;
    #[allow(dead_code)]
    const CAUSE_PAGE_FAULT_INST: u64 = 12;
    const CAUSE_PAGE_FAULT_LOAD: u64 = 13;
    #[allow(dead_code)]
    const CAUSE_PAGE_FAULT_STORE: u64 = 15;

    // Interrupt causes have bit 63 set.
    const CAUSE_M_SOFTWARE_IRQ: u64 = (1u64 << 63) | 3;
    const CAUSE_M_TIMER_IRQ: u64 = (1u64 << 63) | 7;
    const CAUSE_M_EXTERNAL_IRQ: u64 = (1u64 << 63) | 11;

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
        /// This handles both M-mode and S-mode traps.
        fn take_trap(&mut self, cause: u64, tval: u64) {
            use crate::virtual_machine::cpu::registers::PrivilegeMode;

            let pc = self.regs.pc;
            let current_priv = self.regs.priv_mode;

            // Determine target mode and CSRs based on current privilege
            match current_priv {
                PrivilegeMode::Machine => {
                    // M-mode trap
                    self.csrs.mepc = pc & !0x3u64;
                    self.csrs.mcause = cause;
                    self.csrs.mtval = tval;

                    // Save MIE → MPIE; clear MIE; set MPP
                    let mie_bit = (self.csrs.mstatus >> 3) & 1;
                    self.csrs.mstatus &= !(1u64 << 7); // clear MPIE
                    self.csrs.mstatus |= mie_bit << 7; // MPIE = old MIE
                    self.csrs.mstatus &= !(1u64 << 3); // clear MIE

                    // Set MPP based on current privilege
                    self.csrs.mstatus &= !(0x3u64 << 11);
                    self.csrs.mstatus |= (current_priv as u64) << 11;

                    // Jump to mtvec
                    let mtvec = self.csrs.mtvec;
                    let mode = mtvec & 0x3;
                    let base = mtvec & !0x3u64;

                    self.regs.pc = if mode == 1 && (cause & (1u64 << 63)) != 0 {
                        let idx = cause & !(1u64 << 63);
                        base + 4 * idx
                    } else {
                        base
                    };

                    // Switch to M-mode
                    self.regs.priv_mode = PrivilegeMode::Machine;
                }
                PrivilegeMode::Supervisor | PrivilegeMode::User => {
                    // Trap to M-mode (we don't implement delegation to S-mode yet)
                    self.csrs.mepc = pc & !0x3u64;
                    self.csrs.mcause = cause;
                    self.csrs.mtval = tval;

                    // Save MIE → MPIE; clear MIE; set MPP
                    let mie_bit = (self.csrs.mstatus >> 3) & 1;
                    self.csrs.mstatus &= !(1u64 << 7); // clear MPIE
                    self.csrs.mstatus |= mie_bit << 7; // MPIE = old MIE
                    self.csrs.mstatus &= !(1u64 << 3); // clear MIE

                    // Set MPP based on current privilege
                    self.csrs.mstatus &= !(0x3u64 << 11);
                    self.csrs.mstatus |= (current_priv as u64) << 11;

                    // Jump to mtvec
                    let mtvec = self.csrs.mtvec;
                    let mode = mtvec & 0x3;
                    let base = mtvec & !0x3u64;

                    self.regs.pc = if mode == 1 && (cause & (1u64 << 63)) != 0 {
                        let idx = cause & !(1u64 << 63);
                        base + 4 * idx
                    } else {
                        base
                    };

                    // Switch to M-mode
                    self.regs.priv_mode = PrivilegeMode::Machine;
                }
            }
        }

        /// Map a `VmError` to a trap cause/tval pair, invoke the handler (if installed),
        /// or return the original error if no handler is available.
        fn dispatch_trap(&mut self, e: VmError) -> Result<StepOutcome, VmError> {
            let (cause, tval) = match &e {
                VmError::InstructionAccessFault(addr) => (CAUSE_INSN_ACCESS_FAULT, *addr),
                VmError::IllegalInstruction(insn) => (CAUSE_ILLEGAL_INSN, *insn as u64),
                VmError::LoadAccessFault(addr) | VmError::BusError(addr) => {
                    (CAUSE_LOAD_ACCESS_FAULT, *addr)
                }
                VmError::StoreAccessFault(addr) => (CAUSE_STORE_ACCESS_FAULT, *addr),
                VmError::PageFault(addr) => {
                    // Determine page fault type based on context
                    // For simplicity, default to load page fault
                    // In a real implementation, we'd track the access type
                    (CAUSE_PAGE_FAULT_LOAD, *addr)
                }
                _ => return Err(e),
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
                CAUSE_M_EXTERNAL_IRQ
            } else if pending & (1 << 3) != 0 {
                CAUSE_M_SOFTWARE_IRQ
            } else if pending & (1 << 7) != 0 {
                CAUSE_M_TIMER_IRQ
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
            use crate::virtual_machine::memory::MemoryAccess;

            // In S-mode or U-mode, ecall should trap to M-mode
            if self.regs.priv_mode != PrivilegeMode::Machine {
                let cause = match self.regs.priv_mode {
                    PrivilegeMode::User => CAUSE_ECALL_U,
                    PrivilegeMode::Supervisor => CAUSE_ECALL_S,
                    PrivilegeMode::Machine => CAUSE_ECALL_M, // shouldn't reach here
                };
                self.take_trap(cause, 0);
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                return Ok(StepOutcome::Continue);
            }

            // M-mode ecall - handle as syscall
            let syscall = self.regs.read_x(17); // a7

            match syscall {
                // write(fd, buf, len)
                64 => {
                    let len = self.regs.read_x(12) as usize;
                    let buf = self.regs.read_x(11);
                    let mut written = 0usize;
                    for i in 0..len {
                        let byte = bus.read_byte(buf + i as u64).unwrap_or(0);
                        let _ = bus.uart_mut().write_byte(0, byte);
                        written += 1;
                    }
                    self.regs.write_x(10, written as u64);
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.csrs.increment_instret();
                    self.csrs.increment_cycle();
                    Ok(StepOutcome::Continue)
                }
                // exit / exit_group
                93 | 94 => {
                    let exit_code = self.regs.read_x(10) as i64;
                    Ok(StepOutcome::Halted(exit_code))
                }
                // Unknown syscall — return -1.
                _ => {
                    self.regs.write_x(10, u64::MAX);
                    self.regs.pc = self.regs.pc.wrapping_add(4);
                    self.csrs.increment_instret();
                    self.csrs.increment_cycle();
                    Ok(StepOutcome::Continue)
                }
            }
        }

        /// Handle MRET instruction - return from machine-mode trap
        fn handle_mret(&mut self) -> StepOutcome {
            use crate::virtual_machine::cpu::registers::PrivilegeMode;

            // Restore MIE from MPIE
            let mpie = (self.csrs.mstatus >> 7) & 1;
            self.csrs.mstatus &= !(1u64 << 3); // clear MIE
            self.csrs.mstatus |= mpie << 3; // MIE = old MPIE

            // Set MPIE to 1
            self.csrs.mstatus |= 1u64 << 7;

            // Restore previous privilege mode from MPP
            let mpp = (self.csrs.mstatus >> 11) & 0x3;
            let prev_priv = match mpp {
                0 => PrivilegeMode::User,
                1 => PrivilegeMode::Supervisor,
                3 => PrivilegeMode::Machine,
                _ => PrivilegeMode::Machine, // default to M-mode
            };

            self.regs.priv_mode = prev_priv;

            // Set MPP to User mode (least privileged)
            self.csrs.mstatus &= !(0x3u64 << 11);
            self.csrs.mstatus |= 0u64 << 11;

            // Jump to MEPC
            self.regs.pc = self.csrs.mepc;

            self.csrs.increment_instret();
            self.csrs.increment_cycle();
            StepOutcome::Continue
        }

        /// Handle SRET instruction - return from supervisor-mode trap
        fn handle_sret(&mut self) -> Result<StepOutcome, VmError> {
            use crate::virtual_machine::cpu::registers::PrivilegeMode;

            // Check if SRET is allowed (not in U-mode)
            if self.regs.priv_mode == PrivilegeMode::User {
                return Err(VmError::IllegalInstruction(0x102));
            }

            // For now, we'll implement basic SRET similar to MRET but using sstatus/sepc
            // In a full implementation, this would use sstatus fields

            // Restore previous privilege mode
            // For simplicity, assume returning to U-mode
            self.regs.priv_mode = PrivilegeMode::User;

            // Jump to SEPC
            self.regs.pc = self.csrs.sepc;

            self.csrs.increment_instret();
            self.csrs.increment_cycle();
            Ok(StepOutcome::Continue)
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
                    self.take_trap(CAUSE_EBREAK, 0);
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
                    self.take_trap(CAUSE_EBREAK, 0);
                    return Ok(StepOutcome::Continue);
                }
                Err(VmError::Other(msg)) if msg == "MRET" => {
                    return Ok(self.handle_mret());
                }
                Err(VmError::Other(msg)) if msg == "SRET" => {
                    return self.handle_sret();
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
    }
} // end of cpu_impl module
