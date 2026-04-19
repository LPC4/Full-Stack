use super::*;

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
            log::warn!("Instruction pushed without active block: {:?}", inst);
        }
    }

    pub(super) fn set_terminator(&mut self, term: IrTerminator) {
        if let Some(b) = self.current_block.as_mut() {
            if b.terminator.is_none() {
                b.set_terminator(term);
            }
        }
    }

    pub(super) fn new_label(&mut self) -> IrLabel {
        let current = self.next_label;
        self.next_label = self.next_label.saturating_add(1);
        IrLabel::new(format!("label_{}", current))
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
            IrValue::Null => IrType::Pointer(Box::new(IrType::Named("unknown".to_string()))),
        }
    }

    pub(super) fn aggregate_field_offset_and_type(
        &self,
        fields: &[(String, IrType)],
        field: &str,
    ) -> Option<(i64, IrType)> {
        for (idx, (name, field_ty)) in fields.iter().enumerate() {
            if name == field || idx.to_string() == field {
                return Some(((idx as i64) * 8, field_ty.clone()));
            }
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
            other => other.clone(),
        }
    }

    pub(super) fn format_assign_target(&self, target: &AssignTarget) -> String {
        match target {
            AssignTarget::Identifier(name) => name.clone(),
            AssignTarget::Dereference(inner) => format!("@{}", self.format_assign_target(inner)),
            AssignTarget::FieldAccess { expr, field } => {
                format!("{}.{}", self.format_assign_target(expr), field)
            }
            AssignTarget::ArrayIndex { expr, index } => {
                format!(
                    "{}[{}]",
                    self.format_assign_target(expr),
                    self.format_expression(index)
                )
            }
            AssignTarget::Tuple(fields) => {
                let items = fields
                    .iter()
                    .map(|f| f.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", items)
            }
        }
    }

    pub(super) fn format_expression(&self, expr: &Expression) -> String {
        format!("{expr:?}")
    }

    pub(super) fn is_deref_based_index_expr(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Unary {
                op: UnaryOp::Dereference,
                ..
            } => true,
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::FieldAccess {
                expr,
                ..
            }) => self.is_deref_based_index_expr(expr),
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::ArrayIndex {
                expr,
                ..
            }) => self.is_deref_based_index_expr(expr),
            _ => false,
        }
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
            IrType::Pointer(_) => 8, // 64-bit ABI
            IrType::Array { len, element } => len * self.type_size_in_bytes(element),
            IrType::Aggregate(fields) => {
                fields.iter().map(|(_, t)| self.type_size_in_bytes(t)).sum()
            }
            _ => 0,
        }
    }
}
