use os_runtime::{kernel, stdlib};

use crate::TargetMode;
use crate::ir::{IntWidth, IrType};

/// Return the stdlib as a list of (`module_name`, source) tuples for the given mode.
///
/// This makes it possible to compile each HLL file independently so that
/// `.ir`, `.s` and `.o` artifacts exist per original source file instead of
/// concatenating everything into one big bundle.
pub fn get_stdlib_modules_for_mode(mode: TargetMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        TargetMode::Hosted => vec![
            ("types", stdlib::TYPES),
            ("memory_allocator", stdlib::MEMORY_ALLOCATOR_HOSTED),
            ("string_utils", stdlib::STRING_UTILS),
            ("runtime", stdlib::HOSTED_RUNTIME),
            ("syscalls", stdlib::HOSTED_SYSCALLS),
        ],
        TargetMode::Freestanding => vec![
            ("types", stdlib::TYPES),
            ("memory_allocator", stdlib::MEMORY_ALLOCATOR),
            ("string_utils", stdlib::STRING_UTILS),
            ("runtime", stdlib::FREESTANDING_RUNTIME),
            ("console", stdlib::FREESTANDING_CONSOLE),
            ("entry", stdlib::FREESTANDING_ENTRY),
        ],
        TargetMode::Kernel => vec![
            ("types", stdlib::TYPES),
            ("memory_allocator", stdlib::MEMORY_ALLOCATOR),
            ("string_utils", stdlib::STRING_UTILS),
            ("mem", stdlib::MEM),
            ("runtime", stdlib::FREESTANDING_RUNTIME),
            ("console", stdlib::FREESTANDING_CONSOLE),
            ("klog", stdlib::KLOG),
            ("trap_entry", kernel::TRAP_ENTRY),
            ("utilities", kernel::UTILITIES),
            ("checks", kernel::CHECKS),
            ("entry", kernel::RUNTIME),
            ("trap_handler", kernel::TRAP_HANDLER),
            ("pmm", kernel::PMM),
            ("vmm", kernel::VMM),
            ("process", kernel::PROCESS),
            ("syscall", kernel::SYSCALL),
            ("scheduler", kernel::SCHEDULER),
            ("fs", kernel::FS),
        ],
    }
}

/// Shared named types required by independent stdlib modules.
///
/// These are registered directly in the compiler context so that modules like
/// `memory_allocator.hll` can resolve `HeapBlock` without concatenating `types.hll`.
pub fn get_stdlib_type_prelude() -> Vec<(String, IrType)> {
    let heap_block = IrType::Aggregate(vec![
        (
            "next".to_owned(),
            IrType::Pointer(Box::new(IrType::Named("HeapBlock".to_owned()))),
        ),
        (
            "ptr".to_owned(),
            IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
        ),
        ("size".to_owned(), IrType::Integer(IntWidth::I64)),
        ("is_free".to_owned(), IrType::Integer(IntWidth::I64)),
    ]);

    vec![("HeapBlock".to_owned(), heap_block)]
}
