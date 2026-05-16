use crate::high_level_language::compilation_pipeline::TargetMode;

const STD_TYPES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/common/types.hll"
));
const STD_MEMORY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/common/memory_allocator.hll"
));
const STD_STRINGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/common/string_utils.hll"
));
const STD_MEM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/common/mem.hll"
));
const STD_KLOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/common/klog.hll"
));
const STD_RUNTIME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/hosted/runtime.hll"
));
const STD_RUNTIME_FREESTANDING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/freestanding/runtime.hll"
));
const STD_CONSOLE_FREESTANDING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/stdlib/freestanding/console.hll"
));
const STD_KERNEL_RUNTIME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/platform/kernel/runtime_kernel.hll"
));

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
    }
}

/// Hosted stdlib: includes the Linux-syscall runtime and entry point.
pub fn get_stdlib_source() -> String {
    let capacity =
        STD_TYPES.len() + STD_MEMORY.len() + STD_STRINGS.len() + STD_RUNTIME.len() + 256;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", STD_TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        STD_MEMORY,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        STD_STRINGS,
    );
    append_section(&mut combined, "; --- stdlib: runtime ---\n", STD_RUNTIME);
    combined
}

/// Kernel stdlib: types, allocator, strings, mem, freestanding panic/console,
/// klog, and the kernel boot runtime.  No Linux syscalls.  Entry point is
/// `_kernel_start`; user code must define `kmain`.
///
/// Compile this with a distinct `string_prefix` (e.g. `"__kern_str_"`) so that
/// rodata string-literal labels never clash with user-code labels (which use
/// the default `"str_"` prefix).
pub fn get_kernel_stdlib_source() -> String {
    let capacity = STD_TYPES.len()
        + STD_MEMORY.len()
        + STD_STRINGS.len()
        + STD_MEM.len()
        + STD_RUNTIME_FREESTANDING.len()
        + STD_CONSOLE_FREESTANDING.len()
        + STD_KLOG.len()
        + STD_KERNEL_RUNTIME.len()
        + 512;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", STD_TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        STD_MEMORY,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        STD_STRINGS,
    );
    append_section(&mut combined, "; --- stdlib: mem ---\n", STD_MEM);
    append_section(
        &mut combined,
        "; --- stdlib: runtime (freestanding) ---\n",
        STD_RUNTIME_FREESTANDING,
    );
    append_section(
        &mut combined,
        "; --- stdlib: console (freestanding) ---\n",
        STD_CONSOLE_FREESTANDING,
    );
    append_section(&mut combined, "; --- stdlib: klog ---\n", STD_KLOG);
    append_section(
        &mut combined,
        "; --- stdlib: kernel runtime ---\n",
        STD_KERNEL_RUNTIME,
    );
    combined
}

/// Freestanding stdlib: types, allocator, strings — NO Linux syscalls, NO _start.
fn get_freestanding_stdlib_source() -> String {
    let capacity = STD_TYPES.len()
        + STD_MEMORY.len()
        + STD_STRINGS.len()
        + STD_RUNTIME_FREESTANDING.len()
        + 256;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", STD_TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        STD_MEMORY,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        STD_STRINGS,
    );
    append_section(
        &mut combined,
        "; --- stdlib: runtime (freestanding) ---\n",
        STD_RUNTIME_FREESTANDING,
    );
    combined
}
