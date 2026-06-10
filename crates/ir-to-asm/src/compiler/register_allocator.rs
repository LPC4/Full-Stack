//! Physical register allocation: scalar integer/pointer values with the highest
//! use counts are kept in callee-saved registers (s2-s11) instead of stack
//! slots. Interference comes from the same CFG liveness used by slot coloring;
//! values that do not fit stay slot-based. See the crate README.

use super::function_context::FunctionContext;
use super::slot_coloring;
use asm_to_binary::encode_decode::Reg;
use hll_to_ir::{IrFunction, IrInstruction, IrRegister, IrType};
use std::collections::{HashMap, HashSet};

const ALLOCATABLE: [Reg; 10] = [18, 19, 20, 21, 22, 23, 24, 25, 26, 27];

/// Assign physical registers to eligible virtual registers; leftovers go to slot-coloring.
pub fn allocate_registers(
    func: &IrFunction,
    ctx: &mut FunctionContext,
    vregs: &[(IrRegister, IrType)],
    needs_sret: bool,
) {
    // Inline asm may read or write any register, so a function containing it
    // keeps the pure stack-slot scheme.
    let has_inline_asm = func.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|inst| matches!(inst, IrInstruction::InlineAsm { .. }))
    });
    if has_inline_asm {
        return;
    }

    // Phi lowers to nothing in this backend, so any register touching one must
    // stay on the slot path (where the same no-op behavior is preserved).
    // TODO: phi
    let mut phi_regs: HashSet<IrRegister> = HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Phi { dest, incoming, .. } = inst {
                phi_regs.insert(dest.clone());
                for (value, _) in incoming {
                    if let hll_to_ir::IrValue::Register(r) = value {
                        phi_regs.insert(r.clone());
                    }
                }
            }
        }
    }

    let sret_param: Option<&IrRegister> = if needs_sret {
        func.params.first().map(|p| &p.register)
    } else {
        None
    };

    // Candidates: scalar integer/pointer values that are not address-taken.
    // Floats live in the FP file and aggregates in memory; both keep slots.
    let mut candidates: Vec<(IrRegister, IrType)> = Vec::new();
    let mut candidate_set: HashSet<IrRegister> = HashSet::new();
    for (reg, ty) in vregs {
        if ctx.slot_for_reg(reg).is_some() || ctx.is_stack_address(reg) {
            continue;
        }
        if phi_regs.contains(reg) || Some(reg) == sret_param {
            continue;
        }
        if !matches!(
            ctx.resolve_type(ty),
            IrType::Integer(_) | IrType::Pointer(_)
        ) {
            continue;
        }
        if candidate_set.insert(reg.clone()) {
            candidates.push((reg.clone(), ty.clone()));
        }
    }
    if candidates.is_empty() {
        return;
    }

    let interference = slot_coloring::build_interference(func, &candidate_set);
    let use_counts = count_uses(func);

    // Hottest values first; ties broken by first appearance for determinism.
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by_key(|&i| {
        let count = use_counts.get(&candidates[i].0).copied().unwrap_or(0);
        (std::cmp::Reverse(count), i)
    });

    let mut assigned: HashMap<IrRegister, Reg> = HashMap::new();
    for &i in &order {
        let (reg, ty) = &candidates[i];
        let mut taken: HashSet<Reg> = HashSet::new();
        if let Some(neighbors) = interference.get(reg) {
            for nb in neighbors {
                if let Some(&phys) = assigned.get(nb) {
                    taken.insert(phys);
                }
            }
        }
        if let Some(&phys) = ALLOCATABLE.iter().find(|r| !taken.contains(r)) {
            assigned.insert(reg.clone(), phys);
            ctx.assign_phys_reg(reg, phys);
            ctx.set_reg_type(reg, ty.clone());
            ctx.save_reg(phys);
        }
    }
}

// Static use+def occurrence count per register; the allocation priority.
fn count_uses(func: &IrFunction) -> HashMap<IrRegister, usize> {
    let mut counts: HashMap<IrRegister, usize> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            for r in slot_coloring::inst_uses(inst) {
                *counts.entry(r).or_insert(0) += 1;
            }
            for r in slot_coloring::inst_defs(inst) {
                *counts.entry(r).or_insert(0) += 1;
            }
        }
        if let Some(term) = &block.terminator {
            for r in slot_coloring::term_uses(term) {
                *counts.entry(r).or_insert(0) += 1;
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::stack_slots::assign_stack_slots;
    use hll_to_ir::{FloatWidth, IntWidth, IrBlock, IrMathOp, IrTerminator, IrValue};
    use std::collections::HashMap as StdHashMap;

    fn i64_ty() -> IrType {
        IrType::Integer(IntWidth::I64)
    }

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn lit_math(dest: &str, lhs: i64, rhs: i64) -> IrInstruction {
        IrInstruction::Math {
            dest: reg(dest),
            op: IrMathOp::Add,
            ty: i64_ty(),
            lhs: IrValue::Integer(lhs),
            rhs: IrValue::Integer(rhs),
        }
    }

    fn alloc_for(func: &IrFunction) -> FunctionContext {
        let mut ctx = FunctionContext::new(&StdHashMap::new());
        assign_stack_slots(func, &mut ctx, &StdHashMap::new(), true, false);
        ctx
    }

    #[test]
    fn scalar_values_get_callee_saved_registers() {
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(lit_math("a", 1, 2));
        block.push_instruction(lit_math("b", 3, 4));
        block.push_instruction(IrInstruction::Math {
            dest: reg("c"),
            op: IrMathOp::Add,
            ty: i64_ty(),
            lhs: IrValue::Register(reg("a")),
            rhs: IrValue::Register(reg("b")),
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("c")))));
        func.push_block(block);

        let ctx = alloc_for(&func);
        let pa = ctx.phys_reg_for(&reg("a")).expect("a allocated");
        let pb = ctx.phys_reg_for(&reg("b")).expect("b allocated");
        assert!(ALLOCATABLE.contains(&pa) && ALLOCATABLE.contains(&pb));
        assert_ne!(pa, pb, "simultaneously live values need distinct registers");
        assert!(
            ctx.slot_for_reg(&reg("a")).is_none(),
            "allocated registers should not also hold slots"
        );
        assert!(
            ctx.saved_regs().iter().any(|(r, _)| *r == pa),
            "allocated callee-saved registers must be saved in the prologue"
        );
    }

    #[test]
    fn inline_asm_function_is_not_allocated() {
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(lit_math("a", 1, 2));
        block.push_instruction(IrInstruction::InlineAsm {
            lines: vec!["nop".to_owned()],
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("a")))));
        func.push_block(block);

        let ctx = alloc_for(&func);
        assert!(
            ctx.phys_reg_for(&reg("a")).is_none(),
            "inline asm may clobber any register; the function must stay slot-based"
        );
        assert!(ctx.slot_for_reg(&reg("a")).is_some());
    }

    #[test]
    fn float_values_stay_in_slots() {
        let f64_ty = IrType::Float(FloatWidth::F64);
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(IrInstruction::Math {
            dest: reg("f"),
            op: IrMathOp::Add,
            ty: f64_ty,
            lhs: IrValue::Float(1.0),
            rhs: IrValue::Float(2.0),
        });
        block.push_instruction(lit_math("i", 1, 2));
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("i")))));
        func.push_block(block);

        let ctx = alloc_for(&func);
        assert!(
            ctx.phys_reg_for(&reg("f")).is_none(),
            "floats are not GP-allocated"
        );
        assert!(ctx.slot_for_reg(&reg("f")).is_some());
        assert!(ctx.phys_reg_for(&reg("i")).is_some());
    }

    #[test]
    fn pressure_overflow_falls_back_to_slots() {
        // Twelve simultaneously live values against ten allocatable registers:
        // two must stay slot-based.
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        let names: Vec<String> = (0..12).map(|i| format!("v{i}")).collect();
        for (i, name) in names.iter().enumerate() {
            block.push_instruction(lit_math(name, i as i64, 1));
        }
        // All values feed one call, so all are live at once.
        block.push_instruction(IrInstruction::Call {
            dest: None,
            function: "sink".to_owned(),
            args: names.iter().map(|n| IrValue::Register(reg(n))).collect(),
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
        func.push_block(block);

        let ctx = alloc_for(&func);
        let allocated = names
            .iter()
            .filter(|n| ctx.phys_reg_for(&reg(n)).is_some())
            .count();
        let slotted = names
            .iter()
            .filter(|n| ctx.slot_for_reg(&reg(n)).is_some())
            .count();
        assert_eq!(allocated, 10, "exactly the register file capacity is used");
        assert_eq!(slotted, 2, "overflow values must fall back to stack slots");
    }

    #[test]
    fn alloc_destinations_keep_dedicated_slots() {
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(IrInstruction::Alloc {
            dest: reg("p"),
            ty: i64_ty(),
            count: None,
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
        func.push_block(block);

        let ctx = alloc_for(&func);
        assert!(
            ctx.phys_reg_for(&reg("p")).is_none(),
            "address-taken storage must stay in memory"
        );
        assert!(ctx.slot_for_reg(&reg("p")).is_some());
    }
}
