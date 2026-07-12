use super::{
    Expression, FloatWidth, HighLevelCompiler, IntWidth, IrBlock, IrInstruction, IrLabel,
    IrRegister, IrTerminator, IrType, IrValue,
};

impl HighLevelCompiler {
    pub(super) fn start_new_block(&mut self, label: impl Into<String>) {
        if let Some(block) = self.current_block.take() {
            self.current_blocks.push(block);
        }
        self.current_block = Some(IrBlock::new(label));
    }

    pub(super) fn push_instruction(&mut self, inst: IrInstruction) {
        if let Some(b) = self.current_block.as_mut() {
            b.push_instruction(inst);
        } else {
            log::warn!("Instruction pushed without active block: {inst:?}");
        }
    }

    /// Emit a pc-to-source marker for `line`, skipping unknown lines (0) and
    /// consecutive duplicates. Reset per function via [`Self::reset_source_line`].
    pub(super) fn emit_source_line(&mut self, line: u32) {
        // Shift past the prelude; statements inside it map to 0 and are dropped.
        let line = line.saturating_sub(self.source_line_offset);
        if line == 0 {
            return;
        }
        // Always refresh the diagnostic location, even when the marker is a
        // duplicate, so an error on this line points at it.
        self.set_pending_span(line);
        if self.last_source_line == Some(line) {
            return;
        }
        self.last_source_line = Some(line);
        // Without a file name the entry could never map back to a source view,
        // and it would shadow real entries below it in the linked table.
        if self.source_file.is_empty() {
            return;
        }
        let file = self.source_file.clone();
        self.push_instruction(IrInstruction::SourceLine { file, line });
    }

    /// Point later `context.error`/`warn` calls at 1-based user `line`, spanning
    /// its non-whitespace text so the offending statement is underlined.
    pub(super) fn set_pending_span(&mut self, line: u32) {
        let source_line = self
            .source_lines
            .get(line as usize - 1)
            .cloned()
            .unwrap_or_default();
        let lead = source_line.len() - source_line.trim_start().len();
        let span = crate::token::Span {
            line,
            col: (lead + 1) as u32,
            len: source_line.trim().len() as u32,
            source_line,
        };
        self.context.pending_span = Some(span);
    }

    /// Forget the last emitted line so the next statement re-emits its marker.
    pub(super) fn reset_source_line(&mut self) {
        self.last_source_line = None;
    }

    pub(super) fn set_terminator(&mut self, term: IrTerminator) {
        if let Some(b) = self.current_block.as_mut()
            && b.terminator.is_none()
        {
            b.set_terminator(term);
        }
    }

    pub(super) fn new_label(&mut self) -> IrLabel {
        let current = self.next_label;
        self.next_label = self.next_label.saturating_add(1);
        IrLabel::new(format!("label_{current}"))
    }

    pub(super) fn new_temp(&mut self) -> IrRegister {
        let current = self.next_temp;
        self.next_temp = self.next_temp.saturating_add(1);
        IrRegister::Temp(current)
    }

    pub(super) fn infer_type_from_value(&self, value: &IrValue) -> IrType {
        match value {
            IrValue::Integer(_) => IrType::Integer(IntWidth::I32),
            IrValue::Float(_) => IrType::Float(FloatWidth::F64),
            IrValue::Bool(_) => IrType::Integer(IntWidth::I1),
            IrValue::Register(_) => IrType::Void, // Default fallback
            IrValue::Null => IrType::Pointer(Box::new(IrType::Named("unknown".to_owned()))),
            IrValue::GlobalString(_) => IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
        }
    }

    pub(super) fn aggregate_field_offset_and_type(
        &self,
        fields: &[(String, IrType)],
        field: &str,
    ) -> Option<(i64, IrType)> {
        let mut offset = 0i64;
        for (idx, (name, field_ty)) in fields.iter().enumerate() {
            let field_alignment = self.type_alignment_in_bytes(field_ty) as i64;
            offset = Self::align_to(offset, field_alignment);
            if name == field || idx.to_string() == field {
                return Some((offset, field_ty.clone()));
            }
            offset += self.type_size_in_bytes(field_ty) as i64;
        }
        None
    }

    pub(super) fn resolve_named_type(&self, ty: &IrType) -> IrType {
        match ty {
            IrType::Named(name) => self
                .context
                .types
                .resolve(name)
                .cloned()
                .unwrap_or_else(|| IrType::Named(name.clone())),
            IrType::Pointer(inner) => IrType::Pointer(Box::new(self.resolve_named_type(inner))),
            IrType::FunctionPointer {
                params,
                return_type,
            } => IrType::FunctionPointer {
                params: params
                    .iter()
                    .map(|ty| self.resolve_named_type(ty))
                    .collect(),
                return_type: Box::new(self.resolve_named_type(return_type)),
            },
            IrType::Array { len, element } => IrType::Array {
                len: *len,
                element: Box::new(self.resolve_named_type(element)),
            },
            IrType::Aggregate(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), self.resolve_named_type(ty)))
                    .collect(),
            ),
            IrType::Slice(inner) => IrType::Slice(Box::new(self.resolve_named_type(inner))),
            other => other.clone(),
        }
    }

    pub(super) fn format_expression(&self, expr: &Expression) -> String {
        format!("{expr:?}")
    }

    pub(super) fn type_size_in_bytes(&self, ty: &IrType) -> usize {
        match &self.resolve_named_type(ty) {
            IrType::Integer(width) => match width {
                IntWidth::I1 | IntWidth::I8 => 1,
                IntWidth::I16 => 2,
                IntWidth::I32 => 4,
                IntWidth::I64 => 8,
            },
            IrType::Float(width) => match width {
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            },
            IrType::Pointer(_) => 8,
            IrType::FunctionPointer { .. } => 8,
            // Fat pointer: { ptr: 8, len: 8 }.
            IrType::Slice(_) => 16,
            IrType::Array { len, element } => len * self.type_size_in_bytes(element),
            IrType::Aggregate(fields) => {
                let mut offset = 0i64;
                let mut aggregate_alignment = 1i64;

                for (_, field_ty) in fields {
                    let field_alignment = self.type_alignment_in_bytes(field_ty) as i64;
                    aggregate_alignment = aggregate_alignment.max(field_alignment);
                    offset = Self::align_to(offset, field_alignment);
                    offset += self.type_size_in_bytes(field_ty) as i64;
                }

                Self::align_to(offset, aggregate_alignment) as usize
            }
            _ => 0,
        }
    }

    pub(super) fn reflected_layout(&self, ty: &IrType) -> Option<(usize, usize)> {
        match &self.resolve_named_type(ty) {
            IrType::Void | IrType::Named(_) => None,
            resolved => Some((
                self.type_size_in_bytes(resolved),
                self.type_alignment_in_bytes(resolved),
            )),
        }
    }

    pub(super) fn type_alignment_in_bytes(&self, ty: &IrType) -> usize {
        match &self.resolve_named_type(ty) {
            IrType::Void => 1,
            IrType::Integer(width) => match width {
                IntWidth::I1 | IntWidth::I8 => 1,
                IntWidth::I16 => 2,
                IntWidth::I32 => 4,
                IntWidth::I64 => 8,
            },
            IrType::Float(width) => match width {
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            },
            IrType::Pointer(_)
            | IrType::FunctionPointer { .. }
            | IrType::Named(_)
            | IrType::Slice(_) => 8,
            IrType::Array { element, .. } => self.type_alignment_in_bytes(element),
            IrType::Aggregate(fields) => fields
                .iter()
                .map(|(_, t)| self.type_alignment_in_bytes(t))
                .max()
                .unwrap_or(1),
        }
    }

    pub(super) fn is_unsigned_primitive_type(ty: &crate::ast::Type) -> bool {
        match ty {
            crate::ast::Type::Primitive(name) => {
                matches!(name.as_str(), "u8" | "u16" | "u32" | "u64")
            }
            _ => false,
        }
    }

    pub fn diagnostics(&self) -> &[crate::compiler::Diagnostic] {
        self.context.diagnostics.entries()
    }
}
