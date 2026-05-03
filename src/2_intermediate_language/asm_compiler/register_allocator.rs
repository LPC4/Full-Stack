//! Simple register allocator that maps every virtual register to a stack slot.
//! No register reuse - all values are stored in memory.

use super::function_context::FunctionContext;
use crate::intermediate_language::{IrFunction, IrInstruction, IrTerminator, IrType, IrValue};
use std::collections::HashMap;

pub struct RegisterAllocator;

impl RegisterAllocator {
    pub fn new() -> Self {
        Self
    }

    /// First pass: allocate stack slots for all virtual registers in the function.
    pub fn allocate_slots(
        &mut self,
        func: &IrFunction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) {
        for param in &func.params {
            ctx.alloc_slot_for_reg(&param.register, &param.ty);
        }

        for block in &func.blocks {
            for inst in &block.instructions {
                self.visit_instruction(inst, ctx, function_return_types);
            }
            if let Some(term) = &block.terminator {
                self.visit_terminator(term, ctx);
            }
        }
    }

    fn visit_instruction(
        &self,
        inst: &IrInstruction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) {
        use IrInstruction::{
            Alloc, Call, Cast, Cmp, Comment, HeapAlloc, HeapFree, Index, Load, Math, Offset, Phi,
            Store, Unary,
        };
        match inst {
            Alloc { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.mark_stack_address(dest);
                ctx.set_reg_type(dest, IrType::Pointer(Box::new(ty.clone())));
            }
            Load { dest, ty, ptr, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.set_reg_type(dest, ty.clone());
                if ctx.slot_for_reg(ptr).is_none() {
                    ctx.alloc_slot_for_reg(ptr, &IrType::Pointer(Box::new(IrType::Void)));
                }
            }
            Store {
                ty: _, value, ptr, ..
            } => {
                if let IrValue::Register(reg) = value {
                    if ctx.slot_for_reg(reg).is_none() {
                        ctx.alloc_slot_for_reg(
                            reg,
                            &IrType::Integer(crate::intermediate_language::IntWidth::I32),
                        );
                    }
                }
                if ctx.slot_for_reg(ptr).is_none() {
                    ctx.alloc_slot_for_reg(ptr, &IrType::Pointer(Box::new(IrType::Void)));
                }
            }
            Offset { dest, ty, ptr, .. } => {
                ctx.alloc_slot_for_reg(dest, &IrType::Pointer(Box::new(ty.clone())));
                ctx.set_reg_type(dest, IrType::Pointer(Box::new(ty.clone())));
                if ctx.slot_for_reg(ptr).is_none() {
                    ctx.alloc_slot_for_reg(ptr, &IrType::Pointer(Box::new(IrType::Void)));
                }
            }
            Index {
                dest, ty, base_ptr, ..
            } => {
                ctx.alloc_slot_for_reg(dest, &IrType::Pointer(Box::new(ty.clone())));
                ctx.set_reg_type(dest, IrType::Pointer(Box::new(ty.clone())));
                if ctx.slot_for_reg(base_ptr).is_none() {
                    ctx.alloc_slot_for_reg(base_ptr, &IrType::Pointer(Box::new(IrType::Void)));
                }
            }
            Math { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.set_reg_type(dest, ty.clone());
            }
            Unary { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.set_reg_type(dest, ty.clone());
            }
            Cmp { dest, .. } => {
                ctx.alloc_slot_for_reg(
                    dest,
                    &IrType::Integer(crate::intermediate_language::IntWidth::I1),
                );
                ctx.set_reg_type(
                    dest,
                    IrType::Integer(crate::intermediate_language::IntWidth::I1),
                );
            }
            Cast { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.set_reg_type(dest, ty.clone());
            }
            Call { dest, function, .. } => {
                if let Some(dest) = dest {
                    let ret_ty = function_return_types
                        .get(function)
                        .cloned()
                        .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64));
                    ctx.alloc_slot_for_reg(dest, &ret_ty);
                    ctx.set_reg_type(dest, ret_ty.clone());
                    // If the return type is an aggregate, the slot IS the data -- mark as stack address
                    if matches!(ret_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                        ctx.mark_stack_address(dest);
                    }
                }
            }
            Phi { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, ty);
                ctx.set_reg_type(dest, ty.clone());
            }
            HeapAlloc { dest, ty, .. } => {
                ctx.alloc_slot_for_reg(dest, &IrType::Pointer(Box::new(ty.clone())));
                ctx.set_reg_type(dest, IrType::Pointer(Box::new(ty.clone())));
            }
            HeapFree { ptr } => {
                if ctx.slot_for_reg(ptr).is_none() {
                    ctx.alloc_slot_for_reg(ptr, &IrType::Pointer(Box::new(IrType::Void)));
                }
            }
            Comment(_) => {}
        }
    }

    fn visit_terminator(&self, term: &IrTerminator, ctx: &mut FunctionContext) {
        use IrTerminator::{Branch, Return};
        match term {
            Return(Some(val)) => {
                if let IrValue::Register(reg) = val {
                    if ctx.slot_for_reg(reg).is_none() {
                        ctx.alloc_slot_for_reg(
                            reg,
                            &IrType::Integer(crate::intermediate_language::IntWidth::I32),
                        );
                        ctx.set_reg_type(
                            reg,
                            IrType::Integer(crate::intermediate_language::IntWidth::I32),
                        );
                    }
                }
            }
            Branch { cond, .. } => {
                if let IrValue::Register(reg) = cond {
                    if ctx.slot_for_reg(reg).is_none() {
                        ctx.alloc_slot_for_reg(
                            reg,
                            &IrType::Integer(crate::intermediate_language::IntWidth::I1),
                        );
                        ctx.set_reg_type(
                            reg,
                            IrType::Integer(crate::intermediate_language::IntWidth::I1),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

impl Default for RegisterAllocator {
    fn default() -> Self {
        Self::new()
    }
}
