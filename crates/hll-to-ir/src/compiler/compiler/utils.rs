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
