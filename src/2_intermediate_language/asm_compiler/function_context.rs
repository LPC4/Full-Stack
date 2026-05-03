use super::frame_context::FrameContext;
use crate::assembly_language::encode_decode::Reg;
use crate::intermediate_language::{IrFunction, IrLabel, IrRegister, IrType};
use std::collections::{HashMap, HashSet};

// RISC-V register numbers used in prologue/epilogue.
const SP: Reg = 2;
const RA: Reg = 1;
const S0: Reg = 8; // callee-saved frame pointer (used as temp in prologue)

pub trait Rv64Backend {
    fn alloc_temp_reg(&mut self) -> Reg;
    fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64);
    fn emit_sd(&mut self, base: Reg, src: Reg, offset: i32);
    fn emit_ld(&mut self, rd: Reg, base: Reg, offset: i32);
    fn emit_mv(&mut self, rd: Reg, rs: Reg);
    fn emit_jalr(&mut self, rd: Reg, rs1: Reg, imm: i32);
    fn emit_li(&mut self, rd: Reg, imm: i64);
    fn emit_store_from_tmp(&mut self, addr_reg: Reg, val_reg: Reg, ty: &IrType, offset: i32);
    fn emit_load_to_slot(&mut self, slot: usize, addr_reg: Reg, ty: &IrType, offset: i32);
    fn emit_comment(&mut self, text: &str);
}

/// Function-level context that owns prologue/epilogue emission and frame layout.
pub struct FunctionContext {
    pub name: String,
    pub frame: FrameContext,
    type_aliases: HashMap<String, IrType>,
    /// Maps virtual registers to stack offsets.
    reg_slots: HashMap<IrRegister, usize>,
    /// Records the IR type associated with each virtual register.
    reg_types: HashMap<IrRegister, IrType>,
    /// Registers whose value is the address of their own stack slot.
    stack_address_regs: HashSet<IrRegister>,
    /// Maps IR labels to emitted assembly labels.
    label_map: HashMap<IrLabel, String>,
}

impl FunctionContext {
    pub fn new(name: &str, type_aliases: &HashMap<String, IrType>) -> Self {
        Self {
            name: name.to_owned(),
            frame: FrameContext::new(),
            type_aliases: type_aliases.clone(),
            reg_slots: HashMap::new(),
            reg_types: HashMap::new(),
            stack_address_regs: HashSet::new(),
            label_map: HashMap::new(),
        }
    }

    /// Allocate a stack slot for a virtual register.
    pub fn alloc_slot_for_reg(&mut self, reg: &IrRegister, ty: &IrType) -> usize {
        let size = self.frame.type_size(ty, &self.type_aliases);
        let alignment = self.frame.type_alignment(ty, &self.type_aliases);
        let slot = self.frame.alloc_slot(size, alignment);
        self.reg_slots.insert(reg.clone(), slot);
        self.reg_types.insert(reg.clone(), ty.clone());
        slot
    }

    /// Reserve space for saving `ra`.
    pub fn save_ra(&mut self) {
        self.frame.save_ra();
    }

    /// Reserve space for saving a callee-saved register.
    pub fn save_reg(&mut self, reg: u8) {
        self.frame.save_reg(reg);
    }

    /// Finalize the frame layout.
    pub fn finalize(&mut self) {
        self.frame.finalize();
    }

    /// Total stack frame size in bytes.
    pub fn frame_size(&self) -> usize {
        self.frame.frame_size()
    }

    /// Stack offset of the saved return address, if present.
    pub fn ra_offset(&self) -> Option<usize> {
        self.frame.ra_offset()
    }

    /// Saved callee-saved registers and their stack offsets.
    pub fn saved_regs(&self) -> &[(u8, usize)] {
        self.frame.saved_regs()
    }

    /// Get the stack offset for a virtual register.
    pub fn slot_for_reg(&self, reg: &IrRegister) -> Option<usize> {
        self.reg_slots.get(reg).copied()
    }

    /// Record that a virtual register is a function parameter (already has a stack slot).
    pub fn set_param_slot(&mut self, reg: &IrRegister, slot: usize) {
        self.reg_slots.insert(reg.clone(), slot);
    }

    pub fn set_reg_type(&mut self, reg: &IrRegister, ty: IrType) {
        self.reg_types.insert(reg.clone(), ty);
    }

    pub fn type_for_reg(&self, reg: &IrRegister) -> Option<IrType> {
        self.reg_types.get(reg).cloned()
    }

    /// Mark that a register's value should be computed as `sp + slot`.
    pub fn mark_stack_address(&mut self, reg: &IrRegister) {
        self.stack_address_regs.insert(reg.clone());
    }

    pub fn is_stack_address(&self, reg: &IrRegister) -> bool {
        self.stack_address_regs.contains(reg)
    }

    /// Map an IR label to an assembly label string.
    pub fn map_label(&mut self, ir_label: &IrLabel, asm_label: String) {
        self.label_map.insert(ir_label.clone(), asm_label);
    }

    pub fn get_label(&self, ir_label: &IrLabel) -> Option<&String> {
        self.label_map.get(ir_label)
    }

    /// Emit the function prologue using the given backend.
    pub fn emit_prologue(&self, backend: &mut impl Rv64Backend) {
        backend.emit_comment("--- Function Prologue ---");
        let frame_size = self.frame_size();
        backend.emit_comment(&format!("Allocate stack frame: {frame_size} bytes"));
        backend.emit_add_imm(SP, SP, -(frame_size as i64));
        if let Some(offset) = self.ra_offset() {
            backend.emit_comment(&format!("Save return address (ra) at offset {offset}"));
            backend.emit_sd(SP, RA, offset as i32);
        }
        for (reg, offset) in self.saved_regs() {
            backend.emit_comment(&format!(
                "Save callee-saved register s{reg} at offset {offset}"
            ));
            backend.emit_sd(SP, *reg, *offset as i32);
        }
        backend.emit_comment("Set up frame pointer");
        backend.emit_mv(S0, SP);
        backend.emit_comment("--- End Prologue ---");
    }

    /// Emit the function epilogue using the given backend.
    pub fn emit_epilogue(&self, backend: &mut impl Rv64Backend) {
        backend.emit_comment("--- Function Epilogue ---");
        for (reg, offset) in self.saved_regs().iter().rev() {
            backend.emit_comment(&format!(
                "Restore callee-saved register s{reg} from offset {offset}"
            ));
            backend.emit_ld(*reg, SP, *offset as i32);
        }
        if let Some(offset) = self.ra_offset() {
            backend.emit_comment(&format!("Restore return address (ra) from offset {offset}"));
            backend.emit_ld(RA, SP, offset as i32);
        }
        let frame_size = self.frame_size();
        backend.emit_comment(&format!("Deallocate stack frame: {frame_size} bytes"));
        backend.emit_add_imm(SP, SP, frame_size as i64);
        backend.emit_comment("Return to caller");
        backend.emit_jalr(0, RA, 0);
        backend.emit_comment("--- End Epilogue ---");
    }

    /// Emit spills for function parameters that arrive in registers or on the stack.
    pub fn emit_parameter_spills(&self, backend: &mut impl Rv64Backend, func: &IrFunction) {
        if func.params.is_empty() {
            return;
        }

        backend.emit_comment("--- Function Parameter Spills ---");
        let frame_size = self.frame_size() as i64;
        let caller_sp = backend.alloc_temp_reg();
        backend.emit_add_imm(caller_sp, S0, frame_size);

        for (index, param) in func.params.iter().enumerate() {
            let slot = self.slot_for_reg(&param.register).expect("param slot");
            let ty = self.frame.resolve_type(&param.ty, &self.type_aliases);
            if index < 8 {
                backend.emit_comment(&format!(
                    "Spill parameter '{}' from register a{} to stack slot {}",
                    param.register, index, slot
                ));
                backend.emit_store_from_tmp(SP, arg_reg(index), &ty, slot as i32);
            } else {
                let offset = ((index - 8) * 8) as i32;
                backend.emit_comment(&format!(
                    "Spill parameter '{}' from caller's stack (offset {}) to slot {}",
                    param.register, offset, slot
                ));
                backend.emit_load_to_slot(slot, caller_sp, &ty, offset);
            }
        }
        backend.emit_comment("--- End Parameter Spills ---");
    }

    /// Emit spills for function parameters when the function has an sret (hidden pointer) parameter.
    /// The sret pointer arrives in a0 and needs to be preserved before regular parameter spills.
    pub fn emit_parameter_spills_with_sret(
        &self,
        backend: &mut impl Rv64Backend,
        func: &IrFunction,
        sret_slot: usize,
    ) {
        backend.emit_comment("--- Function Parameter Spills (with sret) ---");
        // First, save the sret pointer from a0 to its designated slot
        // The sret pointer is already in a0 at function entry
        let sret_ptr = arg_reg(0); // a0 contains the sret pointer
        backend.emit_comment(&format!(
            "Save sret pointer from a0 to stack slot {sret_slot}"
        ));
        backend.emit_store_from_tmp(
            SP,
            sret_ptr,
            &IrType::Pointer(Box::new(IrType::Void)),
            sret_slot as i32,
        );

        // Now spill the regular parameters (skip index 0 which is __sret, already handled above)
        if func.params.is_empty() {
            backend.emit_comment("--- End Parameter Spills ---");
            return;
        }

        let frame_size = self.frame_size() as i64;
        let caller_sp = backend.alloc_temp_reg();
        backend.emit_add_imm(caller_sp, S0, frame_size);

        for (index, param) in func.params.iter().enumerate().skip(1) {
            let slot = self.slot_for_reg(&param.register).expect("param slot");
            let ty = self.frame.resolve_type(&param.ty, &self.type_aliases);
            // index 1 = first real param = a1, index 2 = a2, etc.
            // Use arg_reg(index) directly -- no shift needed
            if index < 8 {
                backend.emit_comment(&format!(
                    "Spill parameter '{}' from register a{} to stack slot {}",
                    param.register, index, slot
                ));
                backend.emit_store_from_tmp(SP, arg_reg(index), &ty, slot as i32);
            } else {
                let offset = ((index - 8) * 8) as i32;
                backend.emit_comment(&format!(
                    "Spill parameter '{}' from caller's stack (offset {}) to slot {}",
                    param.register, offset, slot
                ));
                backend.emit_load_to_slot(slot, caller_sp, &ty, offset);
            }
        }
        backend.emit_comment("--- End Parameter Spills ---");
    }
}

/// Return the argument register for the given index (a0-a7).
fn arg_reg(i: usize) -> Reg {
    match i {
        0 => 10,
        1 => 11,
        2 => 12,
        3 => 13,
        4 => 14,
        5 => 15,
        6 => 16,
        7 => 17,
        _ => 0,
    }
}
