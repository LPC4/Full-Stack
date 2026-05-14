// ---------------------------------------------------------------------------

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
const STD_IO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/io.hll"
));

fn append_section(buf: &mut String, header: &str, content: &str) {
    buf.push_str(header);
    buf.push_str(content);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
}

/// Return the complete stdlib source, ready to prepend to any user program.
pub fn get_stdlib_source() -> String {
    let capacity = STD_TYPES.len() + STD_MEMORY.len() + STD_STRINGS.len() + STD_IO.len() + 256;
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
    append_section(&mut combined, "; --- stdlib: io ---\n", STD_IO);
    combined
}

