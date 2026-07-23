//! Phi simplification: a phi whose incoming values (minus self-references)
//! collapse to one value is deleted and its uses rewritten (spec 4.3).

use crate::ir::{IrFunction, IrInstruction, IrRegister, IrTerminator, IrValue};

/// Simplify trivial phis in `func`. Returns true if anything changed.
pub fn run(func: &mut IrFunction) -> bool {
    let mut changed = false;
    loop {
        let Some((dest, replacement)) = find_trivial_phi(func) else {
            break;
        };
        remove_phi(func, &dest);
        replace_uses(func, &dest, &replacement);
        changed = true;
    }
    changed
}

// The first phi whose incoming values (minus self-references) collapse to one
// value. Registers used as raw pointers can only be replaced by registers.
fn find_trivial_phi(func: &IrFunction) -> Option<(IrRegister, IrValue)> {
    for block in &func.blocks {
        for inst in &block.instructions {
            let IrInstruction::Phi { dest, incoming, .. } = inst else {
                continue;
            };
            let mut candidate: Option<&IrValue> = None;
            let mut trivial = true;
            for (value, _) in incoming {
                if matches!(value, IrValue::Register(r) if r == dest) {
                    continue;
                }
                match candidate {
                    None => candidate = Some(value),
                    Some(existing) if existing == value => {}
                    Some(_) => {
                        trivial = false;
                        break;
                    }
                }
            }
            let Some(replacement) = candidate else {
                continue;
            };
            if !trivial {
                continue;
            }
            if !matches!(replacement, IrValue::Register(_)) && used_as_pointer(func, dest) {
                continue;
            }
            return Some((dest.clone(), replacement.clone()));
        }
    }
    None
}

fn remove_phi(func: &mut IrFunction, dest: &IrRegister) {
    for block in &mut func.blocks {
        block
            .instructions
            .retain(|inst| !matches!(inst, IrInstruction::Phi { dest: d, .. } if d == dest));
    }
}

// True when `reg` appears in a raw `IrRegister` pointer operand, which cannot
// hold a constant replacement.
fn used_as_pointer(func: &IrFunction, reg: &IrRegister) -> bool {
    func.blocks.iter().any(|block| {
        block.instructions.iter().any(|inst| match inst {
            IrInstruction::Load { ptr, .. } | IrInstruction::HeapFree { ptr } => ptr == reg,
            IrInstruction::Store { ptr, .. } => ptr == reg,
            IrInstruction::Offset { ptr, .. } => ptr == reg,
            IrInstruction::Index { base_ptr, .. } => base_ptr == reg,
            _ => false,
        })
    })
}

fn replace_uses(func: &mut IrFunction, from: &IrRegister, to: &IrValue) {
    let sub = |value: &mut IrValue| {
        if matches!(value, IrValue::Register(r) if r == from) {
            *value = to.clone();
        }
    };
    let sub_reg = |reg: &mut IrRegister| {
        if reg == from
            && let IrValue::Register(new) = to
        {
            *reg = new.clone();
        }
    };
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            match inst {
                IrInstruction::Load { ptr, .. } | IrInstruction::HeapFree { ptr } => sub_reg(ptr),
                IrInstruction::Store { value, ptr, .. } => {
                    sub(value);
                    sub_reg(ptr);
                }
                IrInstruction::Offset { ptr, bytes, .. } => {
                    sub_reg(ptr);
                    sub(bytes);
                }
                IrInstruction::Index { base_ptr, idx, .. } => {
                    sub_reg(base_ptr);
                    sub(idx);
                }
                IrInstruction::Math { lhs, rhs, .. } | IrInstruction::Cmp { lhs, rhs, .. } => {
                    sub(lhs);
                    sub(rhs);
                }
                IrInstruction::Unary { value, .. } | IrInstruction::Cast { value, .. } => {
                    sub(value);
                }
                IrInstruction::Call { args, .. } => {
                    for arg in args {
                        sub(arg);
                    }
                }
                IrInstruction::IndirectCall { callee, args, .. } => {
                    sub(callee);
                    for arg in args {
                        sub(arg);
                    }
                }
                IrInstruction::Phi { incoming, .. } => {
                    for (value, _) in incoming {
                        sub(value);
                    }
                }
                IrInstruction::HeapAlloc { count, .. } => {
                    if let Some(count) = count {
                        sub(count);
                    }
                }
                _ => {}
            }
        }
        match &mut block.terminator {
            Some(IrTerminator::Return(Some(value))) => sub(value),
            Some(IrTerminator::Branch { cond, .. }) => sub(cond),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IntWidth, IrBlock, IrLabel, IrType};

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn i64_ty() -> IrType {
        IrType::Integer(IntWidth::I64)
    }

    fn phi(dest: &str, incoming: Vec<(IrValue, &str)>) -> IrInstruction {
        IrInstruction::Phi {
            dest: reg(dest),
            ty: i64_ty(),
            incoming: incoming
                .into_iter()
                .map(|(value, label)| (value, IrLabel::new(label)))
                .collect(),
        }
    }

    fn function_with(instructions: Vec<IrInstruction>, term: IrTerminator) -> IrFunction {
        let mut func = IrFunction::new("f", i64_ty());
        let mut block = IrBlock::new("merge");
        for inst in instructions {
            block.push_instruction(inst);
        }
        block.set_terminator(term);
        func.push_block(block);
        func
    }

    #[test]
    fn same_value_phi_is_replaced() {
        let mut func = function_with(
            vec![phi(
                "p",
                vec![(IrValue::Integer(7), "a"), (IrValue::Integer(7), "b")],
            )],
            IrTerminator::Return(Some(IrValue::Register(reg("p")))),
        );
        assert!(run(&mut func));
        assert!(func.blocks[0].instructions.is_empty());
        assert_eq!(
            func.blocks[0].terminator,
            Some(IrTerminator::Return(Some(IrValue::Integer(7))))
        );
    }

    #[test]
    fn self_referential_loop_phi_collapses_to_initial_value() {
        let mut func = function_with(
            vec![phi(
                "p",
                vec![
                    (IrValue::Register(reg("init")), "entry"),
                    (IrValue::Register(reg("p")), "body"),
                ],
            )],
            IrTerminator::Return(Some(IrValue::Register(reg("p")))),
        );
        assert!(run(&mut func));
        assert_eq!(
            func.blocks[0].terminator,
            Some(IrTerminator::Return(Some(IrValue::Register(reg("init")))))
        );
    }

    #[test]
    fn diverging_phi_is_kept() {
        let mut func = function_with(
            vec![phi(
                "p",
                vec![(IrValue::Integer(1), "a"), (IrValue::Integer(2), "b")],
            )],
            IrTerminator::Return(Some(IrValue::Register(reg("p")))),
        );
        assert!(!run(&mut func));
        assert_eq!(func.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn chained_trivial_phis_collapse_transitively() {
        let mut func = function_with(
            vec![
                phi("p", vec![(IrValue::Integer(3), "a")]),
                phi("q", vec![(IrValue::Register(reg("p")), "merge")]),
            ],
            IrTerminator::Return(Some(IrValue::Register(reg("q")))),
        );
        assert!(run(&mut func));
        assert!(func.blocks[0].instructions.is_empty());
        assert_eq!(
            func.blocks[0].terminator,
            Some(IrTerminator::Return(Some(IrValue::Integer(3))))
        );
    }

    #[test]
    fn constant_replacement_is_blocked_for_pointer_uses() {
        let mut instructions = vec![phi("p", vec![(IrValue::Integer(4), "a")])];
        instructions.push(IrInstruction::Load {
            dest: reg("v"),
            ty: i64_ty(),
            ptr: reg("p"),
            offset: None,
        });
        let mut func = function_with(
            instructions,
            IrTerminator::Return(Some(IrValue::Register(reg("v")))),
        );
        assert!(!run(&mut func), "constant cannot become a pointer operand");
    }
}
