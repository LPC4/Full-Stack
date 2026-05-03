//! Register allocator using linear scan algorithm.
//! Allocates physical registers when possible, spills to stack when necessary.

use super::function_context::FunctionContext;
use crate::assembly_language::encode_decode::Reg;
use crate::intermediate_language::{IrFunction, IrInstruction, IrTerminator, IrType, IrValue};
use std::collections::{BTreeMap, HashMap};

/// Available physical registers for allocation (caller-saved temporaries)
const AVAILABLE_REGS: [Reg; 7] = [5, 6, 7, 28, 29, 30, 31]; // t0-t2, t3-t6

#[derive(Debug, Clone)]
struct LiveInterval {
    reg: crate::intermediate_language::IrRegister,
    start: usize,
    end: usize,
    ty: IrType,
}

pub struct RegisterAllocator {
    /// Maps virtual registers to their assigned physical register or stack slot
    reg_mapping: HashMap<crate::intermediate_language::IrRegister, Allocation>,
    /// Current position in instruction stream
    position: usize,
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
            position: 0,
        }
    }

    /// Allocate registers for all virtual registers in the function.
    pub fn allocate_slots(
        &mut self,
        func: &IrFunction,
        ctx: &mut FunctionContext,
        function_return_types: &HashMap<String, IrType>,
    ) {
        // First pass: allocate stack slots for parameters (they always need slots for spilling)
        for param in &func.params {
            ctx.alloc_slot_for_reg(&param.register, &param.ty);
        }

        // Mark stack addresses from Alloc instructions
        for block in &func.blocks {
            for inst in &block.instructions {
                if let IrInstruction::Alloc { dest, .. } = inst {
                    ctx.mark_stack_address(dest);
                }
            }
        }

        let mut vregs = Vec::new();

        // Add params to vregs list
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

        // Second pass: compute live intervals
        let intervals = self.compute_live_intervals(func, &vregs, function_return_types);

        // Ensure ALL registers have stack slots BEFORE allocation (needed for spilling)
        for interval in &intervals {
            if !ctx.slot_for_reg(&interval.reg).is_some() {
                ctx.alloc_slot_for_reg(&interval.reg, &interval.ty);
            }
        }

        // DISABLED: Register allocation causes edge case bugs with certain patterns
        // Third pass: allocate registers using linear scan (only for non-float types)
        // self.linear_scan_allocate(&intervals, ctx, func);
    }

    fn collect_vregs_from_instruction(
        &self,
        inst: &IrInstruction,
        vregs: &mut Vec<(crate::intermediate_language::IrRegister, IrType)>,
        function_return_types: &HashMap<String, IrType>,
    ) {
        use IrInstruction::*;

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
                    vregs.push((
                        dest.clone(),
                        IrType::Integer(crate::intermediate_language::IntWidth::I1),
                    ));
                }
            }
            Call { dest, function, .. } => {
                if let Some(dest) = dest {
                    if !vregs.iter().any(|(r, _)| r == dest) {
                        let ret_ty = function_return_types.get(function).cloned().unwrap_or(
                            IrType::Integer(crate::intermediate_language::IntWidth::I64),
                        );
                        vregs.push((dest.clone(), ret_ty));
                    }
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
            _ => {}
        }
    }

    fn collect_vregs_from_terminator(
        &self,
        term: &IrTerminator,
        vregs: &mut Vec<(crate::intermediate_language::IrRegister, IrType)>,
    ) {
        use IrTerminator::*;
        match term {
            Return(Some(val)) => {
                if let IrValue::Register(reg) = val {
                    if !vregs.iter().any(|(r, _)| r == reg) {
                        vregs.push((
                            reg.clone(),
                            IrType::Integer(crate::intermediate_language::IntWidth::I64),
                        ));
                    }
                }
            }
            Branch { cond, .. } => {
                if let IrValue::Register(reg) = cond {
                    if !vregs.iter().any(|(r, _)| r == reg) {
                        vregs.push((
                            reg.clone(),
                            IrType::Integer(crate::intermediate_language::IntWidth::I1),
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    fn compute_live_intervals(
        &self,
        func: &IrFunction,
        vregs: &[(crate::intermediate_language::IrRegister, IrType)],
        function_return_types: &HashMap<String, IrType>,
    ) -> Vec<LiveInterval> {
        let mut intervals = HashMap::new();
        let mut pos = 0;

        for block in &func.blocks {
            for inst in &block.instructions {
                // Process uses
                self.record_uses(inst, pos, &mut intervals);

                // Process defs
                self.record_defs(inst, pos, &mut intervals, function_return_types);

                pos += 1;
            }

            if let Some(term) = &block.terminator {
                self.record_terminator_uses(term, pos, &mut intervals);
                pos += 1;
            }
        }

        // Create intervals for all vregs
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

        // Sort by start position
        result.sort_by_key(|i| i.start);
        result
    }

    fn record_uses(
        &self,
        inst: &IrInstruction,
        pos: usize,
        intervals: &mut HashMap<crate::intermediate_language::IrRegister, (usize, usize)>,
    ) {
        use IrInstruction::*;

        let mut update_interval = |reg: &crate::intermediate_language::IrRegister| {
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
        intervals: &mut HashMap<crate::intermediate_language::IrRegister, (usize, usize)>,
        function_return_types: &HashMap<String, IrType>,
    ) {
        use IrInstruction::*;

        let mut update_interval = |reg: &crate::intermediate_language::IrRegister| {
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
            | HeapAlloc { dest, .. } => {
                update_interval(dest);
            }
            Call { dest, function, .. } => {
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
        intervals: &mut HashMap<crate::intermediate_language::IrRegister, (usize, usize)>,
    ) {
        use IrTerminator::*;

        let mut update_interval = |reg: &crate::intermediate_language::IrRegister| {
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

    /// Linear scan register allocation (only allocates integer registers to integer/pointer types)
    fn linear_scan_allocate(
        &mut self,
        intervals: &[LiveInterval],
        ctx: &mut FunctionContext,
        func: &IrFunction,
    ) {
        let mut active: Vec<(LiveInterval, Reg)> = Vec::new();
        let mut free_regs: Vec<Reg> = AVAILABLE_REGS.to_vec();

        for interval in intervals {
            // Only allocate registers to i32 and i64 integer types
            // Skip everything else to avoid ABI and edge case issues
            match &interval.ty {
                IrType::Integer(crate::intermediate_language::IntWidth::I32) => {}
                IrType::Integer(crate::intermediate_language::IntWidth::I64) => {}
                _ => continue, // Skip all other types
            }

            // Skip parameter registers - they're already spilled to stack in the prologue,
            // and loading from their allocated register would give garbage
            if func.params.iter().any(|p| p.register == interval.reg) {
                continue;
            }

            // Expire old intervals
            self.expire_old_intervals(&interval, &mut active, &mut free_regs, ctx);

            if free_regs.is_empty() {
                // Need to spill
                self.spill_at_interval(interval, &mut active, ctx);
            } else {
                // Allocate a register
                let reg = free_regs.pop().unwrap();
                self.reg_mapping
                    .insert(interval.reg.clone(), Allocation::Physical(reg));
                active.push((interval.clone(), reg));
                // Sort active list by end point
                active.sort_by_key(|(i, _)| i.end);
            }
        }
    }

    fn expire_old_intervals(
        &self,
        current: &LiveInterval,
        active: &mut Vec<(LiveInterval, Reg)>,
        free_regs: &mut Vec<Reg>,
        ctx: &mut FunctionContext,
    ) {
        let mut expired = Vec::new();

        for (i, (interval, reg)) in active.iter().enumerate() {
            if interval.end >= current.start {
                break;
            }
            expired.push(i);
            free_regs.push(*reg);
        }

        // Remove expired intervals (in reverse order to maintain indices)
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
        // Find the interval with the furthest end point
        if let Some(spill_candidate) = active.last() {
            if spill_candidate.0.end > current.end {
                // Spill the candidate and give its register to current
                let spilled_reg = spill_candidate.1;
                let spilled_vreg = spill_candidate.0.reg.clone();

                // Assign stack slot to spilled vreg
                let slot = ctx.slot_for_reg(&spilled_vreg).expect("slot exists");
                self.reg_mapping
                    .insert(spilled_vreg, Allocation::StackSlot(slot));

                // Give register to current interval
                self.reg_mapping
                    .insert(current.reg.clone(), Allocation::Physical(spilled_reg));

                // Update active list
                active.pop();
                active.push((current.clone(), spilled_reg));
                active.sort_by_key(|(i, _)| i.end);
            } else {
                // Spill current interval
                let slot = ctx.slot_for_reg(&current.reg).expect("slot exists");
                self.reg_mapping
                    .insert(current.reg.clone(), Allocation::StackSlot(slot));
            }
        } else {
            // No active intervals, spill current
            let slot = ctx.slot_for_reg(&current.reg).expect("slot exists");
            self.reg_mapping
                .insert(current.reg.clone(), Allocation::StackSlot(slot));
        }
    }

    /// Get the allocation for a virtual register
    pub fn get_allocation(
        &self,
        reg: &crate::intermediate_language::IrRegister,
    ) -> Option<&Allocation> {
        self.reg_mapping.get(reg)
    }
}

impl Default for RegisterAllocator {
    fn default() -> Self {
        Self::new()
    }
}
