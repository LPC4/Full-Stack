//! Firmware: all bare-metal source files embedded as compile-time string constants.
//!
//! ## Boot sequence
//!
//! ```text
//! Power-on
//!   └─ boot/rom.s          M-mode ROM firmware
//!        PMP + delegation + mret → S-mode
//!   └─ kernel/runtime_kernel.hll   Supervisor-mode kernel entry (_kernel_start)
//!        kmain() / kpanic
//!
//! Hosted / userspace programs
//!   └─ stdlib/hosted/runtime.hll   _start → main() → sys_exit
//!   └─ stdlib/freestanding/        bare-metal programs (no Linux syscalls)
//!   └─ stdlib/common/              shared: types, malloc, string utils, mem, klog
//! ```
//!
//! Consumers:
//!   - [`virtual_machine`] — uses [`ROM_SOURCE`] to assemble the ROM image at startup.
//!   - [`hll_to_ir`]       — uses [`stdlib`] and [`kernel`] constants to build stdlib bundles.

/// M-mode ROM firmware source (RISC-V assembly).
///
/// Assembled at VM startup by `virtual_machine::rom::generate_rom_image()`.
/// Layout: `_start` at offset 0x000 (boot stub), `_m_trap` at offset 0x100 (trap handler).
pub const ROM_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/rom.s"));

/// HLL standard library source fragments, one constant per file.
///
/// Consumers assemble these in order to build a complete stdlib bundle.
/// See `hll_to_ir::stdlib` for the three supported link orders.
pub mod stdlib {
    /// Shared type definitions (`Str`, `HeapBlock`).
    pub const TYPES: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/common/types.hll"));

    /// Bump-pointer allocator (`malloc`, `free`, `heap_raw_alloc`).
    pub const MEMORY_ALLOCATOR: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/common/memory_allocator.hll"));

    /// String utilities (`str_len`, `str_equals`, `str_copy`, `str_concat`).
    pub const STRING_UTILS: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/common/string_utils.hll"));

    /// Low-level memory primitives (`memset`, `memcpy`, `memmove`, `memcmp`).
    pub const MEM: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/common/mem.hll"));

    /// Kernel logging helpers (`klog`, `klog_ok`, `klog_warn`, `klog_error`, `klog_int`, `klog_hex`).
    pub const KLOG: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/common/klog.hll"));

    /// Hosted (Linux userspace) runtime: `_start`, `putchar`, `puts`, `print_int`, `exit`.
    pub const HOSTED_RUNTIME: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/hosted/runtime.hll"));

    /// Freestanding runtime: `_kpanic` / `kpanic` (UART direct-write, no syscalls).
    pub const FREESTANDING_RUNTIME: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/freestanding/runtime.hll"));

    /// Freestanding console: `console_putchar`, `console_write`, `console_writeln`,
    /// `console_print_int`, `console_print_hex` (NS16550A UART at 0x10000000).
    pub const FREESTANDING_CONSOLE: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/freestanding/console.hll"));
}

/// Kernel-mode HLL source fragments.
pub mod kernel {
    /// Kernel boot runtime: `_kernel_start`, `kmalloc`, `kshutdown`.
    ///
    /// Entry point is `_kernel_start`; user code must define `kmain`.
    pub const RUNTIME: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/runtime_kernel.hll"));
}
