/// Controls how the linker places the image in virtual memory and what
/// auxiliary symbols to inject into the assembled output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLayout {
    /// Virtual base address where the image is loaded.
    /// - Hosted userspace: 0x0001_0000 (ELF_LOAD_BASE).
    /// - Bare-metal RISC-V after SBI handoff: 0x8020_0000.
    /// - Bare-metal without SBI: 0x8000_0000.
    pub load_base: u64,

    /// If `true`, inject these boundary symbols into the output after assembly:
    ///
    /// | Symbol          | Value                                    |
    /// |-----------------|------------------------------------------|
    /// | `__text_start`  | start of `.text`                         |
    /// | `__text_end`    | end of `.text`                           |
    /// | `__rodata_start`| start of `.rodata`                       |
    /// | `__rodata_end`  | end of `.rodata`                         |
    /// | `__data_start`  | start of `.data`                         |
    /// | `__data_end`    | end of `.data`                           |
    /// | `__bss_start`   | start of `.bss`                          |
    /// | `__bss_end`     | end of `.bss`                            |
    /// | `__heap_start`  | first byte after BSS (16-byte aligned)   |
    /// | `__heap_end`    | `__heap_start + heap_size`               |
    /// | `__stack_top`   | `stack_top` field                        |
    ///
    /// Kernel startup code can read these symbols to zero BSS, set the stack
    /// pointer, and initialise the heap allocator — without hard-coding addresses.
    pub emit_layout_symbols: bool,

    /// Virtual address of the initial stack pointer (`__stack_top`).
    /// Must be set when `emit_layout_symbols` is `true`.
    pub stack_top: u64,

    /// Heap size in bytes placed immediately after `__bss_end` (aligned to 16).
    /// Set to 0 to omit `__heap_end`.
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
    /// Assumes OpenSBI places the kernel at `0x8020_0000`, 128 MiB RAM from
    /// `0x8000_0000`, a 6 MiB stack, and a 4 MiB heap.
    pub fn freestanding_kernel() -> Self {
        Self {
            load_base: 0x8020_0000,
            emit_layout_symbols: true,
            stack_top: 0x8060_0000, // 6 MiB into RAM
            heap_size: 4 * 1024 * 1024, // 4 MiB heap
        }
    }
}
