use super::frame_context::FrameContext;
use crate::intermediate_language::{IrLabel, IrRegister, IrType};
use std::collections::{HashMap, HashSet};

pub struct FunctionContext {
    pub name: String,
    pub frame: FrameContext,
    type_aliases: HashMap<String, IrType>,
    /// Maps virtual registers to stack offsets.
    reg_slots: HashMap<IrRegister, usize>,
    /// Maps IR labels to emitted assembly labels.
    label_map: HashMap<IrLabel, String>,
}

impl FunctionContext {
    pub fn new(name: &str, type_aliases: &HashMap<String, IrType>) -> Self {
        Self {
            name: name.to_string(),
            frame: FrameContext::new(),
            type_aliases: type_aliases.clone(),
            reg_slots: HashMap::new(),
            label_map: HashMap::new(),
        }
    }

    /// Allocate a stack slot for a virtual register.
    pub fn alloc_slot_for_reg(&mut self, reg: &IrRegister, ty: &IrType) -> usize {
        let size = self.type_size(ty);
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

    pub fn resolve_type(&self, ty: &IrType) -> IrType {
        self.resolve_type_inner(ty, &mut HashSet::new())
    }

    pub fn type_size(&self, ty: &IrType) -> usize {
        match self.resolve_type(ty) {
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
            IrType::Array { len, element } => len * self.type_size(&element),
            IrType::Aggregate(fields) => fields.iter().map(|(_, t)| self.type_size(t)).sum(),
            IrType::Named(_) => 8,
        }
    }

    fn resolve_type_inner(&self, ty: &IrType, seen: &mut HashSet<String>) -> IrType {
        match ty {
            IrType::Named(name) => self
                .type_aliases
                .get(name)
                .cloned()
                .map(|resolved| {
                    if !seen.insert(name.clone()) {
                        IrType::Named(name.clone())
                    } else {
                        let out = self.resolve_type_inner(&resolved, seen);
                        seen.remove(name);
                        out
                    }
                })
                .unwrap_or_else(|| IrType::Named(name.clone())),
            IrType::Pointer(inner) => IrType::Pointer(Box::new(self.resolve_type_inner(inner, seen))),
            IrType::Array { len, element } => IrType::Array {
                len: *len,
                element: Box::new(self.resolve_type_inner(element, seen)),
            },
            IrType::Aggregate(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|(name, field_ty)| (name.clone(), self.resolve_type_inner(field_ty, seen)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

