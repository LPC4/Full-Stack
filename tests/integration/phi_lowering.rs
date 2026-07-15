//! Backend phi lowering: hand-built IR (the HLL front end does not emit phi
//! yet) exercises edge copies, loop back-edges, and parallel-copy safety.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::TargetMode;
use hll_to_ir::{
    IntWidth, IrBlock, IrCmpOp, IrFunction, IrInstruction, IrLabel, IrMathOp, IrProgram,
    IrRegister, IrTerminator, IrType, IrValue,
};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn cached_stdlib_objs() -> &'static [(String, AssembledOutput)] {
    static STDLIB: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    STDLIB.get_or_init(|| {
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
            .expect("stdlib compile failed")
    })
}

fn run_ir(program: &IrProgram, regalloc: bool) -> i64 {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_register_allocation(regalloc);
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(program);
    let user_obj = pipeline
        .assemble(&user_tokens)
        .expect("user assemble failed");
    let mut modules: Vec<(&str, &AssembledOutput)> = cached_stdlib_objs()
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    match run.outcome {
        StepOutcome::Halted(code) => code,
        StepOutcome::Continue => panic!("program did not halt; uart: {}", run.uart_output),
    }
}

fn reg(name: &str) -> IrRegister {
    IrRegister::Named(name.to_owned())
}

fn i64_ty() -> IrType {
    IrType::Integer(IntWidth::I64)
}

/// `main` returning the phi-merged value of a two-way branch on `cond`.
fn branch_merge_program(cond: bool) -> IrProgram {
    let mut program = IrProgram::new("phi_merge");
    let mut func = IrFunction::new("main", IrType::Integer(IntWidth::I32));

    let mut entry = IrBlock::new("entry");
    entry.set_terminator(IrTerminator::Branch {
        cond: IrValue::Bool(cond),
        then_label: IrLabel::new("take_then"),
        else_label: IrLabel::new("take_else"),
    });
    func.push_block(entry);

    let mut then_block = IrBlock::new("take_then");
    then_block.set_terminator(IrTerminator::Jump(IrLabel::new("merge")));
    func.push_block(then_block);

    let mut else_block = IrBlock::new("take_else");
    else_block.set_terminator(IrTerminator::Jump(IrLabel::new("merge")));
    func.push_block(else_block);

    let mut merge = IrBlock::new("merge");
    merge.push_instruction(IrInstruction::Phi {
        dest: reg("r"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(11), IrLabel::new("take_then")),
            (IrValue::Integer(22), IrLabel::new("take_else")),
        ],
    });
    merge.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("r")))));
    func.push_block(merge);

    program.push_function(func);
    program
}

#[test]
fn phi_selects_the_taken_edge_value() {
    assert_eq!(run_ir(&branch_merge_program(true), true), 11);
    assert_eq!(run_ir(&branch_merge_program(false), true), 22);
    assert_eq!(run_ir(&branch_merge_program(true), false), 11);
    assert_eq!(run_ir(&branch_merge_program(false), false), 22);
}

/// Three loop-header phis: a counter plus a swapping pair. The swap only
/// survives if back-edge copies have parallel-assignment semantics.
fn swap_loop_program(iterations: i64) -> IrProgram {
    let mut program = IrProgram::new("phi_swap");
    let mut func = IrFunction::new("main", IrType::Integer(IntWidth::I32));

    let mut entry = IrBlock::new("entry");
    entry.set_terminator(IrTerminator::Jump(IrLabel::new("header")));
    func.push_block(entry);

    let mut header = IrBlock::new("header");
    header.push_instruction(IrInstruction::Phi {
        dest: reg("i"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(0), IrLabel::new("entry")),
            (IrValue::Register(reg("i_next")), IrLabel::new("body")),
        ],
    });
    header.push_instruction(IrInstruction::Phi {
        dest: reg("a"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(1), IrLabel::new("entry")),
            (IrValue::Register(reg("b")), IrLabel::new("body")),
        ],
    });
    header.push_instruction(IrInstruction::Phi {
        dest: reg("b"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(2), IrLabel::new("entry")),
            (IrValue::Register(reg("a")), IrLabel::new("body")),
        ],
    });
    header.push_instruction(IrInstruction::Cmp {
        dest: reg("more"),
        op: IrCmpOp::Slt,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("i")),
        rhs: IrValue::Integer(iterations),
    });
    header.set_terminator(IrTerminator::Branch {
        cond: IrValue::Register(reg("more")),
        then_label: IrLabel::new("body"),
        else_label: IrLabel::new("exit"),
    });
    func.push_block(header);

    let mut body = IrBlock::new("body");
    body.push_instruction(IrInstruction::Math {
        dest: reg("i_next"),
        op: IrMathOp::Add,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("i")),
        rhs: IrValue::Integer(1),
    });
    body.set_terminator(IrTerminator::Jump(IrLabel::new("header")));
    func.push_block(body);

    let mut exit = IrBlock::new("exit");
    exit.push_instruction(IrInstruction::Math {
        dest: reg("a10"),
        op: IrMathOp::Mul,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("a")),
        rhs: IrValue::Integer(10),
    });
    exit.push_instruction(IrInstruction::Math {
        dest: reg("code"),
        op: IrMathOp::Add,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("a10")),
        rhs: IrValue::Register(reg("b")),
    });
    exit.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("code")))));
    func.push_block(exit);

    program.push_function(func);
    program
}

#[test]
fn loop_phis_swap_without_clobbering() {
    // (a, b) starts (1, 2) and swaps each iteration; code = a * 10 + b.
    assert_eq!(run_ir(&swap_loop_program(0), true), 12);
    assert_eq!(run_ir(&swap_loop_program(1), true), 21);
    assert_eq!(run_ir(&swap_loop_program(2), true), 12);
    assert_eq!(run_ir(&swap_loop_program(3), true), 21);
    assert_eq!(run_ir(&swap_loop_program(3), false), 21);
}

/// A counter accumulated purely through loop-carried phis.
fn counter_loop_program(limit: i64) -> IrProgram {
    let mut program = IrProgram::new("phi_counter");
    let mut func = IrFunction::new("main", IrType::Integer(IntWidth::I32));

    let mut entry = IrBlock::new("entry");
    entry.set_terminator(IrTerminator::Jump(IrLabel::new("header")));
    func.push_block(entry);

    let mut header = IrBlock::new("header");
    header.push_instruction(IrInstruction::Phi {
        dest: reg("i"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(0), IrLabel::new("entry")),
            (IrValue::Register(reg("i_next")), IrLabel::new("body")),
        ],
    });
    header.push_instruction(IrInstruction::Phi {
        dest: reg("sum"),
        ty: i64_ty(),
        incoming: vec![
            (IrValue::Integer(0), IrLabel::new("entry")),
            (IrValue::Register(reg("sum_next")), IrLabel::new("body")),
        ],
    });
    header.push_instruction(IrInstruction::Cmp {
        dest: reg("more"),
        op: IrCmpOp::Slt,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("i")),
        rhs: IrValue::Integer(limit),
    });
    header.set_terminator(IrTerminator::Branch {
        cond: IrValue::Register(reg("more")),
        then_label: IrLabel::new("body"),
        else_label: IrLabel::new("exit"),
    });
    func.push_block(header);

    let mut body = IrBlock::new("body");
    body.push_instruction(IrInstruction::Math {
        dest: reg("i_next"),
        op: IrMathOp::Add,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("i")),
        rhs: IrValue::Integer(1),
    });
    body.push_instruction(IrInstruction::Math {
        dest: reg("sum_next"),
        op: IrMathOp::Add,
        ty: i64_ty(),
        lhs: IrValue::Register(reg("sum")),
        rhs: IrValue::Register(reg("i_next")),
    });
    body.set_terminator(IrTerminator::Jump(IrLabel::new("header")));
    func.push_block(body);

    let mut exit = IrBlock::new("exit");
    exit.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("sum")))));
    func.push_block(exit);

    program.push_function(func);
    program
}

#[test]
fn loop_carried_phi_accumulates() {
    // sum of 1..=8 = 36, with and without register allocation.
    assert_eq!(run_ir(&counter_loop_program(8), true), 36);
    assert_eq!(run_ir(&counter_loop_program(8), false), 36);
}
