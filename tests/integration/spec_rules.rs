use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;

#[test]
fn rejects_address_of_dereference_expression() {
    let source = r#"
main: () -> i32 {
    ptr: i32* = new(i32)
    bad: i32* = &@ptr
    free(ptr)
    return 0
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(result.is_err(), "expected `&@ptr` to be rejected");
}

#[test]
fn allows_address_of_stack_array_element() {
    let source = r#"
main: () -> i32 {
    arr: i32[4]
    p: i32** = &(arr[0])
    @@p = 7
    return @(arr[0])
}
"#;

    let pipeline = CompilationPipeline::new();
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
    return { left: i32 = left, right: i32 = right }
}

main: () -> i32 {
    pair: { left: i32, right: i32 } = make_pair(2, 3)
    other: { left: i32, right: i32 }* = new({ left: i32, right: i32 })
    @other = { .left = 4, .right = 5 }
    return sum_pair(pair) + @other.left + @other.right
}
"#;

    let pipeline = CompilationPipeline::new();
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
    value: { left: i32, right: i32 } = { left: i32 = 7, right: i32 = 11 }
    return value.left + value.right
}
"#;

    let pipeline = CompilationPipeline::new();
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

    let pipeline = CompilationPipeline::new();
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
type Result = {
    value: i32,
    success: bool
}

get_result: () -> Result {
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

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `Result` destructuring to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_generic_placeholder_arithmetic() {
    let source = r#"
type Box<T> = {
    val: T,
    ptr: T*
}

main: () -> i32 {
    box1: Box<i32>* = new(Box<i32>)
    @box1.val = 42
    return @box1.val + 58
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected generic placeholder arithmetic to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_deref_after_array_indexing() {
    let source = r#"
main: () -> i32 {
    arr: i32[4]
    @arr[0] = 7
    return @arr[0]
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected `@arr[0]` to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn allows_stack_and_heap_arrays() {
    let source = r#"
main: () -> i32 {
    stack: i32[3]
    @stack[0] = 1
    @stack[1] = 2
    @stack[2] = @stack[0] + @stack[1]

    heap: i32[2]* = new([2]i32)
    defer free(heap)
    @heap[0] = @stack[2]
    @heap[1] = 4

    return @heap[0] + @heap[1]
}
"#;

    let pipeline = CompilationPipeline::new();
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
        "/programs/example/array_literals.hll"
    ));

    let pipeline = CompilationPipeline::new();
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
type Point = {
    x: i32,
    y: i32
}

main: () -> i32 {
    p: Point = { .x = 1, .y = 2 }
    return p.x + p.y
}
"#;

    let pipeline = CompilationPipeline::new();
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

    let pipeline = CompilationPipeline::new();
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
fn allows_string_literals_against_text_alias() {
    let source = r#"
type Text = {
    data: u8*,
    length: u64
}

main: () -> i32 {
    greeting: Text = "hello"
    return 0
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_ok(),
        "expected string literals to compile against `Text` aliases: {:?}",
        result.err()
    );
}

#[test]
fn all_launch_examples_compile() {
    let examples = [
        (
            "core_syntax",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/core_syntax.hll"
            )),
        ),
        (
            "pointers_arrays",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/pointers_arrays.hll"
            )),
        ),
        (
            "array_literals",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/array_literals.hll"
            )),
        ),
        (
            "structs_destructuring",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/structs_destructuring.hll"
            )),
        ),
        (
            "control_flow_functions",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/control_flow_functions.hll"
            )),
        ),
        (
            "generics_strings_consts",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
            "/programs/example/generics_strings.hll"
            )),
        ),
    ];

    let pipeline = CompilationPipeline::new();
    for (name, source) in examples {
        let result = pipeline.compile(source);
        assert!(
            result.is_ok(),
            "expected launch example `{}` to compile successfully: {:?}",
            name,
            result.err()
        );
    }
}

#[test]
fn rejects_struct_type_without_commas() {
    let source = r#"
type Point = {
    x: f32
    y: f32
}

main: () -> i32 {
    return 0
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected missing commas in struct type definitions to be rejected"
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

    let pipeline = CompilationPipeline::new();
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

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected returning stack address to be rejected"
    );
}

#[test]
fn rejects_returning_address_of_local_field() {
    let source = r#"
type Point = {
    x: i32,
    y: i32
}

leak: () -> i32* {
    p: Point = { .x = 5, .y = 6 }
    return &(p.x)
}
"#;

    let pipeline = CompilationPipeline::new();
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
    arr: i32[4]
    return &(arr[0])
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected returning the address of a local array element to be rejected"
    );
}
