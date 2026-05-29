use os_runtime::{kernel, stdlib};

use crate::TargetMode;
use crate::ir::{IntWidth, IrType};

fn append_section(buf: &mut String, header: &str, content: &str) {
    buf.push_str(header);
    buf.push_str(content);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
}

/// Return the complete stdlib source for the given target mode.
pub fn get_stdlib_source_for_mode(mode: TargetMode) -> String {
    match mode {
        TargetMode::Hosted => get_stdlib_source(),
        TargetMode::Freestanding => get_freestanding_stdlib_source(),
        TargetMode::Kernel => get_kernel_stdlib_source(),
    }
}

/// Return the stdlib as a list of (module_name, source) tuples for the given mode.
///
/// This makes it possible to compile each HLL file independently so that
/// `.ir`, `.s` and `.o` artifacts exist per original source file instead of
/// concatenating everything into one big bundle.
pub fn get_stdlib_modules_for_mode(mode: TargetMode) -> Vec<(&'static str, &'static str)> {
    match mode {
        TargetMode::Hosted => vec![
            ("types", stdlib::TYPES),
            ("memory_allocator", stdlib::MEMORY_ALLOCATOR),
            ("string_utils", stdlib::STRING_UTILS),
            ("runtime", stdlib::HOSTED_RUNTIME),
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

    vec![
        (
            "Str".to_owned(),
            IrType::Aggregate(vec![
                (
                    "data".to_owned(),
                    IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                ),
                ("length".to_owned(), IrType::Integer(IntWidth::I64)),
            ]),
        ),
        ("HeapBlock".to_owned(), heap_block),
    ]
}

/// Hosted stdlib: includes the Linux-syscall runtime and entry point.
pub fn get_stdlib_source() -> String {
    let capacity = stdlib::TYPES.len()
        + stdlib::MEMORY_ALLOCATOR.len()
        + stdlib::STRING_UTILS.len()
        + stdlib::HOSTED_RUNTIME.len()
        + 256;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", stdlib::TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        stdlib::MEMORY_ALLOCATOR,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        stdlib::STRING_UTILS,
    );
    append_section(
        &mut combined,
        "; --- stdlib: runtime ---\n",
        stdlib::HOSTED_RUNTIME,
    );
    combined
}

/// Kernel stdlib: types, allocator, strings, mem, freestanding panic/console,
/// klog, kernel utilities, kernel checks, and the kernel boot runtime.
/// No Linux syscalls. Entry point is `_kernel_start`; user code must define `kmain`.
///
/// Compile this with a distinct `string_prefix` (e.g. `"__kern_str_"`) so that
/// rodata string-literal labels never clash with user-code labels.
pub fn get_kernel_stdlib_source() -> String {
    let capacity = stdlib::TYPES.len()
        + stdlib::MEMORY_ALLOCATOR.len()
        + stdlib::STRING_UTILS.len()
        + stdlib::MEM.len()
        + stdlib::FREESTANDING_RUNTIME.len()
        + stdlib::FREESTANDING_CONSOLE.len()
        + stdlib::KLOG.len()
        + kernel::TRAP_ENTRY.len()
        + kernel::UTILITIES.len()
        + kernel::CHECKS.len()
        + kernel::TRAP_HANDLER.len()
        + kernel::PMM.len()
        + kernel::VMM.len()
        + kernel::PROCESS.len()
        + kernel::SYSCALL.len()
        + kernel::SCHEDULER.len()
        + kernel::FS.len()
        + 512;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", stdlib::TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        stdlib::MEMORY_ALLOCATOR,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        stdlib::STRING_UTILS,
    );
    append_section(&mut combined, "; --- stdlib: mem ---\n", stdlib::MEM);
    append_section(
        &mut combined,
        "; --- stdlib: runtime (freestanding) ---\n",
        stdlib::FREESTANDING_RUNTIME,
    );
    append_section(
        &mut combined,
        "; --- stdlib: console (freestanding) ---\n",
        stdlib::FREESTANDING_CONSOLE,
    );
    append_section(&mut combined, "; --- stdlib: klog ---\n", stdlib::KLOG);
    append_section(
        &mut combined,
        "; --- kernel: trap entry ---\n",
        kernel::TRAP_ENTRY,
    );
    append_section(
        &mut combined,
        "; --- kernel: utilities ---\n",
        kernel::UTILITIES,
    );
    append_section(&mut combined, "; --- kernel: checks ---\n", kernel::CHECKS);
    append_section(
        &mut combined,
        "; --- kernel: entry (runtime) ---\n",
        kernel::RUNTIME,
    );
    append_section(
        &mut combined,
        "; --- kernel: trap handler ---\n",
        kernel::TRAP_HANDLER,
    );
    append_section(&mut combined, "; --- kernel: pmm ---\n", kernel::PMM);
    append_section(&mut combined, "; --- kernel: vmm ---\n", kernel::VMM);
    append_section(
        &mut combined,
        "; --- kernel: process ---\n",
        kernel::PROCESS,
    );
    append_section(
        &mut combined,
        "; --- kernel: syscall ---\n",
        kernel::SYSCALL,
    );
    append_section(
        &mut combined,
        "; --- kernel: scheduler ---\n",
        kernel::SCHEDULER,
    );
    append_section(&mut combined, "; --- kernel: fs ---\n", kernel::FS);
    combined
}

/// Freestanding stdlib: types, allocator, strings, panic, and `_start` entry wrapper.
fn get_freestanding_stdlib_source() -> String {
    let capacity = stdlib::TYPES.len()
        + stdlib::MEMORY_ALLOCATOR.len()
        + stdlib::STRING_UTILS.len()
        + stdlib::FREESTANDING_RUNTIME.len()
        + stdlib::FREESTANDING_CONSOLE.len()
        + stdlib::FREESTANDING_ENTRY.len()
        + 256;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", stdlib::TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        stdlib::MEMORY_ALLOCATOR,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        stdlib::STRING_UTILS,
    );
    append_section(
        &mut combined,
        "; --- stdlib: runtime (freestanding) ---\n",
        stdlib::FREESTANDING_RUNTIME,
    );
    append_section(
        &mut combined,
        "; --- stdlib: console (freestanding) ---\n",
        stdlib::FREESTANDING_CONSOLE,
    );
    append_section(
        &mut combined,
        "; --- stdlib: entry (_start) ---\n",
        stdlib::FREESTANDING_ENTRY,
    );
    combined
}
