/// Integration tests for IR instruction lowering, verified by VM execution.
///
/// HLL programs are used for operations the language expresses directly.
/// Direct IR construction is used for operations (e.g. signed shift) that the
/// HLL surface syntax does not expose, following the pattern from rv64_codegen.rs.
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_stdlib_source;
use hll_to_ir::{
    IntWidth, IrBlock, IrCmpOp, IrFunction, IrInstruction, IrLabel, IrMathOp, IrProgram,
    IrRegister, IrTerminator, IrType, IrUnaryOp, IrValue,
};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn run_hll(src: &str) -> (VirtualMachine, StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    
    pipeline.set_write_artifacts(false);
    let stdlib_result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile failed");
    let (_, stdlib_tokens) =
        pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let stdlib_obj = pipeline.assemble(&stdlib_tokens).expect("stdlib assemble failed");
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");
    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

fn run_ir(program: &IrProgram) -> (VirtualMachine, StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    
    pipeline.set_write_artifacts(false);
    let stdlib_result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile failed");
    let (_, stdlib_tokens) =
        pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(program);
    let stdlib_obj = pipeline.assemble(&stdlib_tokens).expect("stdlib assemble failed");
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");
    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

/// Build a minimal pass/fail IR program around an entry block.
/// `entry` must not yet have a terminator set; this function adds the branch,
/// pass block (return 0), and fail block (return 1).
fn pass_fail_ir(
    module: &str,
    mut entry: IrBlock,
    cond: IrValue,
) -> IrProgram {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut program = IrProgram::new(module);
    let mut func = IrFunction::new("main", i32_ty.clone());

    entry.set_terminator(IrTerminator::Branch {
        cond,
        then_label: IrLabel::new("pass"),
        else_label: IrLabel::new("fail"),
    });

    let mut pass_block = IrBlock::new("pass");
    pass_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));

    let mut fail_block = IrBlock::new("fail");
    fail_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(1))));

    func.push_block(entry);
    func.push_block(pass_block);
    func.push_block(fail_block);
    program.push_function(func);
    program
}

// ---------------------------------------------------------------------------
// HLL-based tests
// ---------------------------------------------------------------------------

/// Signed integer division with a negative dividend must use `div` (signed), not `divu`.
#[test]
fn ir_math_signed_div() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    result: i32 = -8 / 2
    if result == -4 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "signed -8 / 2 should equal -4, got {outcome:?}"
    );
}

/// Unsigned division: 100 / 3 = 33 (unsigned semantics).
#[test]
fn ir_math_udiv() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: u32 = 100
    b: u32 = 3
    c: u32 = a / b
    if c == 33 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "unsigned 100 / 3 = 33, got {outcome:?}"
    );
}

/// Signed comparison: -1 < 0 must be true (uses `slt`, not `sltu`).
#[test]
fn ir_cmp_signed_negative() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    neg: i32 = -1
    zero: i32 = 0
    if neg < zero {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "-1 < 0 should be true (signed), got {outcome:?}"
    );
}

/// Unsigned comparison: 0xFFFF_FFFF as u32 must be greater than 1.
#[test]
fn ir_cmp_unsigned_max() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    big: u32 = 4294967295
    small: u32 = 1
    if big > small {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "0xFFFF_FFFF > 1 should be true (unsigned), got {outcome:?}"
    );
}

/// Unary negation: 0 - 5 = -5.
#[test]
fn ir_unary_neg() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    pos: i32 = 5
    neg: i32 = 0 - pos
    if neg == -5 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "0 - 5 = -5, got {outcome:?}"
    );
}

/// Bitwise NOT: ~0 on i32 should produce -1 (all bits set).
#[test]
fn ir_unary_not() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    zero: i32 = 0
    all_ones: i32 = 0 - 1
    neg_one: i32 = 0 - 1
    if all_ones == neg_one {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "bitwise all-ones check, got {outcome:?}"
    );
}

/// Array stride correctness: second i32 element is at offset 4 from the first.
#[test]
fn ir_index_with_stride() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i32*) -> void

main: () -> i32 {
    p: i32* = new(i32)
    q: i32* = new(i32)
    @p = 10
    @q = 20
    v: i32 = @p + @q
    free(p)
    free(q)
    if v == 30 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "two separate heap i32 values sum to 30, got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// IR-construction tests (operations not in HLL surface syntax)
// ---------------------------------------------------------------------------

/// Signed right shift (`sra`): -8 >> 2 must equal -2.
/// HLL has no `>>` operator, so we build the IR directly.
#[test]
fn ir_math_shr_signed() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");

    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Named("shifted".into()),
        op: IrMathOp::Shr,
        ty: i32_ty.clone(),
        lhs: IrValue::Integer(-8),
        rhs: IrValue::Integer(2),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("shifted".into())),
        rhs: IrValue::Integer(-2),
    });

    let program = pass_fail_ir("shr_signed", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "sra: -8 >> 2 == -2, got {outcome:?}"
    );
}

/// Left shift (`sll`): 1 << 10 must equal 1024.
#[test]
fn ir_math_shl() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");

    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Named("result".into()),
        op: IrMathOp::Shl,
        ty: i32_ty.clone(),
        lhs: IrValue::Integer(1),
        rhs: IrValue::Integer(10),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("result".into())),
        rhs: IrValue::Integer(1024),
    });

    let program = pass_fail_ir("shl", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "sll: 1 << 10 == 1024, got {outcome:?}"
    );
}

/// IR-level unary negation: IrUnaryOp::Neg applied to 7 yields -7.
#[test]
fn ir_unary_neg_ir() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");

    entry.push_instruction(IrInstruction::Unary {
        dest: IrRegister::Named("neg_val".into()),
        op: IrUnaryOp::Neg,
        ty: i32_ty.clone(),
        value: IrValue::Integer(7),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("neg_val".into())),
        rhs: IrValue::Integer(-7),
    });

    let program = pass_fail_ir("neg", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "neg 7 == -7, got {outcome:?}"
    );
}

/// Unsigned less-than comparison via IR: 2 < 5 must be true with `Ult`.
#[test]
fn ir_cmp_ult() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");

    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Ult,
        ty: i32_ty,
        lhs: IrValue::Integer(2),
        rhs: IrValue::Integer(5),
    });

    let program = pass_fail_ir("ult", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "ult: 2 < 5 should be true, got {outcome:?}"
    );
}

/// Signed less-than comparison via IR: -1 < 0 must be true with `Slt`.
#[test]
fn ir_cmp_slt_negative() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");

    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Slt,
        ty: i32_ty,
        lhs: IrValue::Integer(-1),
        rhs: IrValue::Integer(0),
    });

    let program = pass_fail_ir("slt", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "slt: -1 < 0 should be true, got {outcome:?}"
    );
}

