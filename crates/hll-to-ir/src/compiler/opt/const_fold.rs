//! Local constant folding and propagation.
//!
//! Runs per block: it tracks which virtual registers hold a known integer
//! constant, substitutes those constants into later value operands, and folds
//! pure integer `Math`/`Unary`/`Cmp` instructions whose operands are all
//! constant. The folded instructions are left in place; dead-code elimination
//! removes them once their results are no longer read.
//!
//! Propagation is intentionally local (reset at every block boundary) so it is
//! sound without SSA: a register is only assumed constant after a constant
//! definition seen earlier in the same block, never across control flow.
//!
//! Folding mirrors the RV64 backend exactly. The backend materializes integer
//! operands with a full 64-bit `li`, computes in 64-bit registers, stores the
//! low bytes of the result type, and sign-extends on the next load. So a folded
//! value is the 64-bit op result sign-extended to the result width.

use crate::ir::{
    IntWidth, IrCmpOp, IrFunction, IrInstruction, IrMathOp, IrTerminator, IrType, IrUnaryOp,
    IrValue,
};
use std::collections::HashMap;

/// Fold constants throughout `func`. Returns true if anything changed.
pub fn run(func: &mut IrFunction) -> bool {
    let mut changed = false;
    for block in &mut func.blocks {
        let mut consts: HashMap<crate::ir::IrRegister, i64> = HashMap::new();
        for inst in &mut block.instructions {
            changed |= fold_instruction(inst, &mut consts);
        }
        if let Some(term) = &mut block.terminator {
            changed |= substitute_terminator(term, &consts);
        }
    }
    changed
}

fn fold_instruction(
    inst: &mut IrInstruction,
    consts: &mut HashMap<crate::ir::IrRegister, i64>,
) -> bool {
    let mut changed = false;

    // Propagate known constants into substitutable value operands.
    for operand in operand_uses_mut(inst) {
        if let IrValue::Register(r) = operand
            && let Some(&c) = consts.get(r)
        {
            *operand = IrValue::Integer(c);
            changed = true;
        }
    }

    // Fold pure ops with all-constant operands; otherwise invalidate the def.
    match inst {
        IrInstruction::Math {
            dest,
            op,
            ty,
            lhs,
            rhs,
        } => {
            if let (Some(w), Some(l), Some(r)) =
                (int_width(ty), as_const_int(lhs), as_const_int(rhs))
                && let Some(v) = fold_math(*op, l, r, w)
            {
                consts.insert(dest.clone(), v);
                return changed;
            }
            consts.remove(dest);
        }
        IrInstruction::Unary {
            dest,
            op,
            ty,
            value,
        } => {
            if let (Some(w), Some(v)) = (int_width(ty), as_const_int(value)) {
                consts.insert(dest.clone(), fold_unary(*op, v, w));
                return changed;
            }
            consts.remove(dest);
        }
        IrInstruction::Cmp {
            dest,
            op,
            ty,
            lhs,
            rhs,
        } => {
            if int_width(ty).is_some()
                && let (Some(l), Some(r)) = (as_const_int(lhs), as_const_int(rhs))
            {
                consts.insert(dest.clone(), fold_cmp(*op, l, r));
                return changed;
            }
            consts.remove(dest);
        }
        // Every other instruction that defines a register makes it unknown.
        IrInstruction::Load { dest, .. }
        | IrInstruction::Cast { dest, .. }
        | IrInstruction::Offset { dest, .. }
        | IrInstruction::Index { dest, .. }
        | IrInstruction::Phi { dest, .. }
        | IrInstruction::HeapAlloc { dest, .. }
        | IrInstruction::ReadReg { dest, .. }
        | IrInstruction::GlobalRef { dest, .. }
        | IrInstruction::Alloc { dest, .. } => {
            consts.remove(dest);
        }
        IrInstruction::Call {
            dest: Some(dest), ..
        } => {
            consts.remove(dest);
        }
        _ => {}
    }

    changed
}

/// Substitute a known-constant condition or return value in a terminator.
fn substitute_terminator(
    term: &mut IrTerminator,
    consts: &HashMap<crate::ir::IrRegister, i64>,
) -> bool {
    let operand = match term {
        IrTerminator::Return(Some(value)) => value,
        IrTerminator::Branch { cond, .. } => cond,
        _ => return false,
    };
    if let IrValue::Register(r) = operand
        && let Some(&c) = consts.get(r)
    {
        *operand = IrValue::Integer(c);
        return true;
    }
    false
}

/// Mutable references to value operands a register constant may flow into (Phi excluded).
fn operand_uses_mut(inst: &mut IrInstruction) -> Vec<&mut IrValue> {
    match inst {
        IrInstruction::Math { lhs, rhs, .. } | IrInstruction::Cmp { lhs, rhs, .. } => {
            vec![lhs, rhs]
        }
        IrInstruction::Unary { value, .. } | IrInstruction::Cast { value, .. } => vec![value],
        IrInstruction::Store { value, .. } => vec![value],
        IrInstruction::Offset { bytes, .. } => vec![bytes],
        IrInstruction::Index { idx, .. } => vec![idx],
        IrInstruction::Call { args, .. } => args.iter_mut().collect(),
        IrInstruction::HeapAlloc {
            count: Some(count), ..
        } => vec![count],
        _ => vec![],
    }
}

/// Read an operand as an integer constant. Bool and Null are integer-valued.
fn as_const_int(value: &IrValue) -> Option<i64> {
    match value {
        IrValue::Integer(i) => Some(*i),
        IrValue::Bool(b) => Some(i64::from(*b)),
        IrValue::Null => Some(0),
        _ => None,
    }
}

fn int_width(ty: &IrType) -> Option<IntWidth> {
    match ty {
        IrType::Integer(width) => Some(*width),
        _ => None,
    }
}

/// Sign-extend a 64-bit value from its result width, matching a backend store of
/// the low bytes followed by a sign-extending load.
fn sext(value: i64, width: IntWidth) -> i64 {
    match width {
        IntWidth::I1 => value & 1,
        IntWidth::I8 => i64::from(value as i8),
        IntWidth::I16 => i64::from(value as i16),
        IntWidth::I32 => i64::from(value as i32),
        IntWidth::I64 => value,
    }
}

/// Fold a binary integer op. Returns `None` for division/remainder by zero,
/// which is left to run time so trap/result behavior is not assumed.
fn fold_math(op: IrMathOp, lhs: i64, rhs: i64, width: IntWidth) -> Option<i64> {
    let raw = match op {
        IrMathOp::Add => lhs.wrapping_add(rhs),
        IrMathOp::Sub => lhs.wrapping_sub(rhs),
        IrMathOp::Mul => lhs.wrapping_mul(rhs),
        IrMathOp::And => lhs & rhs,
        IrMathOp::Or => lhs | rhs,
        IrMathOp::Xor => lhs ^ rhs,
        // RV64 sll/srl use the low 6 bits of the shift amount; srl is logical.
        IrMathOp::Shl => lhs.wrapping_shl((rhs & 0x3f) as u32),
        IrMathOp::Shr => ((lhs as u64) >> (rhs & 0x3f)) as i64,
        IrMathOp::Div | IrMathOp::SDiv => {
            if rhs == 0 {
                return None;
            }
            lhs.wrapping_div(rhs)
        }
        IrMathOp::Mod => {
            if rhs == 0 {
                return None;
            }
            lhs.wrapping_rem(rhs)
        }
        IrMathOp::UDiv => {
            if rhs == 0 {
                return None;
            }
            (lhs as u64).wrapping_div(rhs as u64) as i64
        }
        IrMathOp::UMod => {
            if rhs == 0 {
                return None;
            }
            (lhs as u64).wrapping_rem(rhs as u64) as i64
        }
    };
    Some(sext(raw, width))
}

fn fold_unary(op: IrUnaryOp, value: i64, width: IntWidth) -> i64 {
    let raw = match op {
        IrUnaryOp::Neg => 0i64.wrapping_sub(value),
        IrUnaryOp::Not => !value,
    };
    sext(raw, width)
}

/// Fold an integer comparison to 0 or 1. Operands are the exact 64-bit values
/// the backend would `li`, so unsigned ops compare the raw bit patterns.
fn fold_cmp(op: IrCmpOp, lhs: i64, rhs: i64) -> i64 {
    let result = match op {
        IrCmpOp::Eq => lhs == rhs,
        IrCmpOp::Ne => lhs != rhs,
        IrCmpOp::Slt => lhs < rhs,
        IrCmpOp::Sle => lhs <= rhs,
        IrCmpOp::Sgt => lhs > rhs,
        IrCmpOp::Sge => lhs >= rhs,
        IrCmpOp::Ult => (lhs as u64) < (rhs as u64),
        IrCmpOp::Ule => (lhs as u64) <= (rhs as u64),
        IrCmpOp::Ugt => (lhs as u64) > (rhs as u64),
        IrCmpOp::Uge => (lhs as u64) >= (rhs as u64),
    };
    i64::from(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrBlock, IrLabel, IrRegister};

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn i32_ty() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    fn math(dest: &str, op: IrMathOp, lhs: IrValue, rhs: IrValue) -> IrInstruction {
        IrInstruction::Math {
            dest: reg(dest),
            op,
            ty: i32_ty(),
            lhs,
            rhs,
        }
    }

    fn run_block(instructions: Vec<IrInstruction>) -> IrBlock {
        let mut func = IrFunction::new("f", i32_ty());
        let mut block = IrBlock::new("entry");
        for inst in instructions {
            block.push_instruction(inst);
        }
        func.push_block(block);
        run(&mut func);
        func.blocks.into_iter().next().unwrap()
    }

    #[test]
    fn folds_constant_chain_and_propagates() {
        // $a = 2 + 3; $b = $a * 4; the store should see 20.
        let block = run_block(vec![
            math("a", IrMathOp::Add, IrValue::Integer(2), IrValue::Integer(3)),
            math(
                "b",
                IrMathOp::Mul,
                IrValue::Register(reg("a")),
                IrValue::Integer(4),
            ),
            IrInstruction::Store {
                ty: i32_ty(),
                value: IrValue::Register(reg("b")),
                ptr: reg("p"),
                offset: None,
            },
        ]);
        // The store operand is now the folded constant.
        match &block.instructions[2] {
            IrInstruction::Store { value, .. } => {
                assert_eq!(*value, IrValue::Integer(20));
            }
            other => panic!("expected store, got {other:?}"),
        }
    }

    #[test]
    fn sign_extends_to_result_width() {
        // 0x7fffffff + 1 overflows i32 to a negative value.
        let block = run_block(vec![
            math(
                "x",
                IrMathOp::Add,
                IrValue::Integer(0x7fff_ffff),
                IrValue::Integer(1),
            ),
            IrInstruction::Store {
                ty: i32_ty(),
                value: IrValue::Register(reg("x")),
                ptr: reg("p"),
                offset: None,
            },
        ]);
        match &block.instructions[1] {
            IrInstruction::Store { value, .. } => {
                assert_eq!(*value, IrValue::Integer(-2_147_483_648));
            }
            other => panic!("expected store, got {other:?}"),
        }
    }

    #[test]
    fn does_not_fold_division_by_zero() {
        let block = run_block(vec![math(
            "x",
            IrMathOp::SDiv,
            IrValue::Integer(10),
            IrValue::Integer(0),
        )]);
        // Operands unchanged, nothing recorded as constant.
        match &block.instructions[0] {
            IrInstruction::Math { lhs, rhs, .. } => {
                assert_eq!(*lhs, IrValue::Integer(10));
                assert_eq!(*rhs, IrValue::Integer(0));
            }
            other => panic!("expected math, got {other:?}"),
        }
    }

    #[test]
    fn folds_comparison_to_bool() {
        let block = run_block(vec![
            IrInstruction::Cmp {
                dest: reg("c"),
                op: IrCmpOp::Slt,
                ty: i32_ty(),
                lhs: IrValue::Integer(3),
                rhs: IrValue::Integer(7),
            },
            IrInstruction::Store {
                ty: IrType::Integer(IntWidth::I1),
                value: IrValue::Register(reg("c")),
                ptr: reg("p"),
                offset: None,
            },
        ]);
        match &block.instructions[1] {
            IrInstruction::Store { value, .. } => assert_eq!(*value, IrValue::Integer(1)),
            other => panic!("expected store, got {other:?}"),
        }
    }

    #[test]
    fn does_not_propagate_across_blocks() {
        // A constant defined in one block must not leak into another.
        let mut func = IrFunction::new("f", i32_ty());
        let mut entry = IrBlock::new("entry");
        entry.push_instruction(math(
            "a",
            IrMathOp::Add,
            IrValue::Integer(1),
            IrValue::Integer(1),
        ));
        entry.set_terminator(IrTerminator::Jump(IrLabel::new("next")));
        func.push_block(entry);

        let mut next = IrBlock::new("next");
        next.push_instruction(IrInstruction::Store {
            ty: i32_ty(),
            value: IrValue::Register(reg("a")),
            ptr: reg("p"),
            offset: None,
        });
        func.push_block(next);

        run(&mut func);
        match &func.blocks[1].instructions[0] {
            IrInstruction::Store { value, .. } => {
                assert_eq!(*value, IrValue::Register(reg("a")), "must stay a register");
            }
            other => panic!("expected store, got {other:?}"),
        }
    }

    #[test]
    fn reassignment_updates_constant() {
        // $x = 5; $x = $x + 1; a later use sees 6.
        let block = run_block(vec![
            math("x", IrMathOp::Add, IrValue::Integer(2), IrValue::Integer(3)),
            math(
                "x",
                IrMathOp::Add,
                IrValue::Register(reg("x")),
                IrValue::Integer(1),
            ),
            IrInstruction::Store {
                ty: i32_ty(),
                value: IrValue::Register(reg("x")),
                ptr: reg("p"),
                offset: None,
            },
        ]);
        match &block.instructions[2] {
            IrInstruction::Store { value, .. } => assert_eq!(*value, IrValue::Integer(6)),
            other => panic!("expected store, got {other:?}"),
        }
    }
}
