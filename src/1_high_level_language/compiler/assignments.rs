use super::*;

impl HighLevelCompiler {
    pub(super) fn lower_field_access(
        &mut self,
        base: &LoweredValue,
        field: &str,
    ) -> Option<LoweredValue> {
        let resolved_base_ty = self.resolve_named_type(&base.ty);
        let (aggregate_ptr_reg, fields) = match &resolved_base_ty {
            IrType::Aggregate(fields) => {
                let ptr_reg = match &base.value {
                    IrValue::Register(reg) => reg.clone(),
                    _ => return None,
                };
                (ptr_reg, fields.clone())
            }
            IrType::Pointer(inner) => match inner.as_ref() {
                IrType::Aggregate(fields) => {
                    let ptr_reg = match &base.value {
                        IrValue::Register(reg) => reg.clone(),
                        _ => return None,
                    };
                    (ptr_reg, fields.clone())
                }
                _ => return None,
            },
            _ => return None,
        };

        let (offset, ty) = self.aggregate_field_offset_and_type(&fields, field)?;
        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: dest.clone(),
            ty: ty.clone(),
            ptr: aggregate_ptr_reg,
            offset: Some(offset),
        });
        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty,
        })
    }

    pub(super) fn lower_array_index(
        &mut self,
        base: &LoweredValue,
        index: &LoweredValue,
    ) -> Option<LoweredValue> {
        match &base.ty {
            IrType::Array { element, .. } | IrType::Pointer(element) => {
                if let IrValue::Register(ptr_reg) = &base.value {
                    let dest = self.new_temp();
                    self.push_instruction(IrInstruction::Index {
                        dest: dest.clone(),
                        ty: *element.clone(),
                        base_ptr: ptr_reg.clone(),
                        idx: index.value.clone(),
                    });

                    // Return the POINTER, not the loaded value.
                    // The `@` operator in the AST will handle the actual load.
                    return Some(LoweredValue {
                        value: IrValue::Register(dest),
                        ty: IrType::Pointer(element.clone()),
                    });
                }
                None
            }
            _ => None,
        }
    }

    pub(super) fn lower_deref_assign(
        &mut self,
        target: &AssignTarget,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        let (pointee_ptr_reg, pointee_ty) = self.resolve_deref_lvalue(target)?;
        self.push_instruction(IrInstruction::Store {
            ty: pointee_ty,
            value: value.value.clone(),
            ptr: pointee_ptr_reg,
            offset: None,
        });
        Some(value.clone())
    }

    pub(super) fn resolve_deref_lvalue(
        &mut self,
        target: &AssignTarget,
    ) -> Option<(IrRegister, IrType)> {
        match target {
            // `@x = v` where x is a pointer variable stored in a stack slot.
            AssignTarget::Identifier(_) => {
                let (base_ptr_reg, base_ty) = self.resolve_assign_lvalue(target)?;
                let pointee_ty = match &base_ty {
                    IrType::Pointer(inner) => *inner.clone(),
                    _ => {
                        self.context.diagnostics.error(format!(
                            "cannot dereference assignment target `{}` of type `{}`",
                            self.format_assign_target(target),
                            base_ty
                        ));
                        return None;
                    }
                };

                let pointee_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: pointee_ptr_reg.clone(),
                    ty: base_ty,
                    ptr: base_ptr_reg,
                    offset: None,
                });

                Some((pointee_ptr_reg, pointee_ty))
            }
            // `@obj.field = v` / `@arr[idx] = v` resolve directly to the destination address.
            AssignTarget::FieldAccess { .. } | AssignTarget::ArrayIndex { .. } => {
                self.resolve_assign_lvalue(target)
            }
            // Chained dereference (e.g. @@pp = v)
            AssignTarget::Dereference(inner) => {
                let (base_ptr_reg, base_ty) = self.resolve_deref_lvalue(inner)?;
                let pointee_ty = match &base_ty {
                    IrType::Pointer(inner_ty) => *inner_ty.clone(),
                    _ => {
                        self.context.diagnostics.error(format!(
                            "cannot dereference assignment target `{}` of type `{}`",
                            self.format_assign_target(target),
                            base_ty
                        ));
                        return None;
                    }
                };

                let pointee_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: pointee_ptr_reg.clone(),
                    ty: base_ty,
                    ptr: base_ptr_reg,
                    offset: None,
                });

                Some((pointee_ptr_reg, pointee_ty))
            }
            AssignTarget::StructDestructure(_) => {
                self.context.diagnostics.error(format!(
                    "struct-destructure target `{}` is not supported for dereference assignment",
                    self.format_assign_target(target)
                ));
                None
            }
        }
    }

    pub(super) fn resolve_assign_lvalue(
        &mut self,
        target: &AssignTarget,
    ) -> Option<(IrRegister, IrType)> {
        match target {
            AssignTarget::Identifier(name) => {
                let ptr_info = self.context.symbols.lookup(name).cloned()?;
                let value_ty = match ptr_info.ty {
                    IrType::Pointer(inner) => *inner,
                    _ => {
                        self.context
                            .diagnostics
                            .error(format!("cannot assign to non-lvalue target `{name}`"));
                        return None;
                    }
                };
                let slot_ptr_reg = match ptr_info.value {
                    IrValue::Register(reg) => reg,
                    _ => {
                        self.context.diagnostics.error(format!(
                            "assignment target `{}` does not resolve to a register-backed lvalue",
                            self.format_assign_target(target)
                        ));
                        return None;
                    }
                };
                Some((slot_ptr_reg, value_ty))
            }
            AssignTarget::Dereference(inner) => {
                let (base_ptr_reg, base_value_ty) = self.resolve_assign_lvalue(inner)?;
                let next_pointee_ty = match &base_value_ty {
                    IrType::Pointer(inner_ty) => *inner_ty.clone(),
                    _ => {
                        self.context.diagnostics.error(format!(
                                "cannot dereference assignment target `{}` because resolved type is `{}`",
                                self.format_assign_target(inner),
                                base_value_ty
                            ));
                        return None;
                    }
                };
                let next_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: next_ptr_reg.clone(),
                    ty: base_value_ty,
                    ptr: base_ptr_reg,
                    offset: None,
                });
                Some((next_ptr_reg, next_pointee_ty))
            }
            AssignTarget::FieldAccess { expr, field } => {
                let (base_ptr_reg, base_value_ty) = self.resolve_assign_lvalue(expr)?;
                let resolved_base_ty = self.resolve_named_type(&base_value_ty);

                // Handle both direct aggregates and pointers to aggregates
                let (agg_ptr_reg, fields) = match &resolved_base_ty {
                    IrType::Aggregate(fields) => (base_ptr_reg, fields.clone()),
                    IrType::Pointer(inner) => match inner.as_ref() {
                        IrType::Aggregate(fields) => {
                            // Load the heap address from the stack slot first
                            let loaded_ptr = self.new_temp();
                            self.push_instruction(IrInstruction::Load {
                                dest: loaded_ptr.clone(),
                                ty: resolved_base_ty.clone(),
                                ptr: base_ptr_reg,
                                offset: None,
                            });
                            (loaded_ptr, fields.clone())
                        }
                        _ => {
                            self.context.diagnostics.error(format!(
                                    "field assignment target `{}` is not an aggregate (resolved base type: `{}`)",
                                    self.format_assign_target(target),
                                    resolved_base_ty
                                ));
                            return None;
                        }
                    },
                    _ => {
                        self.context.diagnostics.error(format!(
                                "field assignment target `{}` is not an aggregate (resolved base type: `{}`)",
                                self.format_assign_target(target),
                                resolved_base_ty
                            ));
                        return None;
                    }
                };

                let (offset, field_ty) = match self.aggregate_field_offset_and_type(&fields, field)
                {
                    Some(v) => v,
                    None => {
                        let known_fields = fields
                            .iter()
                            .map(|(name, _)| name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.context.diagnostics.error(format!(
                            "unknown field `{}` in assignment target `{}`. Known fields: [{}]",
                            field,
                            self.format_assign_target(target),
                            known_fields
                        ));
                        return None;
                    }
                };
                let field_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Offset {
                    dest: field_ptr_reg.clone(),
                    ty: field_ty.clone(),
                    ptr: agg_ptr_reg,
                    bytes: IrValue::Integer(offset),
                });
                Some((field_ptr_reg, field_ty))
            }
            AssignTarget::ArrayIndex { expr, index } => {
                let normalized_expr: &AssignTarget = match expr.as_ref() {
                    AssignTarget::Dereference(inner) => inner.as_ref(),
                    other => other,
                };
                let (base_ptr_reg, base_value_ty) = self.resolve_assign_lvalue(normalized_expr)?;
                let resolved_base_ty = self.resolve_named_type(&base_value_ty);

                // Handle both direct arrays and pointers to arrays/elements
                let (indexable_ptr_reg, element_ty) = match &resolved_base_ty {
                    IrType::Array { element, .. } => (base_ptr_reg, element.as_ref().clone()),
                    IrType::Pointer(element) => {
                        // Load the pointer value first
                        let loaded_ptr = self.new_temp();
                        self.push_instruction(IrInstruction::Load {
                            dest: loaded_ptr.clone(),
                            ty: resolved_base_ty.clone(),
                            ptr: base_ptr_reg,
                            offset: None,
                        });
                        (loaded_ptr, element.as_ref().clone())
                    }
                    _ => {
                        self.context.diagnostics.error(format!(
                                "array assignment target `{}` is not indexable (resolved base type: `{}`)",
                                self.format_assign_target(target),
                                resolved_base_ty
                            ));
                        return None;
                    }
                };

                let idx = self.lower_expression(index)?;
                let element_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Index {
                    dest: element_ptr_reg.clone(),
                    ty: element_ty.clone(),
                    base_ptr: indexable_ptr_reg,
                    idx: idx.value,
                });
                Some((element_ptr_reg, element_ty))
            }
            AssignTarget::StructDestructure(_) => {
                self.context.diagnostics.error(format!(
                    "struct-destructure assignment target `{}` is not supported in this lowering path",
                    self.format_assign_target(target)
                ));
                None
            }
        }
    }

    pub(super) fn lower_field_assign(
        &mut self,
        expr: &AssignTarget,
        field: &str,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        let target = AssignTarget::FieldAccess {
            expr: Box::new(expr.clone()),
            field: field.to_string(),
        };
        let (field_ptr_reg, _field_ty) = self.resolve_assign_lvalue(&target)?;
        self.push_instruction(IrInstruction::Store {
            ty: value.ty.clone(),
            value: value.value.clone(),
            ptr: field_ptr_reg,
            offset: None,
        });
        Some(value.clone())
    }

    pub(super) fn lower_array_index_assign(
        &mut self,
        expr: &AssignTarget,
        index: &Expression,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        let target = AssignTarget::ArrayIndex {
            expr: Box::new(expr.clone()),
            index: Box::new(index.clone()),
        };
        let (element_ptr_reg, _element_ty) = self.resolve_assign_lvalue(&target)?;
        self.push_instruction(IrInstruction::Store {
            ty: value.ty.clone(),
            value: value.value.clone(),
            ptr: element_ptr_reg,
            offset: None,
        });
        Some(value.clone())
    }
}
