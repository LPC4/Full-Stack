use ir_to_asm::compiler::compiler_rv64::CompilerRv64;
use hll_to_ir::{
    IrBlock, IrFunction, IrInstruction, IrLabel, IrParam, IrProgram, IrRegister, IrTerminator,
    IrType, IrValue, IntWidth,
};

fn int32() -> IrType {
    IrType::Integer(IntWidth::I32)
}

fn int64() -> IrType {
    IrType::Integer(IntWidth::I64)
}

fn compile(program: &IrProgram) -> String {
    CompilerRv64::new().compile(program)
}

#[test]
fn emits_symbolic_labels_for_calls_and_branches() {
    let mut program = IrProgram::new("test");

    let mut callee = IrFunction::new("callee", int32());
    let mut callee_entry = IrBlock::new("entry");
    callee_entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(7))));
    callee.push_block(callee_entry);
    program.push_function(callee);

    let mut main = IrFunction::new("main", int32());
    main.push_param(IrParam {
        ty: IrType::Integer(IntWidth::I1),
        register: IrRegister::Named("cond".into()),
    });

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Call {
        dest: None,
        function: "callee".into(),
        args: vec![],
    });
    entry.set_terminator(IrTerminator::Branch {
        cond: IrValue::Register(IrRegister::Named("cond".into())),
        then_label: IrLabel::new("then"),
        else_label: IrLabel::new("else"),
    });
    main.push_block(entry);

    let mut then_block = IrBlock::new("then");
    then_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(1))));
    main.push_block(then_block);

    let mut else_block = IrBlock::new("else");
    else_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
    main.push_block(else_block);
    program.push_function(main);

    let asm = compile(&program);

    assert!(asm.contains("callee"), "expected symbolic call target, got:\n{asm}");
    assert!(asm.contains("main__else"), "expected symbolic branch target, got:\n{asm}");
    assert!(asm.contains("main__entry:"), "expected function-scoped block label, got:\n{asm}");
    assert!(asm.contains("main__then:"), "expected function-scoped then label, got:\n{asm}");
    assert!(asm.contains("main__else:"), "expected function-scoped else label, got:\n{asm}");
}

#[test]
fn emits_standard_prologue_and_argument_spills() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("spill_args", int32());

    for i in 0..9 {
        func.push_param(IrParam {
            ty: int64(),
            register: IrRegister::Named(format!("arg{i}")),
        });
    }

    let mut entry = IrBlock::new("entry");
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
    func.push_block(entry);
    program.push_function(func);

    let asm = compile(&program);

    assert!(asm.contains("addi") && asm.contains("sp, sp, -"), "expected stack allocation in prologue, got:\n{asm}");
    assert!(asm.contains("sd") && asm.contains("ra,") && asm.contains("(sp)"), "expected ra save in prologue, got:\n{asm}");
    assert!(asm.contains("sd") && asm.contains("s0,") && asm.contains("(sp)"), "expected s0 save in prologue, got:\n{asm}");
    assert!(asm.contains("addi") && asm.contains("s0, sp, 0"), "expected frame pointer initialization, got:\n{asm}");
    assert!(asm.contains("a0") && asm.contains("sd"), "expected first argument spill, got:\n{asm}");
    assert!(asm.contains("a7") && asm.contains("sd"), "expected eighth argument spill, got:\n{asm}");
    assert!(asm.contains("ld") && asm.contains("t0"), "expected stack-passed argument load, got:\n{asm}");
}

#[test]
fn uses_lw_and_sw_for_i32_stack_values() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("arith", int32());

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("x".into()),
        ty: int32(),
        count: None,
    });
    entry.push_instruction(IrInstruction::Store {
        ty: int32(),
        value: IrValue::Integer(42),
        ptr: IrRegister::Named("x".into()),
        offset: None,
    });
    entry.push_instruction(IrInstruction::Load {
        dest: IrRegister::Named("y".into()),
        ty: int32(),
        ptr: IrRegister::Named("x".into()),
        offset: None,
    });
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Register(IrRegister::Named(
        "y".into(),
    )))));
    func.push_block(entry);
    program.push_function(func);

    let asm = compile(&program);

    assert!(asm.contains("\tsw") || asm.contains("sw "), "expected i32 store to use sw, got:\n{asm}");
    assert!(asm.contains("\tlw") || asm.contains("lw "), "expected i32 load to use lw, got:\n{asm}");
}

#[test]
fn keeps_pointer_stack_values_64_bit() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("ptrs", int32());

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::HeapAlloc {
        dest: IrRegister::Named("heap".into()),
        ty: int32(),
        count: None,
    });
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("holder".into()),
        ty: IrType::Pointer(Box::new(int32())),
        count: None,
    });
    entry.push_instruction(IrInstruction::Store {
        ty: IrType::Pointer(Box::new(int32())),
        value: IrValue::Register(IrRegister::Named("heap".into())),
        ptr: IrRegister::Named("holder".into()),
        offset: None,
    });
    entry.push_instruction(IrInstruction::Load {
        dest: IrRegister::Named("alias".into()),
        ty: IrType::Pointer(Box::new(int32())),
        ptr: IrRegister::Named("holder".into()),
        offset: None,
    });
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
    func.push_block(entry);
    program.push_function(func);

    let asm = compile(&program);

    assert!(asm.contains("\tsd") || asm.contains("sd "), "expected pointer store to use sd, got:\n{asm}");
    assert!(asm.contains("\tld") || asm.contains("ld "), "expected pointer load to use ld, got:\n{asm}");
}

#[test]
fn lowers_stack_alloc_as_frame_address_not_memory_load() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("stack_alloc", int32());

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("local".into()),
        ty: int32(),
        count: None,
    });
    entry.push_instruction(IrInstruction::Store {
        ty: int32(),
        value: IrValue::Integer(42),
        ptr: IrRegister::Named("local".into()),
        offset: None,
    });
    entry.set_terminator(IrTerminator::Return(None));
    func.push_block(entry);
    program.push_function(func);

    let asm = compile(&program);

    assert!(asm.contains("addi") && asm.contains("sp, 0"), "expected stack allocation to lower to frame-address arithmetic, got:\n{asm}");
}

#[test]
fn omits_destination_for_void_calls() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("cleanup", int32());

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("ptr".into()),
        ty: IrType::Pointer(Box::new(int32())),
        count: None,
    });
    entry.push_instruction(IrInstruction::Call {
        dest: None,
        function: "free".into(),
        args: vec![IrValue::Register(IrRegister::Named("ptr".into()))],
    });
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
    func.push_block(entry);
    program.push_function(func);

    let asm = compile(&program);

    assert!(asm.contains("free"), "expected call target to be emitted, got:\n{asm}");
    assert!(
        !asm.contains("sd     a0") && !asm.contains("sd a0"),
        "expected no destination store for void call, got:\n{asm}"
    );
}

