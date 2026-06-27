use full_stack::compilation_pipeline::CompilationPipeline;

#[test]
fn allows_address_of_dereference_place() {
    // `@ptr` is a place, so `&@ptr` is a valid (redundant) way to write `ptr`.
    let source = r#"
main: () -> i32 {
    ptr: i32* = new(i32)
    same: i32* = &@ptr
    @same = 7
    defer free(ptr)
    return @ptr
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `&@ptr` to compile as a place address: {:?}",
        result.err()
    );
}

#[test]
fn allows_address_of_stack_array_element() {
    let source = r#"
main: () -> i32 {
    arr: i32[4] = []
    p: i32* = &arr[0]
    @p = 7
    return arr[0]
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `&arr[0]` to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_anonymous_inline_structs_everywhere() {
    let source = r#"
sum_pair: (pair: { left: i32, right: i32 }) -> i32 {
    return pair.left + pair.right
}

make_pair: (left: i32, right: i32) -> { left: i32, right: i32 } {
    return { .left = left, .right = right }
}

main: () -> i32 {
    pair: { left: i32, right: i32 } = make_pair(2, 3)
    other: { left: i32, right: i32 }* = new({ left: i32, right: i32 })
    @other = { .left = 4, .right = 5 }
    return sum_pair(pair) + other.left + other.right
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected anonymous inline structs in all type positions to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_typed_struct_literals() {
    let source = r#"
main: () -> i32 {
    value: { left: i32, right: i32 } = { .left = 7, .right = 11 }
    return value.left + value.right
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected typed struct literals to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_shorthand_struct_literals() {
    let source = r#"
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}

main: () -> i32 {
    { quotient: i32, remainder: i32 } = divide(10, 3)
    return quotient + remainder
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected shorthand struct literals to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_struct_destructuring_from_type_alias() {
    let source = r#"
struct Outcome {
    value: i32,
    success: bool
}

get_result: () -> Outcome {
    return { .value = 42, .success = true }
}

main: () -> i32 {
    { value: i32, success: bool } = get_result()
    if success {
        print(value)
    }
    return value
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `Outcome` destructuring to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_generic_placeholder_arithmetic() {
    let source = r#"
struct Box<T> {
    val: T,
    ptr: T*
}

main: () -> i32 {
    box1: Box<i32>* = new(Box<i32>)
    box1.val = 42
    return box1.val + 58
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected generic placeholder arithmetic to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_array_index_place_access() {
    let source = r#"
main: () -> i32 {
    arr: i32[4] = []
    arr[0] = 7
    return arr[0]
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `arr[0]` place access to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_stack_and_heap_arrays() {
    let source = r#"
main: () -> i32 {
    stack: i32[3] = []
    stack[0] = 1
    stack[1] = 2
    stack[2] = stack[0] + stack[1]

    heap: i32* = new(i32, 2)
    defer free(heap)
    heap[0] = stack[2]
    heap[1] = 4

    return heap[0] + heap[1]
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected stack and heap arrays to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_array_literals_through_assembly() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/programs/example/arrays_slices_and_ranges/arrays_slices_and_ranges.hll"
    ));

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline
        .compile(source)
        .expect("array literal example should compile to IR");

    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    assert!(
        asm.contains("array_literals") || asm.contains("main"),
        "expected assembly output to be generated for the array literal example"
    );
}

#[test]
fn allows_named_struct_alias_through_assembly() {
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

main: () -> i32 {
    p: Point = { .x = 1, .y = 2 }
    return p.x + p.y
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline
        .compile(source)
        .expect("named struct alias example should compile to IR");

    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    assert!(
        !asm.is_empty(),
        "expected assembly output to be generated for the named struct alias example"
    );
}

#[test]
fn allows_signed_comparisons_through_assembly() {
    let source = r#"
main: () -> i32 {
    x: i32 = 10
    y: i32 = 5
    if x > y {
        return 1
    }
    return 0
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline
        .compile(source)
        .expect("signed comparison example should compile to IR");

    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    assert!(
        !asm.is_empty(),
        "expected assembly output to be generated for the signed comparison example"
    );
}

#[test]
fn allows_string_literals_against_slice_alias() {
    let source = r#"
type Text = u8[]

main: () -> i32 {
    greeting: Text = "hello"
    return 0
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected string literals to compile against a `u8[]` alias: {:?}",
        result.err()
    );
}

#[test]
fn all_launch_examples_compile() {
    // The catalog is the single source of truth for launchable examples; iterate
    // it so a newly added example is covered without editing this test.
    use full_stack::view::{ProgramCatalog, ProgramKind};
    let catalog = ProgramCatalog::default();
    let examples = catalog.get_programs_by_kind(ProgramKind::Example);
    assert!(!examples.is_empty(), "catalog exposes no example programs");

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    for program in examples {
        pipeline.set_current_source_path(program.source_path.clone());
        let result = pipeline.compile(&program.source);
        assert!(
            result.is_ok(),
            "expected launch example `{}` to compile successfully: {:?}",
            program.name,
            result.err()
        );
    }
    pipeline.set_current_source_path(None::<String>);
}

// The examples collectively must showcase every implemented HLL feature family;
// each needle below is a representative token some example must demonstrate.
#[test]
fn examples_cover_core_features() {
    use full_stack::view::{ProgramCatalog, ProgramKind};
    let catalog = ProgramCatalog::default();
    let combined = catalog
        .get_programs_by_kind(ProgramKind::Example)
        .iter()
        .map(|p| p.source.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let needles = [
        (":=", "inferred declaration"),
        ("const ", "compile-time constant"),
        (" as ", "cast"),
        ("+=", "compound assignment"),
        ("@", "whole-pointee dereference"),
        ("&", "address-of"),
        ("new(", "heap allocation"),
        ("defer ", "deferred cleanup"),
        ("struct ", "struct declaration"),
        (".len", "slice length"),
        ("[..]", "array-to-slice coercion"),
        ("..=", "inclusive range"),
        ("for ", "for loop"),
        ("enum ", "enum declaration"),
        ("match ", "match expression"),
        ("Option", "Option carrier"),
        ("Result", "Result carrier"),
        ("?", "try propagation"),
        ("<i32>", "generic specialization"),
        ("fn(i32", "function pointer type"),
        ("'A'", "character literal"),
    ];

    for (needle, feature) in needles {
        assert!(
            combined.contains(needle),
            "no launchable example demonstrates {feature} (missing token `{needle}`)"
        );
    }
}

#[test]
fn allows_newline_separated_struct_fields() {
    // Newlines terminate statements, so struct fields may be separated by newlines.
    let source = r#"
struct Point {
    x: f32
    y: f32
}

main: () -> i32 {
    return 0
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected newline-separated struct fields to compile: {:?}",
        result.err()
    );
}

#[test]
fn allows_mixed_boolean_precedence() {
    let source = r#"
main: () -> i32 {
    x: bool = true or false and not true
    if x {
        return 1
    }
    return 0
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected mixed boolean precedence to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn rejects_returning_address_of_local_variable() {
    let source = r#"
leak: () -> i32* {
    x: i32 = 5
    return &x
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected returning stack address to be rejected"
    );
}

#[test]
fn rejects_returning_address_of_local_field() {
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

leak: () -> i32* {
    p: Point = { .x = 5, .y = 6 }
    return &(p.x)
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected returning the address of a local field to be rejected"
    );
}

#[test]
fn rejects_returning_address_of_local_array_element() {
    let source = r#"
leak: () -> i32* {
    arr: i32[4] = []
    return &(arr[0])
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected returning the address of a local array element to be rejected"
    );
}
