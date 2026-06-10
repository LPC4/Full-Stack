//! Dead-code elimination.
//!
//! Removes pure instructions whose destination register is never read anywhere
//! in the function. Iterates to a fixpoint, since removing one instruction can
//! make its operands dead in turn.
//!
//! Conservative about side effects: `Load` is retained because a load may target
//! MMIO and have observable effects, and `Call`/`HeapAlloc`/`HeapFree`/`Store`/
//! `ReadReg`/`InlineAsm`/`Alloc` are always kept. Liveness is whole-function and
//! ignores which definition reaches a use, which is sound here because an
//! instruction is only removed when its register has zero readers at all.

use crate::ir::{IrFunction, IrInstruction, IrRegister, IrTerminator, IrValue};
use std::collections::HashSet;

/// Eliminate dead instructions in `func`. Returns true if anything changed.
pub fn run(func: &mut IrFunction) -> bool {
    let mut changed = false;
    loop {
        let used = collect_used_registers(func);
        let mut removed = false;
        for block in &mut func.blocks {
            let before = block.instructions.len();
            block
                .instructions
                .retain(|inst| keep_instruction(inst, &used));
            removed |= block.instructions.len() != before;
        }
        if removed {
            changed = true;
        } else {
            break;
        }
    }
    changed
}

/// Keep an instruction if it has side effects or its result is still read.
fn keep_instruction(inst: &IrInstruction, used: &HashSet<IrRegister>) -> bool {
    match inst {
        IrInstruction::Math { dest, .. }
        | IrInstruction::Unary { dest, .. }
        | IrInstruction::Cmp { dest, .. }
        | IrInstruction::Cast { dest, .. }
        | IrInstruction::Offset { dest, .. }
        | IrInstruction::Index { dest, .. }
        | IrInstruction::GlobalRef { dest, .. }
        | IrInstruction::Phi { dest, .. } => used.contains(dest),
        // Comment, Alloc, HeapAlloc, HeapFree, InlineAsm, ReadReg, Load, Store,
        // Call: retained for their side effects (or conservatively).
        _ => true,
    }
}

fn collect_used_registers(func: &IrFunction) -> HashSet<IrRegister> {
    let mut used = HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            collect_inst_uses(inst, &mut used);
        }
        if let Some(term) = &block.terminator {
            collect_term_uses(term, &mut used);
        }
    }
    used
}

fn note(used: &mut HashSet<IrRegister>, value: &IrValue) {
    if let IrValue::Register(r) = value {
        used.insert(r.clone());
    }
}

fn collect_inst_uses(inst: &IrInstruction, used: &mut HashSet<IrRegister>) {
    match inst {
        IrInstruction::Load { ptr, .. } => {
            used.insert(ptr.clone());
        }
        IrInstruction::Store { value, ptr, .. } => {
            note(used, value);
            used.insert(ptr.clone());
        }
        IrInstruction::Offset { ptr, bytes, .. } => {
            used.insert(ptr.clone());
            note(used, bytes);
        }
        IrInstruction::Index { base_ptr, idx, .. } => {
            used.insert(base_ptr.clone());
            note(used, idx);
        }
        IrInstruction::Math { lhs, rhs, .. } | IrInstruction::Cmp { lhs, rhs, .. } => {
            note(used, lhs);
            note(used, rhs);
        }
        IrInstruction::Unary { value, .. } | IrInstruction::Cast { value, .. } => {
            note(used, value);
        }
        IrInstruction::Call { args, .. } => {
            for arg in args {
                note(used, arg);
            }
        }
        IrInstruction::Phi { incoming, .. } => {
            for (value, _) in incoming {
                note(used, value);
            }
        }
        IrInstruction::HeapAlloc { count, .. } => {
            if let Some(count) = count {
                note(used, count);
            }
        }
        IrInstruction::HeapFree { ptr } => {
            used.insert(ptr.clone());
        }
        _ => {}
    }
}

fn collect_term_uses(term: &IrTerminator, used: &mut HashSet<IrRegister>) {
    match term {
        IrTerminator::Return(Some(value)) => note(used, value),
        IrTerminator::Branch { cond, .. } => note(used, cond),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IntWidth, IrBlock, IrMathOp, IrType};

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn i32_ty() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    fn math(dest: &str, lhs: IrValue, rhs: IrValue) -> IrInstruction {
        IrInstruction::Math {
            dest: reg(dest),
            op: IrMathOp::Add,
            ty: i32_ty(),
            lhs,
            rhs,
        }
    }

    fn function(instructions: Vec<IrInstruction>, term: IrTerminator) -> IrFunction {
        let mut func = IrFunction::new("f", i32_ty());
        let mut block = IrBlock::new("entry");
        for inst in instructions {
            block.push_instruction(inst);
        }
        block.set_terminator(term);
        func.push_block(block);
        func
    }

    #[test]
    fn removes_unused_pure_chain() {
        let mut func = function(
            vec![
                math("a", IrValue::Integer(1), IrValue::Integer(2)),
                math("b", IrValue::Register(reg("a")), IrValue::Integer(3)),
            ],
            IrTerminator::Return(Some(IrValue::Integer(0))),
        );
        assert!(run(&mut func));
        assert!(
            func.blocks[0].instructions.is_empty(),
            "both dead instructions should be removed"
        );
    }

    #[test]
    fn keeps_value_feeding_return() {
        let mut func = function(
            vec![math("a", IrValue::Integer(1), IrValue::Integer(2))],
            IrTerminator::Return(Some(IrValue::Register(reg("a")))),
        );
        run(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 1, "live value kept");
    }

    #[test]
    fn keeps_call_with_unused_result() {
        let mut func = function(
            vec![IrInstruction::Call {
                dest: Some(reg("a")),
                function: "side_effect".to_owned(),
                args: vec![],
            }],
            IrTerminator::Return(None),
        );
        run(&mut func);
        assert_eq!(
            func.blocks[0].instructions.len(),
            1,
            "calls must never be removed"
        );
    }

    #[test]
    fn keeps_unused_load() {
        // A load may target MMIO; it is retained even when its result is dead.
        let mut func = function(
            vec![IrInstruction::Load {
                dest: reg("a"),
                ty: i32_ty(),
                ptr: reg("p"),
                offset: None,
            }],
            IrTerminator::Return(None),
        );
        run(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 1, "loads are kept");
    }
}
