//! Stack frame layout for a function.

use crate::intermediate_language::IrType;
use std::collections::HashMap;
use super::type_utils;

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
        type_utils::type_size(ty, type_aliases)
    }

    pub fn type_alignment(&self, ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
        type_utils::type_alignment(ty, type_aliases)
    }

    pub fn resolve_type(&self, ty: &IrType, type_aliases: &HashMap<String, IrType>) -> IrType {
        type_utils::resolve_ir_type(ty, type_aliases)
    }
}

impl Default for FrameContext {
    fn default() -> Self {
        Self::new()
    }
}
