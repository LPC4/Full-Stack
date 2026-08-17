//! Firmware, kernel, and stdlib sources embedded as compile-time strings.
//! See `_OS_SPECIFICATION.md` for boot order, privilege transitions, and bundle layout.

pub const BOOT_STARTUP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s"));

pub const BOOT_TRAP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s"));

pub const ROM_SOURCE: &str = concat!(
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s")),
);

include!(concat!(env!("OUT_DIR"), "/sources.rs"));

/// HLL standard library source fragments; `hll_to_ir::stdlib` owns link order.
pub mod stdlib {
    pub const ALLOC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/core/alloc.hll"
    ));

    pub const HEAP_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/baremetal/heap.hll"
    ));

    pub const HEAP_SOURCE_HOSTED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/hosted/heap.hll"
    ));

    pub const STRING: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/core/string.hll"
    ));

    pub const MEM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/stdlib/core/mem.hll"));

    pub const KLOG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/kernel/klog.hll"
    ));

    pub const HOSTED_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/hosted/runtime.hll"
    ));

    pub const HOSTED_SYS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/hosted/sys.hll"
    ));

    pub const FREESTANDING_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/baremetal/runtime.hll"
    ));

    pub const FREESTANDING_CONSOLE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/baremetal/console.hll"
    ));

    /// Only included in pure freestanding mode; kernel has its own `_kernel_start`.
    pub const FREESTANDING_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/freestanding/entry.hll"
    ));
}

/// Kernel-mode HLL source fragments.
pub mod kernel {
    /// Shared layout prelude prepended to every kernel translation unit.
    pub const LAYOUT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/core/layout.hll"
    ));

    pub const ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/platform/kernel/entry.hll"
    ));

    pub const TRAP_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap/trap_entry.hll"
    ));

    pub const UTILITIES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/core/utilities.hll"
    ));

    pub const CHECKS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/core/checks.hll"
    ));

    pub const TRAP_HANDLER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap/trap_handler.hll"
    ));

    pub const PMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/mm/pmm.hll"));

    pub const VMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/mm/vmm.hll"));

    pub const PROCESS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/sched/process.hll"
    ));

    pub const SYSCALL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/core/syscall.hll"
    ));

    pub const SCHEDULER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/sched/scheduler.hll"
    ));

    pub const FS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/fs/fs.hll"));

    pub const MY_KERNEL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/core/my_kernel.hll"
    ));
}
