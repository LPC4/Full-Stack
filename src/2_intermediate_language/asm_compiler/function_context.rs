use super::frame_context::FrameContext;
use crate::intermediate_language::{IrLabel, IrRegister, IrType};
use std::collections::HashMap;

pub struct FunctionContext {
    pub name: String,
    pub frame: FrameContext,
    /// Maps virtual registers to stack offsets.
    reg_slots: HashMap<IrRegister, usize>,
    /// Maps IR labels to emitted assembly labels.
    label_map: HashMap<IrLabel, String>,
}

impl FunctionContext {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            frame: FrameContext::new(),
            reg_slots: HashMap::new(),
            label_map: HashMap::new(),
        }
    }

    /// Allocate a stack slot for a virtual register.
    pub fn alloc_slot_for_reg(&mut self, reg: &IrRegister, ty: &IrType) -> usize {
        let size = type_size(ty);
        let slot = self.frame.alloc_slot(size);
        self.reg_slots.insert(reg.clone(), slot);
        slot
    }

    /// Get the stack offset for a virtual register.
    pub fn slot_for_reg(&self, reg: &IrRegister) -> Option<usize> {
        self.reg_slots.get(reg).copied()
    }

    /// Record that a virtual register is a function parameter (already has a stack slot).
    pub fn set_param_slot(&mut self, reg: &IrRegister, slot: usize) {
        self.reg_slots.insert(reg.clone(), slot);
    }

    /// Map an IR label to an assembly label string.
    pub fn map_label(&mut self, ir_label: &IrLabel, asm_label: String) {
        self.label_map.insert(ir_label.clone(), asm_label);
    }

    pub fn get_label(&self, ir_label: &IrLabel) -> Option<&String> {
        self.label_map.get(ir_label)
    }

    pub fn finalize(&mut self) {
        self.frame.finalize();
    }
}

fn type_size(ty: &IrType) -> usize {
    match ty {
        IrType::Void => 0,
        IrType::Integer(w) => match w {
            crate::intermediate_language::IntWidth::I1 => 1,
            crate::intermediate_language::IntWidth::I8 => 1,
            crate::intermediate_language::IntWidth::I16 => 2,
            crate::intermediate_language::IntWidth::I32 => 4,
            crate::intermediate_language::IntWidth::I64 => 8,
        },
        IrType::Float(w) => match w {
            crate::intermediate_language::FloatWidth::F32 => 4,
            crate::intermediate_language::FloatWidth::F64 => 8,
        },
        IrType::Pointer(_) => 8,
        IrType::Array { len, element } => len * type_size(element),
        IrType::Aggregate(fields) => {
            let mut offset = 0;
            for (_, field_ty) in fields {
                let align = type_alignment(field_ty);
                offset = (offset + align - 1) & !(align - 1);
                offset += type_size(field_ty);
            }
            offset
        }
        IrType::Named(_) => 8, // assume pointer sized
    }
}

fn type_alignment(ty: &IrType) -> usize {
    match ty {
        IrType::Integer(w) => match w {
            crate::intermediate_language::IntWidth::I1 => 1,
            crate::intermediate_language::IntWidth::I8 => 1,
            crate::intermediate_language::IntWidth::I16 => 2,
            crate::intermediate_language::IntWidth::I32 => 4,
            crate::intermediate_language::IntWidth::I64 => 8,
        },
        IrType::Float(w) => match w {
            crate::intermediate_language::FloatWidth::F32 => 4,
            crate::intermediate_language::FloatWidth::F64 => 8,
        },
        IrType::Pointer(_) => 8,
        IrType::Array { element, .. } => type_alignment(element),
        IrType::Aggregate(fields) => {
            fields.iter().map(|(_, t)| type_alignment(t)).max().unwrap_or(1)
        }
        _ => 1,
    }
}