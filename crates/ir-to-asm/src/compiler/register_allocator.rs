//! Linear-scan register allocator: assigns physical temporaries when possible,
//! spills to stack slots otherwise. See the crate README.

use super::function_context::FunctionContext;
use asm_to_binary::encode_decode::Reg;
use hll_to_ir::{IrFunction, IrInstruction, IrTerminator, IrType, IrValue};
use std::collections::HashMap;

/// Caller-saved temporaries available for allocation: t0-t2, t3-t6.
const AVAILABLE_REGS: [Reg; 7] = [5, 6, 7, 28, 29, 30, 31];

#[derive(Debug, Clone)]
struct LiveInterval {
    reg: hll_to_ir::IrRegister,
    start: usize,
    end: usize,
    ty: IrType,
}

pub struct RegisterAllocator {
    /// Maps virtual registers to their assigned physical register or stack slot.
    reg_mapping: HashMap<hll_to_ir::IrRegister, Allocation>,
}

#[derive(Debug, Clone)]
pub enum Allocation {
    Physical(Reg),
    StackSlot(usize),
}

impl RegisterAllocator {
    pub fn new() -> Self {
        Self {
            reg_mapping: HashMap::new(),
        }
    }

    /// Allocate registers for all virtual registers in the function.
    pub fn allocate_slots(
        &mut self,
        func: &IrFunction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) {
        let intervals = self.prepare_intervals_and_stack_slots(func, ctx, function_return_types);
        self.linear_scan_allocate(&intervals, ctx, func);
    }

    /// Prepare stack slots for every live virtual register without assigning
    /// physical registers. This is the path the main code generator uses.
    pub fn allocate_stack_slots(
        &mut self,
        func: &IrFunction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) {
        let _ = self.prepare_intervals_and_stack_slots(func, ctx, function_return_types);
    }

    fn prepare_intervals_and_stack_slots(
        &mut self,
        func: &IrFunction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) -> Vec<LiveInterval> {
        self.reg_mapping.clear();

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
                self.collect_vregs_from_instruction(inst, &mut vregs, function_return_types);
            }
            if let Some(term) = &block.terminator {
                self.collect_vregs_from_terminator(term, &mut vregs);
            }
        }

        // Live intervals feed the physical linear-scan path.
        let intervals = self.compute_live_intervals(func, &vregs);

        // Slot coloring gives every register a slot (sharing where live ranges
        // allow), which the spill path relies on.
        super::slot_coloring::assign_colored_slots(func, ctx, &vregs);

        intervals
    }

    fn collect_vregs_from_instruction(
        &self,
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

    fn collect_vregs_from_terminator(
        &self,
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

    fn compute_live_intervals(
        &self,
        func: &IrFunction,
        vregs: &[(hll_to_ir::IrRegister, IrType)],
    ) -> Vec<LiveInterval> {
        let mut intervals = HashMap::new();
        let mut pos = 0;

        for block in &func.blocks {
            for inst in &block.instructions {
                self.record_uses(inst, pos, &mut intervals);
                self.record_defs(inst, pos, &mut intervals);
                pos += 1;
            }

            if let Some(term) = &block.terminator {
                self.record_terminator_uses(term, pos, &mut intervals);
                pos += 1;
            }
        }

        let mut result = Vec::new();
        for (reg, ty) in vregs {
            if let Some((start, end)) = intervals.get(reg) {
                result.push(LiveInterval {
                    reg: reg.clone(),
                    start: *start,
                    end: *end,
                    ty: ty.clone(),
                });
            }
        }

        // Sort by (start, end) for deterministic allocation.
        result.sort_by_key(|i| (i.start, i.end));
        result
    }

    fn record_uses(
        &self,
        inst: &IrInstruction,
        pos: usize,
        intervals: &mut HashMap<hll_to_ir::IrRegister, (usize, usize)>,
    ) {
        use IrInstruction::{
            Call, Cast, Cmp, HeapAlloc, HeapFree, Index, Load, Math, Offset, Phi, Store, Unary,
        };

        let mut update_interval = |reg: &hll_to_ir::IrRegister| {
            let entry = intervals.entry(reg.clone()).or_insert((pos, pos));
            entry.1 = entry.1.max(pos);
        };

        match inst {
            Load { ptr, .. } => {
                update_interval(ptr);
            }
            Store { value, ptr, .. } => {
                if let IrValue::Register(reg) = value {
                    update_interval(reg);
                }
                update_interval(ptr);
            }
            Offset { ptr, bytes, .. } => {
                update_interval(ptr);
                if let IrValue::Register(reg) = bytes {
                    update_interval(reg);
                }
            }
            Index { base_ptr, idx, .. } => {
                update_interval(base_ptr);
                if let IrValue::Register(reg) = idx {
                    update_interval(reg);
                }
            }
            Math { lhs, rhs, .. } | Cmp { lhs, rhs, .. } => {
                if let IrValue::Register(reg) = lhs {
                    update_interval(reg);
                }
                if let IrValue::Register(reg) = rhs {
                    update_interval(reg);
                }
            }
            Unary { value, .. } => {
                if let IrValue::Register(reg) = value {
                    update_interval(reg);
                }
            }
            Cast { value, .. } => {
                if let IrValue::Register(reg) = value {
                    update_interval(reg);
                }
            }
            Call { args, .. } => {
                for arg in args {
                    if let IrValue::Register(reg) = arg {
                        update_interval(reg);
                    }
                }
            }
            Phi { incoming, .. } => {
                for (value, _) in incoming {
                    if let IrValue::Register(reg) = value {
                        update_interval(reg);
                    }
                }
            }
            HeapAlloc { count, .. } => {
                if let Some(IrValue::Register(reg)) = count {
                    update_interval(reg);
                }
            }
            HeapFree { ptr } => {
                update_interval(ptr);
            }
            _ => {}
        }
    }

    fn record_defs(
        &self,
        inst: &IrInstruction,
        pos: usize,
        intervals: &mut HashMap<hll_to_ir::IrRegister, (usize, usize)>,
    ) {
        use IrInstruction::{
            Alloc, Call, Cast, Cmp, GlobalRef, HeapAlloc, Index, Load, Math, Offset, Phi, ReadReg,
            Unary,
        };

        let mut update_interval = |reg: &hll_to_ir::IrRegister| {
            let entry = intervals.entry(reg.clone()).or_insert((pos, pos));
            entry.1 = entry.1.max(pos);
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
            | GlobalRef { dest, .. } => {
                update_interval(dest);
            }
            Call {
                dest, function: _, ..
            } => {
                if let Some(dest) = dest {
                    update_interval(dest);
                }
            }
            _ => {}
        }
    }

    fn record_terminator_uses(
        &self,
        term: &IrTerminator,
        pos: usize,
        intervals: &mut HashMap<hll_to_ir::IrRegister, (usize, usize)>,
    ) {
        use IrTerminator::{Branch, Return};

        let mut update_interval = |reg: &hll_to_ir::IrRegister| {
            let entry = intervals.entry(reg.clone()).or_insert((pos, pos));
            entry.1 = entry.1.max(pos);
        };

        match term {
            Return(Some(val)) => {
                if let IrValue::Register(reg) = val {
                    update_interval(reg);
                }
            }
            Branch { cond, .. } => {
                if let IrValue::Register(reg) = cond {
                    update_interval(reg);
                }
            }
            _ => {}
        }
    }

    /// Linear-scan allocation; only integer/pointer types get physical registers.
    fn linear_scan_allocate(
        &mut self,
        intervals: &[LiveInterval],
        ctx: &mut FunctionContext,
        func: &IrFunction,
    ) {
        self.reg_mapping.clear();
        let mut active: Vec<(LiveInterval, Reg)> = Vec::new();
        let mut free_regs: Vec<Reg> = AVAILABLE_REGS.to_vec();

        for interval in intervals {
            if !self.is_allocatable_interval(interval, ctx, func) {
                continue;
            }

            self.expire_old_intervals(interval, &mut active, &mut free_regs);

            if free_regs.is_empty() {
                self.spill_at_interval(interval, &mut active, ctx);
            } else {
                let reg = free_regs.pop().unwrap();
                self.reg_mapping
                    .insert(interval.reg.clone(), Allocation::Physical(reg));
                active.push((interval.clone(), reg));
                active.sort_by_key(|(i, _)| i.end);
            }
        }
    }

    fn is_allocatable_interval(
        &self,
        interval: &LiveInterval,
        ctx: &FunctionContext,
        func: &IrFunction,
    ) -> bool {
        if func.params.iter().any(|p| p.register == interval.reg) {
            return false;
        }
        if ctx.is_stack_address(&interval.reg) {
            return false;
        }

        matches!(
            ctx.resolve_type(&interval.ty),
            IrType::Integer(_) | IrType::Pointer(_)
        )
    }

    fn expire_old_intervals(
        &self,
        current: &LiveInterval,
        active: &mut Vec<(LiveInterval, Reg)>,
        free_regs: &mut Vec<Reg>,
    ) {
        let mut expired = Vec::new();

        for (i, (interval, reg)) in active.iter().enumerate() {
            if interval.end >= current.start {
                break;
            }
            expired.push(i);
            free_regs.push(*reg);
        }

        // Remove in reverse order to keep indices valid.
        for i in expired.into_iter().rev() {
            active.remove(i);
        }
    }

    fn spill_at_interval(
        &mut self,
        current: &LiveInterval,
        active: &mut Vec<(LiveInterval, Reg)>,
        ctx: &mut FunctionContext,
    ) {
        // The active list is sorted by end point, so the last entry lives longest.
        if let Some(spill_candidate) = active.last() {
            if spill_candidate.0.end > current.end {
                // Spill the candidate and give its register to current.
                let spilled_reg = spill_candidate.1;
                let spilled_vreg = spill_candidate.0.reg.clone();

                let slot = ctx.slot_for_reg(&spilled_vreg).expect("slot exists");
                self.reg_mapping
                    .insert(spilled_vreg, Allocation::StackSlot(slot));
                self.reg_mapping
                    .insert(current.reg.clone(), Allocation::Physical(spilled_reg));

                active.pop();
                active.push((current.clone(), spilled_reg));
                active.sort_by_key(|(i, _)| i.end);
            } else {
                let slot = ctx.slot_for_reg(&current.reg).expect("slot exists");
                self.reg_mapping
                    .insert(current.reg.clone(), Allocation::StackSlot(slot));
            }
        } else {
            let slot = ctx.slot_for_reg(&current.reg).expect("slot exists");
            self.reg_mapping
                .insert(current.reg.clone(), Allocation::StackSlot(slot));
        }
    }

    /// Get the allocation for a virtual register.
    pub fn get_allocation(&self, reg: &hll_to_ir::IrRegister) -> Option<&Allocation> {
        self.reg_mapping.get(reg)
    }
}

impl Default for RegisterAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use hll_to_ir::{
        IntWidth, IrBlock, IrFunction, IrInstruction, IrMathOp, IrRegister, IrTerminator, IrType,
        IrValue,
    };

    use super::{Allocation, RegisterAllocator};
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

    #[test]
    fn alloc_slot_for_struct_reserves_full_struct_size() {
        let mut ctx = FunctionContext::new("test", &HashMap::new());
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

        let (_, ctx) = allocate_function(&func);

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
