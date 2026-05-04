use full_stack::intermediate_language::asm_compiler::function_context::FunctionContext;
use full_stack::intermediate_language::asm_compiler::register_allocator::{
    Allocation, RegisterAllocator,
};
use full_stack::intermediate_language::{
    IntWidth, IrBlock, IrFunction, IrInstruction, IrMathOp, IrRegister, IrTerminator, IrType,
    IrValue,
};
use std::collections::HashMap;

fn int32() -> IrType {
    IrType::Integer(IntWidth::I32)
}

fn reg(name: &str) -> IrRegister {
    IrRegister::Named(name.to_owned())
}

fn lit_math(dest: &str, lhs: i64, rhs: i64) -> IrInstruction {
    IrInstruction::Math {
        dest: reg(dest),
        op: IrMathOp::Add,
        ty: int32(),
        lhs: IrValue::Integer(lhs),
        rhs: IrValue::Integer(rhs),
    }
}

fn allocate_function(func: &IrFunction) -> (RegisterAllocator, FunctionContext) {
    let mut allocator = RegisterAllocator::new();
    let mut ctx = FunctionContext::new("test", &HashMap::new());
    allocator.allocate_slots(func, &mut ctx, &HashMap::new());
    (allocator, ctx)
}

#[test]
fn linear_allocator_places_short_lived_ints_in_physical_registers() {
    let mut func = IrFunction::new("main", int32());
    let mut block = IrBlock::new("entry");
    block.push_instruction(lit_math("a", 1, 2));
    block.push_instruction(lit_math("b", 3, 4));
    block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("b")))));
    func.push_block(block);

    let (allocator, ctx) = allocate_function(&func);

    assert!(
        ctx.slot_for_reg(&reg("a")).is_some(),
        "stack slot should exist for a"
    );
    assert!(
        ctx.slot_for_reg(&reg("b")).is_some(),
        "stack slot should exist for b"
    );
    assert!(matches!(
        allocator.get_allocation(&reg("a")),
        Some(Allocation::Physical(_))
    ));
    assert!(matches!(
        allocator.get_allocation(&reg("b")),
        Some(Allocation::Physical(_))
    ));
}

#[test]
fn linear_allocator_spills_after_register_pressure_exceeds_available_regs() {
    let mut func = IrFunction::new("main", int32());
    let mut block = IrBlock::new("entry");

    for index in 0..8 {
        block.push_instruction(lit_math(
            &format!("t{index}"),
            index as i64,
            index as i64 + 1,
        ));
    }

    block.push_instruction(IrInstruction::Call {
        dest: None,
        function: "sink".to_owned(),
        args: (0..8)
            .map(|index| IrValue::Register(reg(&format!("t{index}"))))
            .collect(),
    });
    block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
    func.push_block(block);

    let (allocator, ctx) = allocate_function(&func);

    for index in 0..7 {
        let name = format!("t{index}");
        assert!(
            matches!(
                allocator.get_allocation(&reg(&name)),
                Some(Allocation::Physical(_))
            ),
            "expected {name} to fit in a physical register"
        );
    }

    let spilled = reg("t7");
    assert!(matches!(
        allocator.get_allocation(&spilled),
        Some(Allocation::StackSlot(_))
    ));
    assert!(
        ctx.slot_for_reg(&spilled).is_some(),
        "spilled register still needs a stack slot"
    );
}

#[test]
fn linear_allocator_leaves_stack_address_registers_on_the_stack() {
    let mut func = IrFunction::new("main", int32());
    let mut block = IrBlock::new("entry");
    block.push_instruction(IrInstruction::Alloc {
        dest: reg("ptr"),
        ty: int32(),
        count: None,
    });
    block.push_instruction(IrInstruction::Store {
        ty: int32(),
        value: IrValue::Integer(42),
        ptr: reg("ptr"),
        offset: None,
    });
    block.set_terminator(IrTerminator::Return(None));
    func.push_block(block);

    let (allocator, ctx) = allocate_function(&func);

    assert!(
        ctx.is_stack_address(&reg("ptr")),
        "alloc destinations are stack addresses"
    );
    assert!(
        ctx.slot_for_reg(&reg("ptr")).is_some(),
        "stack address registers still need slots"
    );
    assert!(
        allocator.get_allocation(&reg("ptr")).is_none(),
        "stack address registers should not be assigned a physical register"
    );
}

#[test]
fn linear_allocator_reuses_a_register_after_an_interval_expires() {
    let mut func = IrFunction::new("main", int32());
    let mut block = IrBlock::new("entry");
    block.push_instruction(lit_math("first", 1, 2));
    block.push_instruction(IrInstruction::Call {
        dest: None,
        function: "sink".to_owned(),
        args: vec![IrValue::Register(reg("first"))],
    });
    block.push_instruction(lit_math("second", 3, 4));
    block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("second")))));
    func.push_block(block);

    let (allocator, _) = allocate_function(&func);

    let first = allocator.get_allocation(&reg("first"));
    let second = allocator.get_allocation(&reg("second"));

    match (first, second) {
        (Some(Allocation::Physical(a)), Some(Allocation::Physical(b))) => {
            assert_eq!(
                a, b,
                "expired intervals should release their register for reuse"
            );
        }
        other => panic!("unexpected allocations: {other:?}"),
    }
}
