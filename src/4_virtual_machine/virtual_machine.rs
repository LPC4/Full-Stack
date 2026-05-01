//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{RAM_BASE, RAM_SIZE_DEFAULT, SystemBus};
use crate::virtual_machine::cpu::Cpu;
pub use crate::virtual_machine::cpu::StepOutcome;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

pub struct RunResult {
    pub steps: u64,
    pub uart_output: String,
    pub outcome: StepOutcome,
}

/// The top-level virtual machine that wires together the CPU and system bus.
pub struct VirtualMachine {
    cpu: Cpu,
    bus: SystemBus,
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

        // Stack pointer = top of RAM, 16-byte aligned
        let stack_ptr = RAM_BASE + RAM_SIZE_DEFAULT as u64 - 16;

        let cpu = Cpu::new(start_pc, stack_ptr);

        Self { cpu, bus }
    }

    // -----------------------------------------------------------------------
    // Step / run
    // -----------------------------------------------------------------------

    /// Execute a single instruction cycle.
    pub fn step(&mut self) -> Result<StepOutcome, VmError> {
        self.cpu.step(&mut self.bus)
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
                    eprintln!("[VM] step error at pc={:#x}: {e:?}", self.cpu.peek_pc());
                    steps += 1;
                    outcome = StepOutcome::Halted(-1);
                    break;
                }
            }
        }

        let uart_bytes = self.bus.uart_mut().drain_output();
        let uart_output = String::from_utf8_lossy(&uart_bytes).into_owned();

        RunResult {
            steps,
            uart_output,
            outcome,
        }
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
        self.cpu.peek_reg(r)
    }

    pub fn peek_fp_reg(&self, r: usize) -> u64 {
        self.cpu.peek_fp_reg(r)
    }

    pub fn peek_pc(&self) -> u64 {
        self.cpu.peek_pc()
    }

    pub fn peek_csr_mcause(&self) -> u64 {
        self.cpu.peek_csr_mcause()
    }

    pub fn peek_csr_mtvec(&self) -> u64 {
        self.cpu.peek_csr_mtvec()
    }

    pub fn write_csr_mtvec(&mut self, val: u64) {
        self.cpu.write_csr_mtvec(val);
    }
}
