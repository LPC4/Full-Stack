//! Top-level virtual machine: ties together the CPU and memory bus.

use crate::bus::{
    DeviceState, ELF_LOAD_BASE, HEAP_PTR_ADDR, RAM_BASE, RAM_SIZE_DEFAULT, SystemBus,
};
use crate::cpu::Cpu;
pub use crate::cpu::StepOutcome;
pub use crate::cpu::csr::CsrSnapshot;
use crate::cpu::pipeline::TickOutcome;
pub use crate::cpu::pipeline::{CpuPipelineFeed, PipelineStats, StageEntry};
use crate::elf_parser::{ParsedElf, align_up};
use crate::error::VmError;
use crate::memory::MemoryAccess as _;
use asm_to_binary::AssembledOutput;

pub struct RunResult {
    pub steps: u64,
    pub uart_output: String,
    pub outcome: StepOutcome,
}

pub struct VirtualMachine {
    cpu: Cpu,
    bus: SystemBus,
}

/// A captured architectural state: CPU + MMIO devices (small) plus the RAM image
/// (the bulk term, delta-compressed by the session via `without_ram`/`with_ram`).
#[derive(Clone)]
pub struct VmSnapshot {
    cpu: Cpu,
    devices: DeviceState,
    ram: Vec<u8>,
}

impl VmSnapshot {
    /// The architectural RAM image at capture time.
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// This snapshot with the RAM image dropped (CPU + devices only).
    pub fn without_ram(&self) -> Self {
        Self {
            cpu: self.cpu.clone(),
            devices: self.devices.clone(),
            ram: Vec::new(),
        }
    }

    /// This snapshot with `ram` as its RAM image, ready for [`VirtualMachine::restore`].
    pub fn with_ram(&self, ram: Vec<u8>) -> Self {
        Self {
            cpu: self.cpu.clone(),
            devices: self.devices.clone(),
            ram,
        }
    }
}

// --- Constructors ---

impl VirtualMachine {
    /// Create a VM from raw assembled output by exporting it to ELF and loading that image.
    pub fn new(assembled: &AssembledOutput) -> Self {
        let elf = assembled.to_elf(ELF_LOAD_BASE);
        Self::from_elf(&elf).unwrap_or_else(|e| panic!("failed to load assembled ELF: {e}"))
    }

    /// Create a VM from a kernel assembled output.
    ///
    /// Loads the kernel ELF into RAM at `RAM_BASE`, then resets the CPU to
    /// `ROM_BASE` (0x0) so the ROM `_start` boot stub runs first.  `_start`
    /// sets up PMP + delegation and `mret`s into S-mode at `RAM_BASE`.
    pub fn new_kernel(assembled: &AssembledOutput) -> Self {
        let elf = assembled.to_elf_with_entry(ELF_LOAD_BASE, "_kernel_start");
        let mut vm =
            Self::from_elf(&elf).unwrap_or_else(|e| panic!("failed to load kernel ELF: {e}"));
        let entry = vm.cpu.peek_pc(); // ELF entry point (_kernel_start) resolved by from_elf
        vm.cpu.reset_pc(crate::rom::ROM_BASE);
        vm.cpu.set_boot_entry(entry); // a0 = _kernel_start; ROM _start does `csrw mepc, a0`
        vm
    }

    /// Create a VM from a complete ELF-64 image by mapping every `PT_LOAD` segment.
    pub fn from_elf(bytes: &[u8]) -> Result<Self, VmError> {
        let elf = ParsedElf::parse(bytes)?;

        let rom_image = crate::rom::generate_rom_image();
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
                .ok_or_else(|| VmError::Other("ELF segment file range overflow".to_owned()))?;
            let data = bytes
                .get(segment.offset as usize..file_end as usize)
                .ok_or_else(|| VmError::Other("ELF PT_LOAD range outside file".to_owned()))?;

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

// --- Step / run ---

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

// --- Peripheral / debug accessors ---

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

    // --- Bulk debug accessors ---

    pub fn peek_all_xregs(&self) -> [u64; 32] {
        self.cpu.peek_all_xregs()
    }

    pub fn peek_all_fregs(&self) -> [u64; 32] {
        self.cpu.peek_all_fregs()
    }

    pub fn peek_csrs(&self) -> CsrSnapshot {
        self.cpu.peek_csrs()
    }

    /// Debug translation: read-only Sv39 walk in the current CPU context,
    /// skipping permission checks and A/D updates. M-mode and Bare are identity.
    pub fn debug_translate(&self, vaddr: u64) -> Option<u64> {
        let csrs = self.cpu.peek_csrs();
        let mode = (csrs.satp >> 60) & 0xF;
        if self.cpu.peek_priv_mode() == crate::cpu::PrivilegeMode::Machine || mode == 0 {
            return Some(vaddr);
        }
        if mode != 8 {
            return None;
        }
        debug_walk_sv39(&self.bus, csrs.satp, vaddr)
    }

    /// Read `len` bytes of guest memory at virtual `addr` through the current
    /// translation context, chunked per page. Unmapped pages read as zeros.
    pub fn peek_bytes_virtual(&self, addr: u64, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut va = addr;
        let mut remaining = len;
        while remaining > 0 {
            let page_rest = (0x1000 - (va & 0xFFF)) as usize;
            let chunk = page_rest.min(remaining);
            match self.debug_translate(va) {
                Some(pa) => out.extend(self.bus.peek_bytes_raw(pa, chunk)),
                None => out.resize(out.len() + chunk, 0u8),
            }
            va = va.wrapping_add(chunk as u64);
            remaining -= chunk;
        }
        out
    }

    /// Read up to `len` bytes from the address space starting at `addr`.
    /// Unroutable addresses produce 0x00 bytes.
    pub fn peek_bytes(&mut self, addr: u64, len: usize) -> Vec<u8> {
        self.bus.peek_bytes(addr, len)
    }

    /// Like `peek_bytes` but only needs `&self` (no cache stat side effects).
    /// Reads ROM/MMIO directly and bypasses the cache hierarchy for RAM,
    /// so the debug view always reflects physical memory. Safe to call every frame.
    pub fn peek_bytes_raw(&self, addr: u64, len: usize) -> Vec<u8> {
        self.bus.peek_bytes_raw(addr, len)
    }

    /// Borrow the framebuffer device's pixel buffer for display.
    pub fn peek_framebuffer(&self) -> &[u8] {
        self.bus.peek_framebuffer()
    }

    /// Number of full-screen FILL clears performed (a per-frame counter for benches).
    pub fn framebuffer_fill_count(&self) -> u64 {
        self.bus.framebuffer_fill_count()
    }

    /// Push a key event from the host GUI into the keyboard MMIO device.
    pub fn keyboard_push(&mut self, scancode: u16, pressed: bool) {
        self.bus.keyboard_push(scancode, pressed);
    }

    /// Write raw bytes into physical RAM at `addr`.  Used to inject
    /// pre-compiled user binaries alongside a loaded kernel image.
    /// Flushes the L1/L2/L3 cache hierarchy afterward so subsequent
    /// CPU reads see the new data.
    pub fn write_ram(&mut self, addr: u64, data: &[u8]) -> Result<(), VmError> {
        for (i, &byte) in data.iter().enumerate() {
            self.bus.write_byte(addr + i as u64, byte)?;
        }
        self.bus.cold_cache_reset();
        Ok(())
    }

    /// Capture the full architectural state (CPU, MMIO devices, RAM image) for
    /// time travel. RAM is the bulk term; callers may delta-compress it.
    pub fn snapshot(&self) -> VmSnapshot {
        VmSnapshot {
            cpu: self.cpu.clone(),
            devices: self.bus.device_state(),
            ram: self.bus.ram_image(),
        }
    }

    /// Restore a previously captured state, dropping all caches cold so the
    /// restored RAM is authoritative.
    pub fn restore(&mut self, snap: &VmSnapshot) {
        self.cpu = snap.cpu.clone();
        self.bus.restore_device_state(&snap.devices);
        self.bus.load_ram_image(&snap.ram);
    }

    pub fn push_uart_rx(&mut self, byte: u8) {
        self.bus.uart_mut().receive(byte);
    }

    pub fn drain_uart_output(&mut self) -> Vec<u8> {
        self.bus.uart_mut().drain_output()
    }

    /// Lightweight stats-only read - no allocation. Called every step.
    pub fn get_cache_stats(
        &self,
    ) -> (
        crate::memory::cache::CacheStats,
        crate::memory::cache::CacheStats,
        crate::memory::cache::CacheStats,
    ) {
        self.bus.get_cache_stats()
    }

    /// Full snapshots including per-line state - only call when the cache view renders.
    pub fn get_cache_snapshots(
        &self,
    ) -> (
        crate::memory::cache::CacheSnapshot,
        crate::memory::cache::CacheSnapshot,
        crate::memory::cache::CacheSnapshot,
    ) {
        self.bus.get_cache_snapshots()
    }
}

/// Read-only Sv39 walk for the debugger: resolves `vaddr` under the table
/// `satp` points at, ignoring R/W/X/U permissions. `None` when unmapped.
fn debug_walk_sv39(bus: &SystemBus, satp: u64, vaddr: u64) -> Option<u64> {
    let upper = vaddr >> 39;
    if upper != 0 && upper != 0x1FF_FFFF {
        return None; // Not Sv39-canonical.
    }
    const PPN_MASK: u64 = (1u64 << 44) - 1;
    let mut table = (satp & PPN_MASK) << 12;
    for level in (0..3u64).rev() {
        let shift = 12 + 9 * level;
        let vpn = (vaddr >> shift) & 0x1FF;
        let pte = u64::from_le_bytes(bus.peek_bytes_raw(table + vpn * 8, 8).try_into().ok()?);
        if pte & 1 == 0 {
            return None; // PTE invalid.
        }
        let readable = (pte >> 1) & 1 != 0;
        let writable = (pte >> 2) & 1 != 0;
        let executable = (pte >> 3) & 1 != 0;
        let ppn = (pte >> 10) & PPN_MASK;
        if readable || executable {
            // Leaf: superpage low PPN bits must be zero; the VA supplies them.
            if ppn & ((1u64 << (9 * level)) - 1) != 0 {
                return None;
            }
            let offset_mask = (1u64 << shift) - 1;
            return Some((ppn << 12) | (vaddr & offset_mask));
        }
        if writable {
            return None; // W without R is reserved for non-leaf entries.
        }
        table = ppn << 12;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT_PA: u64 = RAM_BASE + 0x10_000;
    const L1_PA: u64 = RAM_BASE + 0x11_000;
    const L0_PA: u64 = RAM_BASE + 0x12_000;
    const FRAME_PA: u64 = RAM_BASE + 0x20_000;

    fn leaf_pte(pa: u64) -> u64 {
        ((pa >> 12) << 10) | 0b1111 // V,R,W,X
    }

    fn table_pte(pa: u64) -> u64 {
        ((pa >> 12) << 10) | 0b0001 // V only: points at the next level
    }

    fn bus_with_table() -> SystemBus {
        let mut bus = SystemBus::new(vec![]);
        // Map VA 0x4000_0000 (vpn2=1, vpn1=0, vpn0=0) to FRAME_PA via a 4K leaf.
        bus.write_doubleword(ROOT_PA + 8, table_pte(L1_PA)).unwrap();
        bus.write_doubleword(L1_PA, table_pte(L0_PA)).unwrap();
        bus.write_doubleword(L0_PA, leaf_pte(FRAME_PA)).unwrap();
        // Identity-map the 1 GB superpage holding RAM (vpn2=2 for 0x8000_0000).
        bus.write_doubleword(ROOT_PA + 16, leaf_pte(0x8000_0000))
            .unwrap();
        bus
    }

    fn satp_sv39() -> u64 {
        (8u64 << 60) | (ROOT_PA >> 12)
    }

    #[test]
    fn debug_walk_resolves_4k_leaf_and_superpage() {
        let bus = bus_with_table();
        assert_eq!(
            debug_walk_sv39(&bus, satp_sv39(), 0x4000_0123),
            Some(FRAME_PA + 0x123)
        );
        assert_eq!(
            debug_walk_sv39(&bus, satp_sv39(), 0x8000_5678),
            Some(0x8000_5678),
            "1 GB superpage identity mapping"
        );
    }

    #[test]
    fn debug_walk_reports_unmapped_and_noncanonical() {
        let bus = bus_with_table();
        assert_eq!(debug_walk_sv39(&bus, satp_sv39(), 0x4000_2000), None);
        assert_eq!(
            debug_walk_sv39(&bus, satp_sv39(), 0x0123_4567_89AB_CDEF),
            None
        );
    }

    #[test]
    fn debug_walk_resolves_the_tail_of_a_mapped_page() {
        let mut bus = bus_with_table();
        for i in 0..4u64 {
            bus.write_byte(FRAME_PA + 0xFFC + i, 0xA0 + i as u8)
                .unwrap();
        }
        let tail = (0..4)
            .map(|i| {
                debug_walk_sv39(&bus, satp_sv39(), 0x4000_0FFC + i)
                    .map(|pa| bus.peek_bytes_raw(pa, 1)[0])
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        assert_eq!(tail, vec![0xA0, 0xA1, 0xA2, 0xA3]);
        assert_eq!(debug_walk_sv39(&bus, satp_sv39(), 0x4000_1000), None);
    }

    #[test]
    fn peek_bytes_virtual_zero_fills_and_chunks_across_pages() {
        // A directly-built VM starts in M-mode, so translation is identity;
        // this covers the per-page chunking and zero-fill of the virtual peek.
        let mut bus = SystemBus::new(vec![]);
        for i in 0..4u64 {
            bus.write_byte(RAM_BASE + 0xFFC + i, 0xB0 + i as u8)
                .unwrap();
        }
        let vm = VirtualMachine {
            cpu: Cpu::new(RAM_BASE, RAM_BASE + 0x10_000),
            bus,
        };
        let bytes = vm.peek_bytes_virtual(RAM_BASE + 0xFFC, 8);
        assert_eq!(bytes[..4], [0xB0, 0xB1, 0xB2, 0xB3]);
        assert_eq!(bytes.len(), 8);
    }

    fn vm_with_ram_byte(addr: u64, val: u8) -> VirtualMachine {
        let mut bus = SystemBus::new(vec![]);
        bus.write_byte(addr, val).unwrap();
        VirtualMachine {
            cpu: Cpu::new(RAM_BASE, RAM_BASE + 0x10_000),
            bus,
        }
    }

    #[test]
    fn snapshot_restore_reverts_ram_writes() {
        let addr = RAM_BASE + 0x100;
        let mut vm = vm_with_ram_byte(addr, 0x11);
        let snap = vm.snapshot();
        assert_eq!(snap.ram()[0x100], 0x11);

        vm.write_ram(addr, &[0x22]).unwrap();
        assert_eq!(vm.peek_bytes_raw(addr, 1), vec![0x22]);

        vm.restore(&snap);
        assert_eq!(vm.peek_bytes_raw(addr, 1), vec![0x11]);
    }

    // The session delta path swaps RAM out (without_ram) and back (with_ram).
    #[test]
    fn without_ram_then_with_ram_restores_state() {
        let addr = RAM_BASE + 0x100;
        let mut vm = vm_with_ram_byte(addr, 0x11);
        let snap = vm.snapshot();
        let light = snap.without_ram();
        assert!(light.ram().is_empty());

        let rebuilt = light.with_ram(snap.ram().to_vec());
        vm.write_ram(addr, &[0x22]).unwrap();
        vm.restore(&rebuilt);
        assert_eq!(vm.peek_bytes_raw(addr, 1), vec![0x11]);
    }
}
