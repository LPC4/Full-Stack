//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{HEAP_PTR_ADDR, RAM_BASE, RAM_SIZE_DEFAULT, SystemBus};
use crate::virtual_machine::cpu::Cpu;
pub use crate::virtual_machine::cpu::StepOutcome;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::linker::{self, LinkedProgram, LinkerConfig};
use crate::virtual_machine::memory::MemoryAccess;

pub struct RunResult {
    pub steps: u64,
    pub uart_output: String,
    pub outcome: StepOutcome,
}

pub struct VirtualMachine {
    cpu: Cpu,
    bus: SystemBus,
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl VirtualMachine {
    /// Create a VM from raw assembled output, linking it at the default base address.
    pub fn new(assembled: &AssembledOutput) -> Self {
        let program = linker::link(assembled, &LinkerConfig::default());
        Self::from_linked(&program)
    }

    /// Create a VM from an already-linked program image.
    pub fn from_linked(program: &LinkedProgram) -> Self {
        let mut bus = SystemBus::new(Vec::new());

        // Load the flat image into RAM.
        for (i, &byte) in program.bytes.iter().enumerate() {
            let _ = bus.write_byte(program.load_addr + i as u64, byte);
        }

        // Write the initial heap bump-pointer value.
        let _ = bus.write_doubleword(HEAP_PTR_ADDR, program.heap_base);

        // Stack pointer = top of RAM, 16-byte aligned.
        let stack_ptr = RAM_BASE + RAM_SIZE_DEFAULT as u64 - 16;
        let mut cpu = Cpu::new(program.entry_point, stack_ptr);

        // Set ra to the `exit` stub so that returning from `main` halts cleanly.
        if let Some(&exit_addr) = program.symbols.get("exit") {
            cpu.set_return_addr(exit_addr);
        }

        Self { cpu, bus }
    }
}

// ---------------------------------------------------------------------------
// Step / run
// ---------------------------------------------------------------------------

impl VirtualMachine {
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
}

// ---------------------------------------------------------------------------
// Peripheral / debug accessors
// ---------------------------------------------------------------------------

impl VirtualMachine {
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
