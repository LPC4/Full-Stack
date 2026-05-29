//! Firmware: all bare-metal source files embedded as compile-time string constants.
//!
//! ## Boot sequence
//!
//! ```text
//! Power-on
//!   boot/startup.s       M-mode ROM: PMP, delegation, mret to S-mode
//!   boot/trap.s          M-mode ROM: trap handler, syscall dispatch
//!   kernel/entry.hll     S-mode kernel entry (_kernel_start -> kmain)
//!
//! Hosted / userspace programs
//!   stdlib/hosted/runtime.hll     _start -> main() -> sys_exit
//!   stdlib/freestanding/          bare-metal programs (no Linux syscalls)
//!   stdlib/common/                shared: types, malloc, string utils, mem, klog
//! ```
//!
//! Consumers:
//!   - [`virtual_machine`] assembles [`ROM_SOURCE`] into the ROM image at startup.
//!   - [`hll_to_ir`] uses [`stdlib`] and [`kernel`] constants to build stdlib bundles.

/// M-mode boot stub (RISC-V assembly): PMP, delegation, mret to S-mode.
/// Placed at ROM offset 0x000, padded to 0x100 bytes.
pub const BOOT_STARTUP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s"));

/// M-mode trap handler (RISC-V assembly): ecall dispatch and syscall implementations.
/// Placed at ROM offset 0x100, immediately after the `_start` padding.
pub const BOOT_TRAP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s"));

/// Full ROM source: `BOOT_STARTUP` concatenated with `BOOT_TRAP`.
/// `_start` at 0x000, `_m_trap` at 0x100.
pub const ROM_SOURCE: &str = concat!(
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s")),
);

/// HLL standard library source fragments, one constant per file.
///
/// Consumers assemble these in order to build a complete stdlib bundle.
/// See `hll_to_ir::stdlib` for the three supported link orders.
pub mod stdlib {
    /// Shared type definitions (`Str`, `HeapBlock`).
    pub const TYPES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/types.hll"
    ));

    /// Bump-pointer allocator (`malloc`, `free`, `heap_raw_alloc`).
    pub const MEMORY_ALLOCATOR: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/memory_allocator.hll"
    ));

    /// String utilities (`str_len`, `str_equals`, `str_copy`, `str_concat`).
    pub const STRING_UTILS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/string_utils.hll"
    ));

    /// Low-level memory primitives (`memset`, `memcpy`, `memmove`, `memcmp`).
    pub const MEM: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/mem.hll"
    ));

    /// Kernel logging helpers (`klog`, `klog_ok`, `klog_warn`, `klog_error`, `klog_int`, `klog_hex`).
    pub const KLOG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/klog.hll"
    ));

    /// Hosted (Linux userspace) runtime: `_start`, `putchar`, `puts`, `print_int`, `exit`.
    pub const HOSTED_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/hosted/runtime.hll"
    ));

    /// Freestanding runtime: `_kpanic` / `kpanic` (UART direct-write, no syscalls).
    pub const FREESTANDING_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/runtime.hll"
    ));

    /// Freestanding console: `console_putchar`, `console_write`, `console_writeln`,
    /// `console_print_int`, `console_print_hex` (NS16550A UART at 0x10000000).
    pub const FREESTANDING_CONSOLE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/console.hll"
    ));

    /// Freestanding entry wrapper: `_start` calls `main`, then halts via SYSCON.
    /// ONLY included in pure freestanding mode, kernel has its own `_kernel_start`.
    pub const FREESTANDING_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/entry.hll"
    ));
}

/// Kernel-mode HLL source fragments.
pub mod kernel {
    /// Kernel entry: minimal kernel entrypoint (`_kernel_start` -> `kmain`).
    pub const RUNTIME: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/entry.hll"));

    /// S-mode trap entry: stvec prologue/epilogue, trap_init, sscratch helpers.
    /// The entry-point for all S-mode traps and interrupts.
    pub const TRAP_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap_entry.hll"
    ));

    /// Kernel platform helpers: kmalloc, kshutdown, timer, PLIC init.
    /// Core kernel infrastructure functions that use externs.
    pub const UTILITIES: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/utilities.hll"));

    /// Kernel checks and diagnostics: memory_self_test, etc.
    /// Called during boot to validate kernel systems.
    pub const CHECKS: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/checks.hll"));

    /// S-mode trap dispatcher: `trap_handler(frame: u64*)`.
    /// Reads scause from the trap frame and dispatches to timer/external/software
    /// interrupt handlers or exception handlers.  Depends on `kpanic`, `klog_hex`,
    /// and `timer_set` (all provided by the kernel stdlib bundle).
    pub const TRAP_HANDLER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap_handler.hll"
    ));

    /// Physical Memory Manager: `pmm_init`, `pmm_alloc`, `pmm_free`.
    /// 4 KiB page granularity; free-list + bump allocator.
    pub const PMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/pmm.hll"));

    /// Sv39 Virtual Memory Manager: `vmm_init`, `vmm_enable`, `vmm_map`,
    /// `vmm_map_1gib`, `vmm_map_range`. Depends on `pmm_alloc` and `memset`.
    pub const VMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/vmm.hll"));

    /// Process Control Block and lifecycle: `process_init`, `process_create`.
    /// Depends on `pmm_alloc`, `vmm_map`, `kmalloc`, `memset`, `memcpy`.
    pub const PROCESS: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/process.hll"));

    /// Syscall dispatch: `syscall_dispatch`, `sys_write_impl`.
    /// Depends on `klog_int`, `klog_error`.
    pub const SYSCALL: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/syscall.hll"));

    /// Round-robin scheduler: `scheduler_init`, `scheduler_add`, `schedule`.
    /// Depends on `memcpy`, `kpanic`, `klog_*`.
    pub const SCHEDULER: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/scheduler.hll"));

    /// Inode-based read-write filesystem: `fs_init`, `fs_open`, `fs_read`, `fs_write`,
    /// `fs_close`, `fs_create`, `fs_mkdir`, `fs_rename`.
    pub const FS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/fs.hll"));

    /// Reference kernel: full boot sequence demonstrating real and stub subsystems.
    /// Defines `kmain`; depends on the kernel stdlib bundle.
    pub const MY_KERNEL: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/my_kernel.hll"));
}

/// User-space example programs.
pub mod user {
    /// Hello-world user program: writes a greeting via ecall, then yields forever.
    pub const USER_HELLO: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/user_hello.hll"));
}
