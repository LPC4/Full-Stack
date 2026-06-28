/// Tests that the compiler rejects programs referencing undefined identifiers.
///
/// This is a regression suite for the class of bug that caused 8 test failures:
/// a variable was renamed but not all uses were updated, leaving a dangling
/// reference that the compiler must catch before any binary is produced.
use full_stack::compilation_pipeline::{CompilationError, CompilationPipeline};

fn reject(src: &str, fragment: &str) {
    let mut p = CompilationPipeline::new();
    p.set_write_artifacts(false);
    let err = p
        .compile(src)
        .expect_err(&format!("expected compile to fail with '{fragment}'"));
    match &err {
        CompilationError::DiagnosticErrors(diags) => assert!(
            diags.iter().any(|d| d.message.contains(fragment)),
            "expected diagnostic containing `{fragment}`\ngot: {diags:?}"
        ),
        other => panic!("expected DiagnosticErrors, got: {other:?}"),
    }
}

fn accept(src: &str) {
    let mut p = CompilationPipeline::new();
    p.set_write_artifacts(false);
    p.compile(src)
        .unwrap_or_else(|e| panic!("expected compile to succeed, got: {e}"));
}

// --- Undefined local variables ---

#[test]
fn undefined_local_on_rhs_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    x: i32 = ghost_var
    return x
}
"#,
        "Undefined identifier",
    );
}

#[test]
fn undefined_local_in_arithmetic_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    x: i32 = 1 + no_such_var
    return x
}
"#,
        "Undefined identifier",
    );
}

#[test]
fn undefined_local_as_call_arg_is_rejected() {
    reject(
        r#"
external sink: (c: i32) -> i32
main: () -> i32 {
    sink(missing_var)
    return 0
}
"#,
        "Undefined identifier",
    );
}

#[test]
fn undefined_local_in_if_condition_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    if no_cond == 0 { return 1 }
    return 0
}
"#,
        "Undefined identifier",
    );
}

#[test]
fn undefined_local_in_while_condition_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    while ghost == 1 { return 1 }
    return 0
}
"#,
        "Undefined identifier",
    );
}

#[test]
fn undefined_local_in_return_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    return undefined_result
}
"#,
        "Undefined identifier",
    );
}

// --- Regression: exact shape of the computed_binary_pa bug ---
// A variable was defined under one name, then the name was changed but the
// old name was left on the right-hand side of an assignment.

#[test]
fn renamed_variable_residue_is_rejected() {
    reject(
        r#"
spawn_user_process: () {
    binary_pa: u64 = u64(0x87F00000)
    src_user_pa: u64 = computed_binary_pa
}
"#,
        "Undefined identifier",
    );
}

// The same shape but deeper: the stale name appears inside an expression tree.
#[test]
fn renamed_variable_residue_in_expr_is_rejected() {
    reject(
        r#"
compute: () {
    actual_val: u64 = u64(4096)
    result: u64 = (old_name_val + 1) * 2
}
"#,
        "Undefined identifier",
    );
}

// --- Float operations the backend cannot lower must be rejected cleanly ---
//

#[test]
fn float_modulo_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    a: f32 = 1.5
    b: f32 = 2.5
    c: f32 = a % b
    return 0
}
"#,
        "Mod",
    );
}

#[test]
fn float_bitwise_and_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    a: f32 = 1.5
    b: f32 = 2.5
    c: f32 = a & b
    return 0
}
"#,
        "BitwiseAnd",
    );
}

#[test]
fn float_shift_is_rejected() {
    reject(
        r#"
main: () -> i32 {
    a: f64 = 4.0
    b: f64 = 2.0
    c: f64 = a << b
    return 0
}
"#,
        "Shl",
    );
}

// --- Valid programs that must continue to compile ---

#[test]
fn variable_defined_before_use_is_accepted() {
    accept(
        r#"
main: () -> i32 {
    x: i32 = 7
    y: i32 = x + 1
    return y
}
"#,
    );
}

#[test]
fn external_declaration_does_not_require_definition() {
    // `external foo` tells the compiler the function lives in another module;
    // it must not be flagged as an undefined identifier.
    accept(
        r#"
external foo: (x: i32) -> i32

main: () -> i32 {
    return foo(1)
}
"#,
    );
}

#[test]
fn global_variable_used_in_function_is_accepted() {
    accept(
        r#"
counter: u64 = 0

bump: () {
    counter = counter + 1
}

main: () -> i32 {
    bump()
    return 0
}
"#,
    );
}

#[test]
fn const_used_in_expression_is_accepted() {
    accept(
        r#"
const PAGE_SIZE = 4096

main: () -> i32 {
    x: i32 = PAGE_SIZE * 2
    return 0
}
"#,
    );
}

#[test]
fn multiple_renames_all_correct_is_accepted() {
    // Both a and b are defined; the result uses both. No ghost names.
    accept(
        r#"
main: () -> i32 {
    a: u64 = u64(0x1000)
    b: u64 = a
    c: u64 = b + u64(1)
    return 0
}
"#,
    );
}
