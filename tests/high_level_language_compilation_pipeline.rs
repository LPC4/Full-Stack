use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;

fn assert_semantic_error_contains(source: &str, expected: &str) {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);

    let err = match result {
        Ok(ok) => panic!("expected compilation to fail, but it succeeded: {ok:?}"),
        Err(err) => err,
    };

    match err {
        full_stack::high_level_language::compilation_pipeline::CompilationError::SemanticErrors(
            errors,
        ) => {
            assert!(
                errors.iter().any(|msg| msg.contains(expected)),
                "expected a semantic error containing `{expected}`, got: {errors:?}"
            );
        }
        other => panic!("expected semantic errors, got: {other:?}"),
    }
}

#[test]
fn test_pipeline_compiles_valid_program() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.run_semantic_analysis = false;

    let source = r#"
main: () -> i32 {
    return 42;
}
"#;

    let result = pipeline.compile(source);
    if let Err(ref e) = result {
        eprintln!("Compilation failed with error: {}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_pipeline_catches_lexer_error() {
    let pipeline = CompilationPipeline::new();
    let source = "@invalid_token!@#";

    let result = pipeline.compile(source);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        full_stack::high_level_language::compilation_pipeline::CompilationError::LexerError(_)
    ));
}

#[test]
fn rejects_address_of_dereference_expression() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    ptr: i32* = new(i32)
    return &@ptr
}
"#,
        "cannot take address of a dereference expression",
    );
}

#[test]
fn rejects_returning_stack_addresses() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    x: i32 = 5
    return &x
}
"#,
        "Returning address of local `x` is not allowed",
    );
}

#[test]
fn rejects_returning_address_of_local_field() {
    assert_semantic_error_contains(
        r#"
type Point = {
    x: i32,
    y: i32
}

main: () -> i32* {
    p: { x: i32, y: i32 } = { .x = 1, .y = 2 }
    return &(p.x)
}
"#,
        "Returning address of local `p` is not allowed",
    );
}

#[test]
fn rejects_returning_address_of_local_array_element() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    arr: i32[4]
    return &(arr[0])
}
"#,
        "Returning address of local `arr` is not allowed",
    );
}

#[test]
fn rejects_ambiguous_boolean_precedence() {
    assert_semantic_error_contains(
        r#"
main: () -> bool {
    return true or false and true
}
"#,
        "ambiguous boolean precedence must be parenthesized",
    );
}

#[test]
fn rejects_invalid_pointer_arithmetic() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    left: i32* = new(i32)
    right: i32* = new(i32)
    return left + right
}
"#,
        "Type error in binary operation",
    );
}

