//! Stack frame layout for a function.

use crate::intermediate_language::IrType;
use std::collections::HashMap;

pub struct FrameContext {
    /// Total frame size in bytes.
    frame_size: usize,
    /// Offset where return address (ra) is stored, if any.
    ra_offset: Option<usize>,
    /// List of (register index, offset) for saved callee-saved registers.
    saved_regs: Vec<(u8, usize)>,
    /// Next available offset from the frame base (negative from sp).
    next_offset: usize,
    /// Alignment requirement (16 bytes for RISC‑V ABI).
    alignment: usize,
}

impl FrameContext {
    pub fn new() -> Self {
        Self {
            frame_size: 0,
            ra_offset: None,
            saved_regs: Vec::new(),
            next_offset: 0,
            alignment: 16,
        }
    }

    fn align_to(value: usize, alignment: usize) -> usize {
        let alignment = alignment.max(1);
        (value + alignment - 1) & !(alignment - 1)
    }

    /// Reserve space for a stack slot of given size and alignment, return its offset from sp.
    pub fn alloc_slot(&mut self, size: usize, alignment: usize) -> usize {
        let offset = Self::align_to(self.next_offset, alignment);
        self.next_offset = offset + size;
        offset
    }

    /// Mark that the return address must be saved.
    pub fn save_ra(&mut self) {
        if self.ra_offset.is_none() {
            self.ra_offset = Some(self.alloc_slot(8, 8));
        }
    }

    /// Mark that a callee‑saved integer register must be saved.
    pub fn save_reg(&mut self, reg: u8) {
        if !self.saved_regs.iter().any(|(r, _)| *r == reg) {
            let offset = self.alloc_slot(8, 8);
            self.saved_regs.push((reg, offset));
        }
    }

    pub fn ra_offset(&self) -> Option<usize> {
        self.ra_offset
    }

    pub fn saved_regs(&self) -> &[(u8, usize)] {
        &self.saved_regs
    }

    pub fn finalize(&mut self) {
        // Align frame size to 16 bytes.
        self.frame_size = (self.next_offset + self.alignment - 1) & !(self.alignment - 1);
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Compute the size of a type after resolving aliases.
    pub fn type_size(&self, ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
        let resolved = self.resolve_type(ty, type_aliases);
        match resolved {
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
            IrType::Array { len, element } => len * self.type_size(&element, type_aliases),
            IrType::Aggregate(fields) => {
                let mut offset = 0usize;
                let mut aggregate_alignment = 1usize;

                for (_, field_ty) in fields {
                    let field_alignment = self.type_alignment(&field_ty, type_aliases);
                    aggregate_alignment = aggregate_alignment.max(field_alignment);
                    offset = Self::align_to(offset, field_alignment);
                    offset += self.type_size(&field_ty, type_aliases);
                }

                Self::align_to(offset, aggregate_alignment)
            }
            IrType::Named(_) => 8, // Should have been resolved, but fallback to pointer size
        }
    }

    pub fn type_alignment(&self, ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
        match self.resolve_type(ty, type_aliases) {
            IrType::Void => 1,
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
            IrType::Pointer(_) | IrType::Named(_) => 8,
            IrType::Array { element, .. } => self.type_alignment(&element, type_aliases),
            IrType::Aggregate(fields) => fields
                .iter()
                .map(|(_, field_ty)| self.type_alignment(field_ty, type_aliases))
                .max()
                .unwrap_or(1),
        }
    }

    pub fn resolve_type(&self, ty: &IrType, type_aliases: &HashMap<String, IrType>) -> IrType {
        self.resolve_type_inner(ty, type_aliases, &mut std::collections::HashSet::new())
    }

    fn resolve_type_inner(
        &self,
        ty: &IrType,
        type_aliases: &HashMap<String, IrType>,
        seen: &mut std::collections::HashSet<String>,
    ) -> IrType {
        match ty {
            IrType::Named(name) => {
                if let Some(resolved) = type_aliases.get(name) {
                    if !seen.insert(name.clone()) {
                        IrType::Named(name.clone())
                    } else {
                        let out = self.resolve_type_inner(resolved, type_aliases, seen);
                        seen.remove(name);
                        out
                    }
                } else {
                    IrType::Named(name.clone())
                }
            }
            IrType::Pointer(inner) => {
                IrType::Pointer(Box::new(self.resolve_type_inner(inner, type_aliases, seen)))
            }
            IrType::Array { len, element } => IrType::Array {
                len: *len,
                element: Box::new(self.resolve_type_inner(element, type_aliases, seen)),
            },
            IrType::Aggregate(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|(name, field_ty)| {
                        (
                            name.clone(),
                            self.resolve_type_inner(field_ty, type_aliases, seen),
                        )
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

impl Default for FrameContext {
    fn default() -> Self {
        Self::new()
    }
}
