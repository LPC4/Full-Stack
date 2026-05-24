use os_runtime::{kernel, stdlib};

use crate::TargetMode;

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
/// klog, and the kernel boot runtime.  No Linux syscalls.  Entry point is
/// `_kernel_start`; user code must define `kmain`.
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
        + stdlib::KERNEL_UTILS.len()
        + kernel::TRAP_HANDLER.len()
        + kernel::PMM.len()
        + kernel::VMM.len()
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
    append_section(&mut combined, "; --- stdlib: kernel utils ---\n", stdlib::KERNEL_UTILS);
    append_section(
        &mut combined,
        "; --- stdlib: kernel entry (runtime) ---\n",
        kernel::RUNTIME,
    );
    append_section(
        &mut combined,
        "; --- stdlib: trap handler ---\n",
        kernel::TRAP_HANDLER,
    );
    append_section(
        &mut combined,
        "; --- stdlib: pmm ---\n",
        kernel::PMM,
    );
    append_section(
        &mut combined,
        "; --- stdlib: vmm ---\n",
        kernel::VMM,
    );
    combined
}

/// Freestanding stdlib: types, allocator, strings — no Linux syscalls, no `_start`.
fn get_freestanding_stdlib_source() -> String {
    let capacity = stdlib::TYPES.len()
        + stdlib::MEMORY_ALLOCATOR.len()
        + stdlib::STRING_UTILS.len()
        + stdlib::FREESTANDING_RUNTIME.len()
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
    combined
}
