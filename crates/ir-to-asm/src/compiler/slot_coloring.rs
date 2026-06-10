//! Stack-slot coloring: scalar registers with non-overlapping live ranges share
//! a slot. See the crate README for the rationale and algorithm.

use super::function_context::FunctionContext;
use hll_to_ir::{IrFunction, IrInstruction, IrRegister, IrTerminator, IrType, IrValue};
use std::collections::{HashMap, HashSet};

/// Assign stack slots to every value register that still needs one (Alloc destinations excluded).
pub fn assign_colored_slots(
    func: &IrFunction,
    ctx: &mut FunctionContext,
    vregs: &[(IrRegister, IrType)],
) {
    let mut colorable: Vec<IrRegister> = Vec::new();
    let mut colorable_set: HashSet<IrRegister> = HashSet::new();

    for (reg, ty) in vregs {
        if ctx.slot_for_reg(reg).is_some() || ctx.phys_reg_for(reg).is_some() {
            continue;
        }
        if !ctx.is_stack_address(reg) && is_colorable_type(&ctx.resolve_type(ty)) {
            if colorable_set.insert(reg.clone()) {
                colorable.push(reg.clone());
                ctx.set_reg_type(reg, ty.clone());
            }
        } else {
            // Aggregate, array, or address-taken values need a dedicated slot.
            ctx.alloc_slot_for_reg(reg, ty);
        }
    }

    if colorable.is_empty() {
        return;
    }

    let interference = build_interference(func, &colorable_set);
    let colors = greedy_color(&colorable, &interference);

    // Reserve one 8-byte slot per color, then map each register onto it.
    let num_colors = colors.values().map(|c| c + 1).max().unwrap_or(0);
    let mut color_offset: Vec<usize> = Vec::with_capacity(num_colors);
    for _ in 0..num_colors {
        color_offset.push(ctx.frame.alloc_slot(8, 8));
    }

    for reg in &colorable {
        let color = colors[reg];
        ctx.set_reg_slot(reg, color_offset[color]);
    }
}

/// A scalar value type fits in one 8-byte slot.
fn is_colorable_type(ty: &IrType) -> bool {
    matches!(
        ty,
        IrType::Integer(_) | IrType::Pointer(_) | IrType::Float(_)
    )
}

/// Build the interference graph among colorable registers via live-variable analysis.
pub(super) fn build_interference(
    func: &IrFunction,
    colorable: &HashSet<IrRegister>,
) -> HashMap<IrRegister, HashSet<IrRegister>> {
    let n = func.blocks.len();

    // Successor edges per block.
    let mut label_to_idx: HashMap<&str, usize> = HashMap::new();
    for (i, block) in func.blocks.iter().enumerate() {
        label_to_idx.insert(block.label.0.as_str(), i);
    }
    let succ: Vec<Vec<usize>> = func
        .blocks
        .iter()
        .map(|block| match &block.terminator {
            Some(IrTerminator::Jump(label)) => label_to_idx
                .get(label.0.as_str())
                .copied()
                .into_iter()
                .collect(),
            Some(IrTerminator::Branch {
                then_label,
                else_label,
                ..
            }) => {
                let mut v = Vec::new();
                if let Some(&t) = label_to_idx.get(then_label.0.as_str()) {
                    v.push(t);
                }
                if let Some(&e) = label_to_idx.get(else_label.0.as_str()) {
                    v.push(e);
                }
                v
            }
            _ => Vec::new(),
        })
        .collect();

    // Per-block use (read before any write) and def (written), colorable only.
    let mut block_use: Vec<HashSet<IrRegister>> = vec![HashSet::new(); n];
    let mut block_def: Vec<HashSet<IrRegister>> = vec![HashSet::new(); n];
    for (b, block) in func.blocks.iter().enumerate() {
        let mut used = HashSet::new();
        let mut defined = HashSet::new();
        for inst in &block.instructions {
            for r in inst_uses(inst) {
                if colorable.contains(&r) && !defined.contains(&r) {
                    used.insert(r);
                }
            }
            for r in inst_defs(inst) {
                if colorable.contains(&r) {
                    defined.insert(r);
                }
            }
        }
        if let Some(term) = &block.terminator {
            for r in term_uses(term) {
                if colorable.contains(&r) && !defined.contains(&r) {
                    used.insert(r);
                }
            }
        }
        block_use[b] = used;
        block_def[b] = defined;
    }

    // Backward dataflow to a fixpoint: live_in = use + (live_out - def).
    let mut live_in: Vec<HashSet<IrRegister>> = vec![HashSet::new(); n];
    let mut live_out: Vec<HashSet<IrRegister>> = vec![HashSet::new(); n];
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..n).rev() {
            let mut out = HashSet::new();
            for &s in &succ[b] {
                for r in &live_in[s] {
                    out.insert(r.clone());
                }
            }
            let mut in_set = block_use[b].clone();
            for r in &out {
                if !block_def[b].contains(r) {
                    in_set.insert(r.clone());
                }
            }
            if out != live_out[b] {
                live_out[b] = out;
                changed = true;
            }
            if in_set != live_in[b] {
                live_in[b] = in_set;
                changed = true;
            }
        }
    }

    // Scan each block backward over a running live set, adding an edge for every
    // def against everything else live at that point.
    let mut graph: HashMap<IrRegister, HashSet<IrRegister>> = HashMap::new();
    for reg in colorable {
        graph.entry(reg.clone()).or_default();
    }

    for (b, block) in func.blocks.iter().enumerate() {
        let mut live = live_out[b].clone();
        if let Some(term) = &block.terminator {
            for r in term_uses(term) {
                if colorable.contains(&r) {
                    live.insert(r);
                }
            }
        }
        for inst in block.instructions.iter().rev() {
            for d in inst_defs(inst) {
                if !colorable.contains(&d) {
                    continue;
                }
                for v in &live {
                    if *v != d {
                        add_edge(&mut graph, &d, v);
                    }
                }
                live.remove(&d);
            }
            for u in inst_uses(inst) {
                if colorable.contains(&u) {
                    live.insert(u);
                }
            }
        }

        // Parameters are all live at entry, so they mutually interfere and
        // interfere with anything else live there.
        if b == 0 {
            let params: Vec<IrRegister> = func
                .params
                .iter()
                .map(|p| p.register.clone())
                .filter(|r| colorable.contains(r))
                .collect();
            for p in &params {
                for v in &live {
                    if v != p {
                        add_edge(&mut graph, p, v);
                    }
                }
            }
            for i in 0..params.len() {
                for j in (i + 1)..params.len() {
                    add_edge(&mut graph, &params[i], &params[j]);
                }
            }
        }
    }

    graph
}

fn add_edge(graph: &mut HashMap<IrRegister, HashSet<IrRegister>>, a: &IrRegister, b: &IrRegister) {
    graph.entry(a.clone()).or_default().insert(b.clone());
    graph.entry(b.clone()).or_default().insert(a.clone());
}

/// Greedy graph coloring in IR appearance order; each register gets the lowest unused color.
fn greedy_color(
    order: &[IrRegister],
    graph: &HashMap<IrRegister, HashSet<IrRegister>>,
) -> HashMap<IrRegister, usize> {
    let mut colors: HashMap<IrRegister, usize> = HashMap::new();
    for reg in order {
        let mut used: HashSet<usize> = HashSet::new();
        if let Some(neighbors) = graph.get(reg) {
            for nb in neighbors {
                if let Some(&c) = colors.get(nb) {
                    used.insert(c);
                }
            }
        }
        let mut color = 0;
        while used.contains(&color) {
            color += 1;
        }
        colors.insert(reg.clone(), color);
    }
    colors
}

// --- Use/def extraction ---

fn value_reg(value: &IrValue) -> Option<IrRegister> {
    if let IrValue::Register(reg) = value {
        Some(reg.clone())
    } else {
        None
    }
}

pub(super) fn inst_uses(inst: &IrInstruction) -> Vec<IrRegister> {
    use IrInstruction::{
        Call, Cast, Cmp, HeapAlloc, HeapFree, Index, Load, Math, Offset, Phi, Store, Unary,
    };
    let mut uses = Vec::new();
    match inst {
        Load { ptr, .. } => uses.push(ptr.clone()),
        Store { value, ptr, .. } => {
            uses.extend(value_reg(value));
            uses.push(ptr.clone());
        }
        Offset { ptr, bytes, .. } => {
            uses.push(ptr.clone());
            uses.extend(value_reg(bytes));
        }
        Index { base_ptr, idx, .. } => {
            uses.push(base_ptr.clone());
            uses.extend(value_reg(idx));
        }
        Math { lhs, rhs, .. } | Cmp { lhs, rhs, .. } => {
            uses.extend(value_reg(lhs));
            uses.extend(value_reg(rhs));
        }
        Unary { value, .. } | Cast { value, .. } => uses.extend(value_reg(value)),
        Call { args, .. } => {
            for arg in args {
                uses.extend(value_reg(arg));
            }
        }
        Phi { incoming, .. } => {
            for (value, _) in incoming {
                uses.extend(value_reg(value));
            }
        }
        HeapAlloc { count, .. } => {
            if let Some(value) = count {
                uses.extend(value_reg(value));
            }
        }
        HeapFree { ptr } => uses.push(ptr.clone()),
        _ => {}
    }
    uses
}

pub(super) fn inst_defs(inst: &IrInstruction) -> Vec<IrRegister> {
    use IrInstruction::{
        Alloc, Call, Cast, Cmp, GlobalRef, HeapAlloc, Index, Load, Math, Offset, Phi, ReadReg,
        Unary,
    };
    match inst {
        Alloc { dest, .. }
        | Load { dest, .. }
        | Math { dest, .. }
        | Unary { dest, .. }
        | Cmp { dest, .. }
        | Cast { dest, .. }
        | Phi { dest, .. }
        | Offset { dest, .. }
        | Index { dest, .. }
        | HeapAlloc { dest, .. }
        | ReadReg { dest, .. }
        | GlobalRef { dest, .. } => vec![dest.clone()],
        Call { dest, .. } => dest.clone().into_iter().collect(),
        _ => Vec::new(),
    }
}

pub(super) fn term_uses(term: &IrTerminator) -> Vec<IrRegister> {
    match term {
        IrTerminator::Return(Some(value)) => value_reg(value).into_iter().collect(),
        IrTerminator::Branch { cond, .. } => value_reg(cond).into_iter().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::function_context::FunctionContext;
    use crate::compiler::stack_slots::assign_stack_slots;
    use hll_to_ir::{IntWidth, IrBlock, IrMathOp, IrTerminator, IrValue};
    use std::collections::HashMap;

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

    fn slots_for(func: &IrFunction) -> (FunctionContext, HashMap<IrRegister, usize>) {
        let mut ctx = FunctionContext::new(&HashMap::new());
        assign_stack_slots(func, &mut ctx, &HashMap::new(), false, false);
        let mut map = HashMap::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                for d in inst_defs(inst) {
                    if let Some(slot) = ctx.slot_for_reg(&d) {
                        map.insert(d, slot);
                    }
                }
            }
        }
        (ctx, map)
    }

    #[test]
    fn disjoint_scalars_share_one_slot() {
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(lit_math("a", 1, 2));
        block.push_instruction(IrInstruction::Call {
            dest: None,
            function: "sink".to_owned(),
            args: vec![IrValue::Register(reg("a"))],
        });
        block.push_instruction(lit_math("b", 3, 4));
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("b")))));
        func.push_block(block);

        let (_, slots) = slots_for(&func);
        assert_eq!(
            slots[&reg("a")],
            slots[&reg("b")],
            "non-overlapping values should share a slot"
        );
    }

    #[test]
    fn simultaneously_live_scalars_get_distinct_slots() {
        // Both values feed the same call, so they are live at once.
        let mut func = IrFunction::new("main", i64_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(lit_math("a", 1, 2));
        block.push_instruction(lit_math("b", 3, 4));
        block.push_instruction(IrInstruction::Call {
            dest: None,
            function: "sink".to_owned(),
            args: vec![IrValue::Register(reg("a")), IrValue::Register(reg("b"))],
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("a")))));
        func.push_block(block);

        let (_, slots) = slots_for(&func);
        assert_ne!(
            slots[&reg("a")],
            slots[&reg("b")],
            "overlapping values must not share a slot"
        );
    }

    #[test]
    fn loop_carried_value_does_not_share_with_body_temp() {
        // `acc` is live across the back-edge and overlaps `t`, so loop-aware
        // liveness must keep them in distinct slots.
        let mut func = IrFunction::new("main", i64_ty());

        let mut entry = IrBlock::new("entry");
        entry.push_instruction(lit_math("acc", 0, 0));
        entry.set_terminator(IrTerminator::Jump(hll_to_ir::IrLabel::new("body")));
        func.push_block(entry);

        let mut body = IrBlock::new("body");
        // t = acc + 1; acc = acc + t.
        body.push_instruction(IrInstruction::Math {
            dest: reg("t"),
            op: IrMathOp::Add,
            ty: i64_ty(),
            lhs: IrValue::Register(reg("acc")),
            rhs: IrValue::Integer(1),
        });
        body.push_instruction(IrInstruction::Math {
            dest: reg("acc"),
            op: IrMathOp::Add,
            ty: i64_ty(),
            lhs: IrValue::Register(reg("acc")),
            rhs: IrValue::Register(reg("t")),
        });
        body.set_terminator(IrTerminator::Branch {
            cond: IrValue::Register(reg("acc")),
            then_label: hll_to_ir::IrLabel::new("body"),
            else_label: hll_to_ir::IrLabel::new("done"),
        });
        func.push_block(body);

        let mut done = IrBlock::new("done");
        done.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("acc")))));
        func.push_block(done);

        let (_, slots) = slots_for(&func);
        assert_ne!(
            slots[&reg("acc")],
            slots[&reg("t")],
            "loop-carried value must not share a slot with an overlapping body temp"
        );
    }
}
