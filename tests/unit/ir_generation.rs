/// Unit tests for HLL → IR lowering.
///
/// Tests here verify:
///   - Compilation pipeline smoke tests
///   - IR instruction selection (signed/unsigned arithmetic and comparisons)
///   - Type cast lowering
///   - Heap allocation/free lowering
///
/// Semantic acceptance/rejection rules live in tests/integration/spec_rules.rs.
/// Struct destructuring tests live in tests/unit/struct_destructuring.rs.
use full_stack::high_level_language::compilation_pipeline::{
    CompilationError, CompilationPipeline,
};
use full_stack::intermediate_language::{IrCmpOp, IrInstruction, IrMathOp};

fn compile_ok(source: &str) -> full_stack::high_level_language::compilation_pipeline::CompilationResult {
    CompilationPipeline::new()
        .compile(source)
        .unwrap_or_else(|e| panic!("expected compilation to succeed, got: {e}"))
}

fn assert_semantic_error(source: &str, fragment: &str) {
    let result = CompilationPipeline::new().compile(source);
    let err = result.expect_err("expected compilation to fail");
    match err {
        CompilationError::SemanticErrors(errors) => assert!(
            errors.iter().any(|m| m.contains(fragment)),
            "expected semantic error containing `{fragment}`, got: {errors:?}"
        ),
        other => panic!("expected SemanticErrors, got: {other:?}"),
    }
}

fn has_instruction<F>(source: &str, pred: F) -> bool
where
    F: Fn(&IrInstruction) -> bool,
{
    let result = compile_ok(source);
    result
        .ir_program
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .any(pred)
}

fn count_instructions<F>(source: &str, pred: F) -> usize
where
    F: Fn(&IrInstruction) -> bool,
{
    let result = compile_ok(source);
    result
        .ir_program
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| pred(i))
        .count()
}

// ── Smoke tests ───────────────────────────────────────────────────────────────

#[test]
fn compiles_minimal_valid_program() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.run_semantic_analysis = false;
    assert!(pipeline.compile("main: () -> i32 { return 42 }").is_ok());
}

#[test]
fn rejects_invalid_tokens() {
    let result = CompilationPipeline::new().compile("@invalid_token!@#");
    assert!(matches!(result.unwrap_err(), CompilationError::LexerError(_)));
}

#[test]
fn rejects_invalid_pointer_arithmetic() {
    assert_semantic_error(
        r#"main: () -> i32* {
    left: i32* = new(i32)
    right: i32* = new(i32)
    return left + right
}"#,
        "Type error in binary operation",
    );
}

// ── Signed / unsigned arithmetic lowering ────────────────────────────────────

#[test]
fn unsigned_division_emits_udiv() {
    assert!(has_instruction(
        r#"main: () -> i32 { a: u32 = 10  b: u32 = 2  c: u32 = a / b  return i32(c) }"#,
        |i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::Div),
    ));
}

#[test]
fn signed_division_emits_sdiv() {
    assert!(has_instruction(
        r#"main: () -> i32 { a: i32 = 10  b: i32 = 2  c: i32 = a / b  return c }"#,
        |i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::SDiv),
    ));
}

#[test]
fn unsigned_comparison_emits_unsigned_ops() {
    assert!(has_instruction(
        r#"main: () -> bool { a: u32 = 10  b: u32 = 20  return a < b }"#,
        |i| matches!(i, IrInstruction::Cmp { op, .. }
            if matches!(op, IrCmpOp::Ult | IrCmpOp::Ule | IrCmpOp::Ugt | IrCmpOp::Uge)),
    ));
}

#[test]
fn signed_comparison_emits_signed_ops() {
    assert!(has_instruction(
        r#"main: () -> bool { a: i32 = 10  b: i32 = 20  return a < b }"#,
        |i| matches!(i, IrInstruction::Cmp { op, .. }
            if matches!(op, IrCmpOp::Slt | IrCmpOp::Sle | IrCmpOp::Sgt | IrCmpOp::Sge)),
    ));
}

#[test]
fn mixed_signed_unsigned_ops_both_emitted() {
    let source = r#"main: () -> i32 {
    signed_val: i32 = 10 / 2
    ua: u32 = 5
    ub: u32 = 10
    unsigned_val: u32 = ua / ub
    signed_cmp: bool = 5 < 10
    unsigned_cmp: bool = ua < ub
    return signed_val
}"#;
    let result = compile_ok(source);
    let instrs: Vec<_> = result
        .ir_program
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .collect();
    assert!(
        instrs.iter().any(|i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::SDiv)),
        "expected SDiv"
    );
    assert!(
        instrs.iter().any(|i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::Div)),
        "expected Div (unsigned)"
    );
    assert!(
        instrs.iter().any(|i| matches!(i, IrInstruction::Cmp { op, .. } if matches!(op, IrCmpOp::Slt | IrCmpOp::Sle | IrCmpOp::Sgt | IrCmpOp::Sge))),
        "expected signed comparison"
    );
    assert!(
        instrs.iter().any(|i| matches!(i, IrInstruction::Cmp { op, .. } if matches!(op, IrCmpOp::Ult | IrCmpOp::Ule | IrCmpOp::Ugt | IrCmpOp::Uge))),
        "expected unsigned comparison"
    );
}

// ── Type cast lowering ────────────────────────────────────────────────────────

#[test]
fn cast_i32_to_i64_emits_cast() {
    assert!(has_instruction(
        r#"main: () -> i64 { a: i32 = 42  return i64(a) }"#,
        |i| matches!(i, IrInstruction::Cast { .. }),
    ));
}

#[test]
fn cast_u32_to_u64_emits_cast() {
    assert!(has_instruction(
        r#"main: () -> u64 { a: u32 = 42  return u64(a) }"#,
        |i| matches!(i, IrInstruction::Cast { .. }),
    ));
}

#[test]
fn cast_i64_to_i32_emits_cast() {
    assert!(has_instruction(
        r#"main: () -> i32 { a: i64 = 42  return i32(a) }"#,
        |i| matches!(i, IrInstruction::Cast { .. }),
    ));
}

#[test]
fn cast_i32_to_f64_emits_cast() {
    assert!(has_instruction(
        r#"main: () -> f64 { a: i32 = 42  return f64(a) }"#,
        |i| matches!(i, IrInstruction::Cast { .. }),
    ));
}

#[test]
fn cast_pointer_to_pointer_emits_cast() {
    assert!(has_instruction(
        r#"main: () -> i8* { a: i32* = new(i32)  return i8*(a) }"#,
        |i| matches!(i, IrInstruction::Cast { .. }),
    ));
}

#[test]
fn chained_casts_emit_multiple_cast_instructions() {
    assert!(
        count_instructions(
            r#"main: () -> i64 { a: i32 = 10  return i64(a) + i64(20) }"#,
            |i| matches!(i, IrInstruction::Cast { .. }),
        ) >= 2
    );
}

// ── Heap allocation lowering ──────────────────────────────────────────────────

#[test]
fn free_emits_heap_free_instruction() {
    let source = r#"external print: (value: i32) -> i32
main: () -> i32 {
    ptr: i32* = new(i32)
    @ptr = 42
    value: i32 = @ptr
    free(ptr)
    return value
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::HeapAlloc { .. })));
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::HeapFree { .. })));
}

#[test]
fn free_with_no_args_is_rejected() {
    assert!(
        CompilationPipeline::new()
            .compile("main: () -> i32 { ptr: i32* = new(i32)  free()  return 0 }")
            .is_err()
    );
}
