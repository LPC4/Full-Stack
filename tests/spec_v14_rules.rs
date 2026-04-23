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
    assert!(result.is_ok(), "expected `Result` destructuring to compile successfully: {:?}", result.err());
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
    assert!(result.is_ok(), "expected generic placeholder arithmetic to compile successfully: {:?}", result.err());
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
    assert!(result.is_ok(), "expected `@arr[0]` to compile successfully: {:?}", result.err());
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
fn rejects_ambiguous_boolean_precedence() {
    let source = r#"
main: () -> i32 {
    x: bool = true or false and not true
    return 0
}
"#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);
    assert!(
        result.is_err(),
        "expected ambiguous boolean precedence to be rejected"
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




