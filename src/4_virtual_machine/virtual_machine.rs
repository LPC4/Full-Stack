//! Top-level virtual machine: ties together the CPU, memory bus, and pipeline.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{SystemBus, RAM_BASE, RAM_SIZE_DEFAULT};
use crate::virtual_machine::cpu::{
    csr::CsrFile,
    pipeline::{decode, execute, fetch, memory, writeback},
    registers::Registers,
};
use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

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

pub struct RunResult {
    pub steps: u64,
    pub uart_output: String,
    pub outcome: StepOutcome,
}

pub struct VirtualMachine {
    regs: Registers,
    csrs: CsrFile,
    bus: SystemBus,
    reservation: Option<u64>,
}

// ---------------------------------------------------------------------------
// impl VirtualMachine
// ---------------------------------------------------------------------------

impl VirtualMachine {
    pub fn new(assembled: &AssembledOutput) -> Self {
        let mut bus = SystemBus::new(Vec::new());

        // Load all section bytes into RAM at RAM_BASE in ELF layout order.
        let mut offset = 0u64;
        for byte in assembled.text_bytes() {
            let _ = bus.write_byte(RAM_BASE + offset, *byte);
            offset += 1;
        }
        for byte in assembled.rodata_bytes() {
            let _ = bus.write_byte(RAM_BASE + offset, *byte);
            offset += 1;
        }
        for byte in assembled.data_bytes() {
            let _ = bus.write_byte(RAM_BASE + offset, *byte);
            offset += 1;
        }
        // BSS: zero-initialized; write explicit zeros so the region is backed by RAM.
        for byte in assembled.bss_bytes() {
            let _ = bus.write_byte(RAM_BASE + offset, *byte);
            offset += 1;
        }

        // Compute entry point
        let start_pc = if assembled.symbol_table.contains_key("_start") {
            RAM_BASE + assembled.symbol_table["_start"]
        } else {
            RAM_BASE
        };

        let mut regs = Registers::new();
        regs.pc = start_pc;
        // Stack pointer = top of RAM, 16-byte aligned
        regs.write_x(2, RAM_BASE + RAM_SIZE_DEFAULT as u64 - 16);

        Self {
            regs,
            csrs: CsrFile::new(),
            bus,
            reservation: None,
        }
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

    // -----------------------------------------------------------------------
    // Per-step interrupt check
    // -----------------------------------------------------------------------

    /// Tick the CLINT timer, update MIP.MTIP, and take any pending interrupt
    /// whose enable bit is set and global MIE is active.
    /// Returns `Some(StepOutcome::Continue)` if an interrupt was taken.
    fn check_interrupts(&mut self) -> Option<StepOutcome> {
        // Advance CLINT time counter by one tick per instruction.
        self.bus.clint_mut().tick();
        if self.bus.clint_mut().timer_irq_pending() {
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

    // -----------------------------------------------------------------------
    // Step / run
    // -----------------------------------------------------------------------

    pub fn step(&mut self) -> Result<StepOutcome, VmError> {
        // Check for pending interrupts before fetching the next instruction.
        if let Some(outcome) = self.check_interrupts() {
            return Ok(outcome);
        }

        let pc = self.regs.pc;

        let raw = match fetch::fetch(&mut self.bus, pc) {
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        let insn = match decode::decode(raw) {
            Ok(i)  => i,
            Err(e) => return self.dispatch_trap(e),
        };

        let exec_result = match execute::execute(&insn, &self.regs, &self.csrs, pc) {
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        // Handle ecall/ebreak before the memory stage.
        match exec_result {
            ExecResult::Ecall  => return self.handle_ecall(),
            ExecResult::Ebreak => {
                self.csrs.increment_instret();
                self.csrs.increment_cycle();
                return Ok(StepOutcome::Halted(-2));
            }
            _ => {}
        }

        let mem_result = match memory::memory_stage(exec_result, &mut self.bus, &mut self.reservation) {
            Ok(r)  => r,
            Err(e) => return self.dispatch_trap(e),
        };

        let next_pc = match writeback::writeback(mem_result, &mut self.regs, &mut self.csrs) {
            Ok(pc)             => pc,
            Err(VmError::Ecall)  => return self.handle_ecall(),
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

    fn handle_ecall(&mut self) -> Result<StepOutcome, VmError> {
        let syscall = self.regs.read_x(17); // a7

        match syscall {
            // write(fd, buf, len)
            64 => {
                let len = self.regs.read_x(12) as usize;
                let buf = self.regs.read_x(11);
                let mut written = 0usize;
                for i in 0..len {
                    let byte = self.bus.read_byte(buf + i as u64).unwrap_or(0);
                    let _ = self.bus.uart_mut().write_byte(0, byte);
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

    pub fn run(&mut self, max_steps: u64) -> RunResult {
        let mut steps = 0u64;
        let mut outcome = StepOutcome::Continue;

        for _ in 0..max_steps {
            match self.step() {
                Ok(StepOutcome::Continue) => {
                    steps += 1;
                }
                Ok(StepOutcome::Halted(code)) => {
                    steps += 1;
                    outcome = StepOutcome::Halted(code);
                    break;
                }
                Err(e) => {
                    eprintln!("[VM] step error at pc={:#x}: {e:?}", self.regs.pc);
                    steps += 1;
                    outcome = StepOutcome::Halted(-1);
                    break;
                }
            }
        }

        let uart_bytes = self.bus.uart_mut().drain_output();
        let uart_output = String::from_utf8_lossy(&uart_bytes).into_owned();

        RunResult { steps, uart_output, outcome }
    }

    // -----------------------------------------------------------------------
    // Peripheral / debug accessors
    // -----------------------------------------------------------------------

    /// Feed a byte into the UART receive FIFO (simulates console input).
    pub fn uart_receive(&mut self, byte: u8) {
        self.bus.uart_mut().receive(byte);
    }

    pub fn uart_output(&mut self) -> Vec<u8> {
        self.bus.uart_mut().drain_output()
    }

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
