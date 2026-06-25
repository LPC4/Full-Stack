//! Stack-slot planning. The RV64 backend is stack-only: every IR value lives in
//! a frame slot. This collects the typed virtual registers, reserves dedicated
//! full-size slots for address-taken `Alloc` storage, and hands the rest to the
//! slot-coloring pass (which shares one slot across registers whose live ranges
//! do not overlap). See the crate README.

use super::function_context::FunctionContext;
use hll_to_ir::{IrFunction, IrInstruction, IrTerminator, IrType, IrValue};
use std::collections::HashMap;

/// Reserve stack slots for every virtual register. See _IR_SPECIFICATIONS.md for layout details.
///
/// Alloc dests get dedicated full-size slots; hot scalar regs get phys regs first.
/// Remaining regs are slot-colored; `needs_sret` marks functions with hidden sret param.
pub fn assign_stack_slots(
    func: &IrFunction,
    ctx: &mut FunctionContext,
    function_return_types: &HashMap<String, IrType>,
    regalloc: bool,
    needs_sret: bool,
) {
    // Pre-allocate Alloc destinations first so struct allocs get
    // (type_size * count) bytes rather than the 8-byte pointer size.
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Alloc { dest, ty, count } = inst {
                ctx.mark_stack_address(dest);
                ctx.alloc_slot_for_alloc(dest, ty, *count);
            }
        }
    }

    let mut vregs = Vec::new();
    for param in &func.params {
        vregs.push((param.register.clone(), param.ty.clone()));
    }
    for block in &func.blocks {
        for inst in &block.instructions {
            collect_vregs_from_instruction(inst, &mut vregs, function_return_types);
        }
        if let Some(term) = &block.terminator {
            collect_vregs_from_terminator(term, &mut vregs);
        }
    }

    // A register stored as a composite is a composite even if its producer was
    // typed as a scalar (e.g. an external aggregate-returning call); upgrade it.
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Store {
                ty,
                value: IrValue::Register(reg),
                ..
            } = inst
                && matches!(
                    ty,
                    IrType::Array { .. } | IrType::Aggregate(_) | IrType::Slice(_)
                )
                && let Some(entry) = vregs.iter_mut().find(|(r, _)| r == reg)
                && !matches!(
                    entry.1,
                    IrType::Array { .. } | IrType::Aggregate(_) | IrType::Slice(_)
                )
            {
                entry.1 = ty.clone();
            }
        }
    }

    // Physical register allocation claims the hottest scalars first; whatever
    // it leaves behind falls through to slot coloring.
    if regalloc {
        super::register_allocator::allocate_registers(func, ctx, &vregs, needs_sret);
    }

    // Slot coloring gives every remaining register a slot, sharing where live
    // ranges allow.
    super::slot_coloring::assign_colored_slots(func, ctx, &vregs);
}

// Record the destination register (and its type) of every value-producing
// instruction so the coloring pass can size and share its slot.
fn collect_vregs_from_instruction(
    inst: &IrInstruction,
    vregs: &mut Vec<(hll_to_ir::IrRegister, IrType)>,
    function_return_types: &HashMap<String, IrType>,
) {
    use IrInstruction::{
        Alloc, Call, Cast, Cmp, GlobalRef, HeapAlloc, Index, Load, Math, Offset, Phi, ReadReg,
        Unary,
    };

    match inst {
        Alloc { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                let ptr_ty = IrType::Pointer(Box::new(ty.clone()));
                vregs.push((dest.clone(), ptr_ty));
            }
        }
        Load { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), ty.clone()));
            }
        }
        Math { dest, ty, .. } | Unary { dest, ty, .. } | Cast { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), ty.clone()));
            }
        }
        Cmp { dest, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), IrType::Integer(hll_to_ir::IntWidth::I1)));
            }
        }
        Call { dest, function, .. } => {
            if let Some(dest) = dest
                && !vregs.iter().any(|(r, _)| r == dest)
            {
                let ret_ty = function_return_types
                    .get(function)
                    .cloned()
                    .unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64));
                vregs.push((dest.clone(), ret_ty));
            }
        }
        Phi { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), ty.clone()));
            }
        }
        Offset { dest, ty, .. } | Index { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), IrType::Pointer(Box::new(ty.clone()))));
            }
        }
        HeapAlloc { dest, ty, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), IrType::Pointer(Box::new(ty.clone()))));
            }
        }
        ReadReg { dest, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((dest.clone(), IrType::Integer(hll_to_ir::IntWidth::I64)));
            }
        }
        GlobalRef { dest, .. } => {
            if !vregs.iter().any(|(r, _)| r == dest) {
                vregs.push((
                    dest.clone(),
                    IrType::Pointer(Box::new(IrType::Integer(hll_to_ir::IntWidth::I8))),
                ));
            }
        }
        _ => {}
    }
}

// Record registers a terminator reads so they are guaranteed a slot.
fn collect_vregs_from_terminator(
    term: &IrTerminator,
    vregs: &mut Vec<(hll_to_ir::IrRegister, IrType)>,
) {
    use IrTerminator::{Branch, Return};
    match term {
        Return(Some(val)) => {
            if let IrValue::Register(reg) = val
                && !vregs.iter().any(|(r, _)| r == reg)
            {
                vregs.push((reg.clone(), IrType::Integer(hll_to_ir::IntWidth::I64)));
            }
        }
        Branch { cond, .. } => {
            if let IrValue::Register(reg) = cond
                && !vregs.iter().any(|(r, _)| r == reg)
            {
                vregs.push((reg.clone(), IrType::Integer(hll_to_ir::IntWidth::I1)));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use hll_to_ir::{
        IntWidth, IrBlock, IrFunction, IrInstruction, IrRegister, IrTerminator, IrType,
    };

    use super::assign_stack_slots;
    use crate::compiler::function_context::FunctionContext;

    fn i64_ty() -> IrType {
        IrType::Integer(IntWidth::I64)
    }

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

    #[test]
    fn alloc_slot_for_struct_reserves_full_struct_size() {
        let mut ctx = FunctionContext::new(&HashMap::new());
        let struct_ty = heap_block_ty();
        ctx.alloc_slot_for_alloc(&reg("block"), &struct_ty, None);
        ctx.finalize();
        assert!(
            ctx.frame_size() >= 32,
            "frame should be at least 32 bytes for a 32-byte struct Alloc, got {}",
            ctx.frame_size()
        );
    }

    #[test]
    fn alloc_slot_for_struct_does_not_overlap_next_slot() {
        let mut func = IrFunction::new("main", int32());
        let mut block = IrBlock::new("entry");

        block.push_instruction(IrInstruction::Alloc {
            dest: reg("block"),
            ty: heap_block_ty(),
            count: None,
        });
        block.push_instruction(IrInstruction::Alloc {
            dest: reg("ptr"),
            ty: i64_ty(),
            count: None,
        });
        block.set_terminator(IrTerminator::Return(None));
        func.push_block(block);

        let mut ctx = FunctionContext::new(&HashMap::new());
        assign_stack_slots(&func, &mut ctx, &HashMap::new(), false, false);

        let slot_block = ctx
            .slot_for_reg(&reg("block"))
            .expect("block must have a slot");
        let slot_ptr = ctx.slot_for_reg(&reg("ptr")).expect("ptr must have a slot");

        let block_end = slot_block + 32;
        assert!(
            slot_ptr >= block_end || slot_ptr + 8 <= slot_block,
            "struct slot [{slot_block}, {block_end}) overlaps pointer slot [{slot_ptr}, {}): \
             regression - Alloc struct was given only 8 bytes instead of 32",
            slot_ptr + 8
        );
    }
}
