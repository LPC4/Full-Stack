use full_stack::intermediate_language::asm_compiler::function_context::FunctionContext;
use full_stack::intermediate_language::asm_compiler::register_allocator::{
    Allocation, RegisterAllocator,
};
use full_stack::intermediate_language::{
    IntWidth, IrBlock, IrFunction, IrInstruction, IrMathOp, IrRegister, IrTerminator, IrType,
    IrValue,
};
use std::collections::HashMap;

fn i64_ty() -> IrType {
    IrType::Integer(IntWidth::I64)
}

/// A 4-field struct of i64 — mirrors HeapBlock (next, ptr, size, is_free).
/// Size = 4 × 8 = 32 bytes.
fn heap_block_ty() -> IrType {
    IrType::Aggregate(vec![
        ("next".to_string(), i64_ty()),
        ("ptr".to_string(), i64_ty()),
        ("size".to_string(), i64_ty()),
        ("is_free".to_string(), i64_ty()),
    ])
}

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

// ---------------------------------------------------------------------------
// Regression: Alloc slot must be sized by the inner struct type, not pointer.
//
// Before the fix, IrInstruction::Alloc { dest, ty: HeapBlock, .. } was given
// an 8-byte slot (pointer size) instead of 32 bytes (struct size).  The next
// register's slot then overlapped the struct's second field, corrupting struct
// literals in malloc and causing the heap_list load to target an invalid address.
// ---------------------------------------------------------------------------

/// `alloc_slot_for_alloc` with a 32-byte struct must reserve ≥ 32 bytes.
/// The frame size after allocation must be at least 32 bytes.
#[test]
fn alloc_slot_for_struct_reserves_full_struct_size() {
    let mut ctx = FunctionContext::new("test", &HashMap::new());
    let struct_ty = heap_block_ty(); // 4 × i64 = 32 bytes
    ctx.alloc_slot_for_alloc(&reg("block"), &struct_ty, None);
    ctx.finalize();
    assert!(
        ctx.frame_size() >= 32,
        "frame should be at least 32 bytes for a 32-byte struct Alloc, got {}",
        ctx.frame_size()
    );
}

/// After allocating a 32-byte struct Alloc followed by an 8-byte pointer, the
/// two slots must not overlap.  Before the fix, the pointer was placed at
/// struct_offset + 8 (overlapping fields 2–4 of the struct).
#[test]
fn alloc_slot_for_struct_does_not_overlap_next_slot() {
    let mut func = IrFunction::new("main", int32());
    let mut block = IrBlock::new("entry");

    // $37 = Alloc HeapBlock  (needs 32 bytes)
    block.push_instruction(IrInstruction::Alloc {
        dest: reg("block"),
        ty: heap_block_ty(),
        count: None,
    });
    // $38 = Alloc i64 pointer  (8 bytes) — this is the register that used to collide
    block.push_instruction(IrInstruction::Alloc {
        dest: reg("ptr"),
        ty: i64_ty(),
        count: None,
    });
    block.set_terminator(IrTerminator::Return(None));
    func.push_block(block);

    let (_, ctx) = allocate_function(&func);

    let slot_block = ctx.slot_for_reg(&reg("block")).expect("block must have a slot");
    let slot_ptr = ctx.slot_for_reg(&reg("ptr")).expect("ptr must have a slot");

    // The struct occupies [slot_block, slot_block + 32).
    // The pointer slot must not fall inside that range.
    let block_end = slot_block + 32;
    assert!(
        slot_ptr >= block_end || slot_ptr + 8 <= slot_block,
        "struct slot [{slot_block}, {block_end}) overlaps pointer slot [{slot_ptr}, {}): \
         regression — Alloc struct was given only 8 bytes instead of 32",
        slot_ptr + 8
    );
}
