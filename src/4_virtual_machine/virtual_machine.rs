//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{
    ELF_LOAD_BASE, HEAP_PTR_ADDR, RAM_BASE, RAM_SIZE_DEFAULT, SystemBus,
};
use crate::virtual_machine::cpu::pipeline::TickOutcome;
use crate::virtual_machine::cpu::Cpu;
pub use crate::virtual_machine::cpu::StepOutcome;
pub use crate::virtual_machine::cpu::csr::CsrSnapshot;
pub use crate::virtual_machine::cpu::pipeline::{CpuPipelineFeed, PipelineStats, StageEntry};
use crate::virtual_machine::elf_parser::{ParsedElf, align_up};
use crate::virtual_machine::error::VmError;
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
    /// Create a VM from raw assembled output by exporting it to ELF and loading that image.
    pub fn new(assembled: &AssembledOutput) -> Self {
        let elf = assembled.to_elf(ELF_LOAD_BASE);
        Self::from_elf(&elf).unwrap_or_else(|e| panic!("failed to load assembled ELF: {e}"))
    }

    /// Create a VM from a complete ELF-64 image by mapping every PT_LOAD segment.
    pub fn from_elf(bytes: &[u8]) -> Result<Self, VmError> {
        let elf = ParsedElf::parse(bytes)?;

        let rom_image = crate::virtual_machine::rom::generate_rom_image();
        let mut bus = SystemBus::new(rom_image);

        let image_base = elf
            .load_segments
            .iter()
            .map(|segment| segment.vaddr)
            .min()
            .unwrap_or(RAM_BASE);

        let mut highest_mapped = RAM_BASE;
        for segment in &elf.load_segments {
            if segment.mem_size < segment.file_size {
                return Err(VmError::Other(format!(
                    "ELF PT_LOAD segment at {:#x} has file size {} larger than memory size {}",
                    segment.vaddr, segment.file_size, segment.mem_size
                )));
            }

            let mapped_vaddr = RAM_BASE + (segment.vaddr - image_base);
            highest_mapped = highest_mapped.max(mapped_vaddr + segment.mem_size);

            let file_end = segment
                .offset
                .checked_add(segment.file_size)
                .ok_or_else(|| VmError::Other("ELF segment file range overflow".to_string()))?;
            let data = bytes
                .get(segment.offset as usize..file_end as usize)
                .ok_or_else(|| VmError::Other("ELF PT_LOAD range outside file".to_string()))?;

            for (i, &byte) in data.iter().enumerate() {
                bus.write_byte(mapped_vaddr + i as u64, byte)?;
            }

            if segment.mem_size > segment.file_size {
                for addr in (mapped_vaddr + segment.file_size)..(mapped_vaddr + segment.mem_size) {
                    bus.write_byte(addr, 0)?;
                }
            }
        }

        let heap_base = align_up(highest_mapped, 0x1000);
        bus.write_doubleword(HEAP_PTR_ADDR, heap_base)?;

        bus.cold_cache_reset();

        let stack_ptr = RAM_BASE + RAM_SIZE_DEFAULT as u64 - 16;
        let cpu = Cpu::new(RAM_BASE + (elf.entry_point - image_base), stack_ptr);

        Ok(Self { cpu, bus })
    }
}

// ---------------------------------------------------------------------------
// Step / run
// ---------------------------------------------------------------------------

impl VirtualMachine {
    pub fn step(&mut self) -> Result<StepOutcome, VmError> {
        match self.cpu.tick(&mut self.bus)? {
            TickOutcome::Continue | TickOutcome::EcallSquash => {
                if let Some(code) = self.bus.take_syscon_exit() {
                    return Ok(StepOutcome::Halted(code));
                }
                Ok(StepOutcome::Continue)
            }
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
        self.cpu.last_cycle()
    }

    /// Cumulative pipeline performance stats.
    pub fn pipeline_stats(&self) -> &PipelineStats {
        self.cpu.stats()
    }

    /// Total instructions retired through WB since reset.
    pub fn insns_retired(&self) -> u64 {
        self.cpu.stats().insns_retired
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

    /// Lightweight stats-only read — no allocation. Called every step.
    pub fn get_cache_stats(
        &self,
    ) -> (
        crate::virtual_machine::memory::cache::CacheStats,
        crate::virtual_machine::memory::cache::CacheStats,
        crate::virtual_machine::memory::cache::CacheStats,
    ) {
        self.bus.get_cache_stats()
    }

    /// Full snapshots including per-line state — only call when the cache view renders.
    pub fn get_cache_snapshots(
        &self,
    ) -> (
        crate::virtual_machine::memory::cache::CacheSnapshot,
        crate::virtual_machine::memory::cache::CacheSnapshot,
        crate::virtual_machine::memory::cache::CacheSnapshot,
    ) {
        self.bus.get_cache_snapshots()
    }
}
