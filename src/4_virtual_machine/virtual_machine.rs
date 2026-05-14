//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{
    ELF_LOAD_BASE, HEAP_PTR_ADDR, RAM_BASE, RAM_SIZE_DEFAULT, SystemBus,
};
use crate::virtual_machine::cpu::PipelinedCpu;
pub use crate::virtual_machine::cpu::StepOutcome;
pub use crate::virtual_machine::cpu::csr::CsrSnapshot;
pub use crate::virtual_machine::cpu::pipelined::{CpuPipelineFeed, PipelineStats, StageEntry};
use crate::virtual_machine::error::VmError;
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
    /// Create a VM from raw assembled output by exporting it to ELF and loading that image.
    pub fn new(assembled: &AssembledOutput) -> Self {
        let elf = assembled.to_elf(ELF_LOAD_BASE);
        Self::from_elf(&elf).unwrap_or_else(|e| panic!("failed to load assembled ELF: {e}"))
    }

    /// Create a VM from a complete ELF-64 image by mapping every PT_LOAD segment.
    pub fn from_elf(bytes: &[u8]) -> Result<Self, VmError> {
        let elf = ParsedElf::parse(bytes)?;
        let mut bus = SystemBus::new(Vec::new());

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

        // Write the initial heap bump-pointer value.
        let heap_base = align_up(highest_mapped, 0x1000);
        bus.write_doubleword(HEAP_PTR_ADDR, heap_base)?;

        // Reset caches to cold state: the loader writes directly to physical memory on real
        // hardware, so caches start empty. Stats and line states from loading are discarded.
        bus.cold_cache_reset();

        // Stack pointer = top of RAM, 16-byte aligned.
        let stack_ptr = RAM_BASE + RAM_SIZE_DEFAULT as u64 - 16;
        let cpu = PipelinedCpu::new(RAM_BASE + (elf.entry_point - image_base), stack_ptr);

        Ok(Self { cpu, bus })
    }
}

// ---------------------------------------------------------------------------
// Step / run
// ---------------------------------------------------------------------------

impl VirtualMachine {
    pub fn step(&mut self) -> Result<StepOutcome, VmError> {
        use crate::virtual_machine::cpu::pipelined::TickOutcome;
        match self.cpu.tick(&mut self.bus)? {
            TickOutcome::Continue | TickOutcome::EcallSquash => Ok(StepOutcome::Continue),
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

#[derive(Debug)]
struct ParsedElf {
    entry_point: u64,
    load_segments: Vec<ElfLoadSegment>,
}

#[derive(Debug)]
struct ElfLoadSegment {
    offset: u64,
    vaddr: u64,
    file_size: u64,
    mem_size: u64,
}

impl ParsedElf {
    fn parse(bytes: &[u8]) -> Result<Self, VmError> {
        const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
        const ELFCLASS64: u8 = 2;
        const ELFDATA2LSB: u8 = 1;
        const PT_LOAD: u32 = 1;

        if bytes.len() < 64 {
            return Err(VmError::Other("ELF image is too small".to_string()));
        }
        if &bytes[0..4] != ELF_MAGIC {
            return Err(VmError::Other("ELF magic header missing".to_string()));
        }
        if bytes[4] != ELFCLASS64 {
            return Err(VmError::Other("ELF image is not 64-bit".to_string()));
        }
        if bytes[5] != ELFDATA2LSB {
            return Err(VmError::Other("ELF image is not little-endian".to_string()));
        }

        let entry_point = read_u64(bytes, 24)?;
        let phoff = read_u64(bytes, 32)?;
        let phentsize = read_u16(bytes, 54)? as u64;
        let phnum = read_u16(bytes, 56)? as u64;

        if phentsize < 56 {
            return Err(VmError::Other(
                "ELF program header size is invalid".to_string(),
            ));
        }

        let mut load_segments = Vec::new();
        for i in 0..phnum {
            let base = phoff
                .checked_add(i * phentsize)
                .ok_or_else(|| VmError::Other("ELF program header overflow".to_string()))?;
            let ph = base as usize;
            let end = ph
                .checked_add(phentsize as usize)
                .ok_or_else(|| VmError::Other("ELF program header slice overflow".to_string()))?;
            let header = bytes
                .get(ph..end)
                .ok_or_else(|| VmError::Other("ELF program header outside file".to_string()))?;

            let p_type = read_u32(header, 0)?;
            if p_type != PT_LOAD {
                continue;
            }

            load_segments.push(ElfLoadSegment {
                offset: read_u64(header, 8)?,
                vaddr: read_u64(header, 16)?,
                file_size: read_u64(header, 32)?,
                mem_size: read_u64(header, 40)?,
            });
        }

        if load_segments.is_empty() {
            return Err(VmError::Other(
                "ELF contains no PT_LOAD segments".to_string(),
            ));
        }

        Ok(Self {
            entry_point,
            load_segments,
        })
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    (value + alignment - 1) & !(alignment - 1)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, VmError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, VmError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, VmError> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
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

    /// Total instructions retired through WB since reset.
    pub fn insns_retired(&self) -> u64 {
        self.cpu.stats.insns_retired
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
