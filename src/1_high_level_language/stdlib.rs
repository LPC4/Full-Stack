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

pub fn prepend_stdlib(source: &str) -> String {
    let mut combined = String::with_capacity(
        STD_TYPES.len() + STD_MEMORY.len() + STD_STRINGS.len() + source.len() + 256,
    );
    combined.push_str("; --- embedded stdlib: types ---\n");
    combined.push_str(STD_TYPES);
    if !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str("; --- embedded stdlib: memory_allocator ---\n");
    combined.push_str(STD_MEMORY);
    if !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str("; --- embedded stdlib: string_utils ---\n");
    combined.push_str(STD_STRINGS);
    if !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push('\n');
    combined.push_str(source);
    combined
}

