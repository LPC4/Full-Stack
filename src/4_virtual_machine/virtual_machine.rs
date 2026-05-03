//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{HEAP_PTR_ADDR, RAM_BASE, RAM_SIZE_DEFAULT, SystemBus};
use crate::virtual_machine::cpu::PipelinedCpu;
pub use crate::virtual_machine::cpu::StepOutcome;
pub use crate::virtual_machine::cpu::csr::CsrSnapshot;
pub use crate::virtual_machine::cpu::pipelined::{CpuPipelineFeed, PipelineStats, StageEntry};
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::linker::{self, LinkedProgram, LinkerConfig};
use crate::virtual_machine::memory::MemoryAccess;

pub struct RunResult {
    pub steps: u64,
    pub uart_output: String,
    pub outcome: StepOutcome,
}

pub struct VirtualMachine {
    cpu: PipelinedCpu,
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
        let mut cpu = PipelinedCpu::new(program.entry_point, stack_ptr);

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
        use crate::virtual_machine::cpu::pipelined::TickOutcome;
        match self.cpu.tick(&mut self.bus)? {
            TickOutcome::Continue => Ok(StepOutcome::Continue),
            TickOutcome::Halted(code) => Ok(StepOutcome::Halted(code)),
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

    /// Raw pipeline stage feed from the most recent tick.
    pub fn pipeline_snapshot(&self) -> &CpuPipelineFeed {
        &self.cpu.last_cycle
    }

    /// Cumulative pipeline performance stats.
    pub fn pipeline_stats(&self) -> &PipelineStats {
        &self.cpu.stats
    }

    // ---------------------------------------------------------------------------
    // Bulk debug accessors
    // ---------------------------------------------------------------------------

    pub fn peek_all_xregs(&self) -> [u64; 32] {
        self.cpu.peek_all_xregs()
    }

    pub fn peek_all_fregs(&self) -> [u64; 32] {
        self.cpu.peek_all_fregs()
    }

    pub fn peek_csrs(&self) -> CsrSnapshot {
        self.cpu.peek_csrs()
    }

    /// Read up to `len` bytes from the address space starting at `addr`.
    /// Unroutable addresses produce 0x00 bytes.
    pub fn peek_bytes(&mut self, addr: u64, len: usize) -> Vec<u8> {
        self.bus.peek_bytes(addr, len)
    }

    pub fn push_uart_rx(&mut self, byte: u8) {
        self.bus.uart_mut().receive(byte);
    }

    pub fn drain_uart_output(&mut self) -> Vec<u8> {
        self.bus.uart_mut().drain_output()
    }

    /// Get cache statistics for all three levels (L1, L2, L3)
    pub fn get_cache_stats(
        &self,
    ) -> (
        crate::virtual_machine::memory::cache::CacheStats,
        crate::virtual_machine::memory::cache::CacheStats,
        crate::virtual_machine::memory::cache::CacheStats,
    ) {
        self.bus.get_cache_stats()
    }
}
