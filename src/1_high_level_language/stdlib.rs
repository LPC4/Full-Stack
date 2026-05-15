use crate::high_level_language::compilation_pipeline::TargetMode;

const STD_TYPES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/types.hll"
));
const STD_MEMORY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/memory_allocator.hll"
));
const STD_STRINGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/string_utils.hll"
));
const STD_RUNTIME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/runtime.hll"
));
const STD_RUNTIME_FREESTANDING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/runtime_freestanding.hll"
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
