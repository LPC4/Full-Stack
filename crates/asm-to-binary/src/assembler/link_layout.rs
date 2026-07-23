/// Controls image placement in virtual memory and which auxiliary symbols to inject.
/// Symbol contract: `_RISCV_SPECIFICATIONS.md` section 9.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLayout {
    /// Virtual base address where the image is loaded.
    pub load_base: u64,

    /// Inject section, heap, and stack boundary symbols for kernel startup.
    pub emit_layout_symbols: bool,

    /// Virtual address emitted as `__stack_top`.
    pub stack_top: u64,

    /// Heap bytes placed after `__bss_end`; zero omits `__heap_end`.
    pub heap_size: u64,
}

impl Default for LinkLayout {
    fn default() -> Self {
        Self::hosted()
    }
}

impl LinkLayout {
    /// Hosted (Linux userspace) load at `ELF_LOAD_BASE`, no extra symbols.
    pub fn hosted() -> Self {
        Self {
            load_base: 0x0001_0000,
            emit_layout_symbols: false,
            stack_top: 0,
            heap_size: 0,
        }
    }

    /// Bare-metal RISC-V kernel defaults.
    ///
    /// Assumes `OpenSBI` places the kernel at `0x8020_0000`, 128 MiB RAM from
    /// `0x8000_0000`, a 6 MiB stack, and a 4 MiB heap.
    pub fn freestanding_kernel() -> Self {
        Self {
            load_base: 0x8020_0000,
            emit_layout_symbols: true,
            stack_top: 0x8060_0000,     // 6 MiB into RAM
            heap_size: 4 * 1024 * 1024, // 4 MiB heap
        }
    }
}
