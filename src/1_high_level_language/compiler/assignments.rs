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
                    // Multiply index by element size to get byte offset
                    let size = self.type_size_in_bytes(element);
                    let byte_offset_reg = self.new_temp();
                    self.push_instruction(IrInstruction::Math {
                        dest: byte_offset_reg.clone(),
                        op: IrMathOp::Mul,
                        ty: IrType::Integer(IntWidth::I64),
                        lhs: index.value.clone(),
                        rhs: IrValue::Integer(size as i64),
                    });

                    let dest = self.new_temp();
                    self.push_instruction(IrInstruction::Offset {
                        dest: dest.clone(),
                        ty: *element.clone(),
                        ptr: ptr_reg.clone(),
                        bytes: IrValue::Register(byte_offset_reg),
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
            AssignTarget::Tuple(_) => {
                self.context.diagnostics.error(format!(
                    "tuple target `{}` is not supported for dereference assignment",
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
                // Multiply index by element size
                let size = self.type_size_in_bytes(&element_ty);
                let byte_offset_reg = self.new_temp();
                self.push_instruction(IrInstruction::Math {
                    dest: byte_offset_reg.clone(),
                    op: IrMathOp::Mul,
                    ty: IrType::Integer(IntWidth::I64),
                    lhs: idx.value,
                    rhs: IrValue::Integer(size as i64),
                });

                let element_ptr_reg = self.new_temp();
                self.push_instruction(IrInstruction::Offset {
                    dest: element_ptr_reg.clone(),
                    ty: element_ty.clone(),
                    ptr: indexable_ptr_reg,
                    bytes: IrValue::Register(byte_offset_reg),
                });
                Some((element_ptr_reg, element_ty))
            }
            AssignTarget::Tuple(_) => {
                self.context.diagnostics.error(format!(
                    "tuple assignment target `{}` is not supported in this lowering path",
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

    pub(super) fn lower_tuple_destructuring(
        &mut self,
        targets: &[AssignTarget],
        tuple_value: &LoweredValue,
    ) -> Option<LoweredValue> {
        // Tuple destructuring: extract each field from the aggregate value
        // and assign to corresponding target

        // Extract field types from the aggregate
        let field_types: Vec<IrType> = match &tuple_value.ty {
            IrType::Aggregate(fields) => fields.iter().map(|(_name, ty)| ty.clone()).collect(),
            _ => {
                self.context
                    .diagnostics
                    .error("tuple destructuring requires aggregate type".to_string());
                return None;
            }
        };

        // Check that target count matches field count
        if targets.len() != field_types.len() {
            self.context.diagnostics.error(format!(
                "tuple destructuring: expected {} targets, got {}",
                field_types.len(),
                targets.len()
            ));
            return None;
        }

        // The tuple_value.value should be a register pointing to the aggregate
        let tuple_ptr = match &tuple_value.value {
            IrValue::Register(reg) => reg.clone(),
            _ => {
                self.context
                    .diagnostics
                    .error("tuple destructuring requires register value".to_string());
                return None;
            }
        };

        // For each target, load the corresponding field and assign
        let mut offset = 0i64;
        for (target, field_ty) in targets.iter().zip(field_types.iter()) {
            // Load field value from tuple
            let field_reg = self.new_temp();
            self.push_instruction(IrInstruction::Load {
                dest: field_reg.clone(),
                ty: field_ty.clone(),
                ptr: tuple_ptr.clone(),
                offset: Some(offset),
            });

            // Create a LoweredValue for this field
            let field_value = LoweredValue {
                value: IrValue::Register(field_reg),
                ty: field_ty.clone(),
            };

            // Recursively assign to the target
            self.lower_assign_target(target, &field_value)?;

            // Use actual type size for offset calculation
            offset += self.type_size_in_bytes(field_ty) as i64;
        }

        // Return the tuple value
        Some(tuple_value.clone())
    }

    pub(super) fn lower_assign_target(
        &mut self,
        target: &AssignTarget,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        // Helper to assign a single value to a target
        match target {
            AssignTarget::Identifier(name) => {
                // Check if variable exists; if not, declare it as a new local
                let (ptr_type, ptr_reg) = if let Some(info) = self.context.symbols.lookup(name) {
                    (info.ty.clone(), info.value.clone())
                } else {
                    // Variable doesn't exist - declare it as a new local variable
                    let lowered_ty = value.ty.clone();
                    let ptr_reg = IrRegister::Named(name.clone());

                    self.push_instruction(IrInstruction::Comment(format!(
                        "local var (from tuple destructure): {}",
                        name
                    )));
                    self.push_instruction(IrInstruction::Alloc {
                        dest: ptr_reg.clone(),
                        ty: lowered_ty.clone(),
                        count: None,
                    });

                    let ptr_type = IrType::Pointer(Box::new(lowered_ty));
                    self.context.symbols.insert(
                        name.clone(),
                        ptr_type.clone(),
                        IrValue::Register(ptr_reg.clone()),
                    );

                    (ptr_type, IrValue::Register(ptr_reg))
                };

                if let IrType::Pointer(inner_ty) = &ptr_type {
                    if let IrValue::Register(ptr_reg) = &ptr_reg {
                        self.push_instruction(IrInstruction::Store {
                            ty: *inner_ty.clone(),
                            value: value.value.clone(),
                            ptr: ptr_reg.clone(),
                            offset: None,
                        });
                        return Some(value.clone());
                    }
                }
                self.context
                    .diagnostics
                    .error(format!("cannot assign to non-pointer target `{name}`"));
                None
            }
            AssignTarget::Dereference(target) => self.lower_deref_assign(target, value),
            AssignTarget::FieldAccess { expr, field } => {
                self.lower_field_assign(expr, field, value)
            }
            AssignTarget::ArrayIndex { expr, index } => {
                self.lower_array_index_assign(expr, index, value)
            }
            AssignTarget::Tuple(_) => {
                self.context
                    .diagnostics
                    .error("nested tuple destructuring not supported".to_string());
                None
            }
        }
    }
}
