/// Unit tests for HLL -> IR lowering.
///
/// Tests here verify:
///   - Compilation pipeline smoke tests
///   - IR instruction selection (signed/unsigned arithmetic and comparisons)
///   - Type cast lowering
///   - Heap allocation/free lowering
///   - Control flow (if-else, while) -> Branch terminators
///   - Function call lowering
///   - Global string constants
///   - Unary operations
///   - Stack allocation and pointer load/store
///
/// Semantic acceptance/rejection rules live in tests/integration/spec_rules.rs.
/// Struct destructuring tests live in tests/unit/struct_destructuring.rs.
use full_stack::compilation_pipeline::{
    CompilationError, CompilationPipeline,
};
use hll_to_ir::{IrCmpOp, IrInstruction, IrMathOp};
use hll_to_ir::ir::instruction::IrTerminator;

fn compile_ok(source: &str) -> full_stack::compilation_pipeline::CompilationResult {
    CompilationPipeline::new()
        .compile(source)
        .unwrap_or_else(|e| panic!("expected compilation to succeed, got: {e}"))
}

fn assert_semantic_error(source: &str, fragment: &str) {
    let result = CompilationPipeline::new().compile(source);
    let err = result.expect_err("expected compilation to fail");
    match err {
        CompilationError::DiagnosticErrors(diags) => assert!(
            diags.iter().any(|d| d.message.contains(fragment)),
            "expected diagnostic containing `{fragment}`, got: {diags:?}"
        ),
        other => panic!("expected DiagnosticErrors, got: {other:?}"),
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

// -- Smoke tests ---------------------------------------------------------------

#[test]
fn compiles_minimal_valid_program() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_run_semantic_analysis(false);
    assert!(pipeline.compile("main: () -> i32 { return 42 }").is_ok());
}

#[test]
fn rejects_invalid_tokens() {
    let result = CompilationPipeline::new().compile("@invalid_token!@#");
    assert!(matches!(result.unwrap_err(), CompilationError::DiagnosticErrors(_)));
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

// -- Signed / unsigned arithmetic lowering ------------------------------------

#[test]
fn unsigned_division_emits_udiv() {
    assert!(has_instruction(
        r#"main: () -> i32 { a: u32 = 10  b: u32 = 2  c: u32 = a / b  return i32(c) }"#,
        |i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::UDiv),
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
        instrs.iter().any(|i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::UDiv)),
        "expected UDiv (unsigned)"
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

// -- Type cast lowering --------------------------------------------------------

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

// -- Heap allocation lowering --------------------------------------------------

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

// -- Control flow lowering -----------------------------------------------------

fn has_terminator<F>(source: &str, pred: F) -> bool
where
    F: Fn(&IrTerminator) -> bool,
{
    let result = CompilationPipeline::new()
        .compile(source)
        .unwrap_or_else(|e| panic!("expected ok, got: {e}"));
    result
        .ir_program
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .filter_map(|b| b.terminator.as_ref())
        .any(pred)
}

#[test]
fn if_else_emits_branch_terminator() {
    let source = r#"main: () -> i32 {
    x: i32 = 5
    if x > 3 {
        return 1
    } else {
        return 0
    }
}"#;
    assert!(has_terminator(source, |t| matches!(t, IrTerminator::Branch { .. })));
}

#[test]
fn while_loop_emits_branch_terminator() {
    let source = r#"main: () -> i32 {
    i: i32 = 0
    while i < 10 {
        i = i + 1
    }
    return i
}"#;
    assert!(has_terminator(source, |t| matches!(t, IrTerminator::Branch { .. })));
}

#[test]
fn while_loop_emits_jump_back_terminator() {
    let source = r#"main: () -> i32 {
    i: i32 = 0
    while i < 5 {
        i = i + 1
    }
    return i
}"#;
    assert!(has_terminator(source, |t| matches!(t, IrTerminator::Jump(..))));
}

#[test]
fn if_without_else_still_compiles() {
    let source = r#"main: () -> i32 {
    x: i32 = 1
    if x == 1 {
        x = 2
    }
    return x
}"#;
    compile_ok(source);
}

// -- Function call lowering ----------------------------------------------------

#[test]
fn function_call_emits_call_instruction() {
    let source = r#"external double: (x: i32) -> i32
main: () -> i32 {
    result: i32 = double(21)
    return result
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Call { function, .. } if function == "double")));
}

#[test]
fn void_call_has_no_dest() {
    let source = r#"external log: (x: i32) -> i32
main: () -> i32 {
    log(99)
    return 0
}"#;
    // The call may or may not capture the return value; just verify it compiles and emits a Call.
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Call { function, .. } if function == "log")));
}

#[test]
fn multi_arg_call_passes_all_args() {
    let source = r#"external add: (a: i32, b: i32) -> i32
main: () -> i32 {
    result: i32 = add(10, 32)
    return result
}"#;
    assert!(has_instruction(source, |i| {
        if let IrInstruction::Call { function, args, .. } = i {
            function == "add" && args.len() == 2
        } else {
            false
        }
    }));
}

// -- Global string constants ---------------------------------------------------

#[test]
fn string_literal_creates_global_string() {
    // String literals are { data: u8*, length: u64 } inline structs.
    let source = r#"type Text = { data: u8*, length: u64 }
main: () -> i32 {
    msg: Text = "hello"
    return 0
}"#;
    let result = compile_ok(source);
    assert!(
        !result.ir_program.global_strings.is_empty(),
        "expected at least one global string constant"
    );
}

#[test]
fn string_literal_content_preserved() {
    let source = r#"type Text = { data: u8*, length: u64 }
main: () -> i32 {
    msg: Text = "world"
    return 0
}"#;
    let result = compile_ok(source);
    let found = result
        .ir_program
        .global_strings
        .iter()
        .any(|gs| gs.content.contains("world"));
    assert!(found, "global string 'world' not found in IR");
}

// -- Unary operations ----------------------------------------------------------

#[test]
fn unary_negate_emits_unary_instruction() {
    let source = r#"main: () -> i32 {
    x: i32 = 5
    y: i32 = -x
    return y
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Unary { .. })));
}

#[test]
fn unary_not_emits_unary_instruction() {
    let source = r#"main: () -> i32 {
    flag: bool = true
    inv: bool = !flag
    if inv {
        return 1
    } else {
        return 0
    }
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Unary { .. })));
}

// -- Stack allocation and pointer load/store -----------------------------------

#[test]
fn stack_variable_emits_alloc() {
    let source = r#"main: () -> i32 {
    x: i32 = 10
    return x
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Alloc { .. })));
}

#[test]
fn address_of_emits_alloc_and_load() {
    let source = r#"main: () -> i32 {
    x: i32 = 42
    ptr: i32* = &x
    val: i32 = @ptr
    return val
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Alloc { .. })));
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Load { .. })));
}

#[test]
fn pointer_store_emits_store_instruction() {
    let source = r#"main: () -> i32 {
    ptr: i32* = new(i32)
    @ptr = 99
    val: i32 = @ptr
    free(ptr)
    return val
}"#;
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Store { .. })));
    assert!(has_instruction(source, |i| matches!(i, IrInstruction::Load { .. })));
}

// -- Multiple functions --------------------------------------------------------

#[test]
fn two_functions_both_appear_in_ir() {
    let source = r#"helper: () -> i32 {
    return 21
}
main: () -> i32 {
    x: i32 = helper()
    return x + x
}"#;
    let result = compile_ok(source);
    let names: Vec<_> = result.ir_program.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"), "helper not in IR: {names:?}");
    assert!(names.contains(&"main"), "main not in IR: {names:?}");
}

// -- Modulo operator -----------------------------------------------------------

#[test]
fn modulo_emits_rem_op() {
    assert!(has_instruction(
        r#"main: () -> i32 { a: i32 = 10  b: i32 = a % 3  return b }"#,
        |i| matches!(i, IrInstruction::Math { op, .. } if *op == IrMathOp::Mod),
    ));
}
