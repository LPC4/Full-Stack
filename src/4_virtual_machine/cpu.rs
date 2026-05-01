pub mod alu;
pub mod csr;
pub mod decoder;
pub mod mmu;
pub mod pipeline;
pub mod registers;

// Re-export Cpu and StepOutcome for external use
pub use cpu_impl::{Cpu, StepOutcome};

mod cpu_impl {

use crate::virtual_machine::bus::SystemBus;
use crate::virtual_machine::cpu::csr::CsrFile;
use crate::virtual_machine::cpu::pipeline::{decode, execute, fetch, memory, writeback};
use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
use crate::virtual_machine::cpu::registers::Registers;
use crate::virtual_machine::cpu::decoder::DecodedInsn;
use crate::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// mcause constants
// ---------------------------------------------------------------------------

const CAUSE_INSN_ACCESS_FAULT:  u64 = 1;
const CAUSE_ILLEGAL_INSN:       u64 = 2;
const CAUSE_LOAD_ACCESS_FAULT:  u64 = 5;
const CAUSE_STORE_ACCESS_FAULT: u64 = 7;

// Interrupt causes have bit 63 set.
const CAUSE_M_SOFTWARE_IRQ: u64 = (1u64 << 63) | 3;
const CAUSE_M_TIMER_IRQ:    u64 = (1u64 << 63) | 7;
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
        regs.write_x(2, stack_ptr);   // sp

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
        fetch::fetch(bus, self.regs.pc)
    }

    /// Execute a decoded instruction.
    fn execute(&self, insn: &DecodedInsn) -> Result<ExecResult, VmError> {
        execute::execute(insn, &self.regs, &self.csrs, self.regs.pc)
    }

    /// Perform memory operations for Load/Store/Atomic instructions.
    fn memory_stage(&mut self, result: ExecResult, bus: &mut SystemBus) -> Result<memory::MemResult, VmError> {
        memory::memory_stage(result, bus, &mut self.reservation)
    }

    /// Write back results to registers and CSRs.
    fn writeback(&mut self, result: memory::MemResult) -> Result<u64, VmError> {
        writeback::writeback(result, &mut self.regs, &mut self.csrs)
    }

    // -----------------------------------------------------------------------
    // Trap / interrupt handling
    // -----------------------------------------------------------------------

    /// Enter a machine-mode trap: saves state, updates CSRs, jumps to handler.
    fn take_trap(&mut self, cause: u64, tval: u64) {
        let pc = self.regs.pc;
        self.csrs.mepc  = pc & !0x3u64;
        self.csrs.mcause = cause;
        self.csrs.mtval  = tval;

        // Save MIE → MPIE; clear MIE; set MPP = 3 (M-mode).
        let mie_bit = (self.csrs.mstatus >> 3) & 1;
        self.csrs.mstatus &= !(1u64 << 7);  // clear MPIE
        self.csrs.mstatus |= mie_bit << 7;   // MPIE = old MIE
        self.csrs.mstatus &= !(1u64 << 3);  // clear MIE
        self.csrs.mstatus |= 3u64 << 11;    // MPP = 3 (M-mode)

        let mtvec = self.csrs.mtvec;
        let mode  = mtvec & 0x3;
        let base  = mtvec & !0x3u64;

        // Vectored mode (1): jump to base + 4*cause for interrupts, base for exceptions.
        self.regs.pc = if mode == 1 && (cause & (1u64 << 63)) != 0 {
            let idx = cause & !(1u64 << 63);
            base + 4 * idx
        } else {
            base
        };
    }

    /// Map a `VmError` to a trap cause/tval pair, invoke the handler (if installed),
    /// or return the original error if no handler is available.
    fn dispatch_trap(&mut self, e: VmError) -> Result<StepOutcome, VmError> {
        let (cause, tval) = match &e {
            VmError::InstructionAccessFault(addr) => (CAUSE_INSN_ACCESS_FAULT, *addr),
            VmError::IllegalInstruction(insn)     => (CAUSE_ILLEGAL_INSN, *insn as u64),
            VmError::LoadAccessFault(addr)
            | VmError::BusError(addr)              => (CAUSE_LOAD_ACCESS_FAULT, *addr),
            VmError::StoreAccessFault(addr)        => (CAUSE_STORE_ACCESS_FAULT, *addr),
            _ => return Err(e),
        };

        // Always record diagnostic CSRs.
        self.csrs.mcause = cause;
        self.csrs.mepc   = self.regs.pc & !0x3u64;
        self.csrs.mtval  = tval;

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
            self.csrs.mip |= 1u64 << 7;  // set MTIP
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
        use crate::virtual_machine::memory::MemoryAccess;

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
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        let insn = match decode::decode(raw) {
            Ok(i)  => i,
            Err(e) => return self.dispatch_trap(e),
        };

        let exec_result = match self.execute(&insn) {
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        // Handle ecall/ebreak before the memory stage.
        match exec_result {
            ExecResult::Ecall  => return self.handle_ecall(bus),
            ExecResult::Ebreak => {
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                return Ok(StepOutcome::Halted(-2));
            }
            _ => {}
        }

        let mem_result = match self.memory_stage(exec_result, bus) {
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        let next_pc = match self.writeback(mem_result) {
            Ok(pc)             => pc,
            Err(VmError::Ecall)  => return self.handle_ecall(bus),
            Err(VmError::Ebreak) => {
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                return Ok(StepOutcome::Halted(-2));
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
