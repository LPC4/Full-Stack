use super::{
    AssignTarget, EvalMode, Expression, HighLevelCompiler, IrInstruction, IrType, IrValue, Literal,
    LoweredValue, UnaryOp,
};
use crate::ast::{PrimaryExpr, Type};
use crate::ir::{FloatWidth, IntWidth, IrCastMode};

impl HighLevelCompiler {
    /// Evaluate expression in Value mode (rvalue context).
    pub(super) fn lower_expression(&mut self, expression: &Expression) -> Option<LoweredValue> {
        self.lower_expr(expression, EvalMode::Value)
    }

    /// Unified expression lowering.
    ///
    /// `EvalMode::Value` produces the loaded/computed value.
    /// `EvalMode::Address` produces a pointer (`Pointer(T)`) to the storage location.
    pub(super) fn lower_expr(
        &mut self,
        expression: &Expression,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        match expression {
            Expression::Primary(primary) => self.lower_primary(primary, mode),
            Expression::Binary { op, left, right } => {
                let lhs = self.lower_expr(left, EvalMode::Value)?;
                let rhs = self.lower_expr(right, EvalMode::Value)?;
                self.lower_binary(op, lhs, rhs)
            }
            Expression::Unary { op, expr } => self.lower_unary_expr(op, expr, mode),
            Expression::Cast { target_ty, expr } => self.lower_cast(target_ty, expr),
            Expression::Assignment { target, rvalue } => self.lower_assignment(target, rvalue),
        }
    }

    fn lower_primary(&mut self, primary: &PrimaryExpr, mode: EvalMode) -> Option<LoweredValue> {
        match primary {
            PrimaryExpr::Identifier(name) => self.lower_identifier(name, mode),
            PrimaryExpr::Literal(literal) => Some(self.lower_literal(literal)),
            PrimaryExpr::Grouped(expr) => self.lower_expr(expr, mode),
            PrimaryExpr::FieldAccess { expr, field } => {
                self.lower_field_access_expr(expr, field, mode)
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                self.lower_array_index_expr(expr, index, mode)
            }
            PrimaryExpr::New { ty, args } => {
                use crate::ir::IrInstruction;
                let dest = self.new_temp();
                let lowered_ty = self.lower_type(ty);

                if matches!(lowered_ty, IrType::Array { .. }) {
                    self.context.error(format!(
                        "new([N]T) syntax is removed; use new(T, N) instead (e.g. new({}, N))",
                        match &lowered_ty {
                            IrType::Array { element, .. } => element.as_ref().clone(),
                            other => other.clone(),
                        }
                    ));
                    return None;
                }

                let count = match args.len() {
                    0 => None,
                    1 => {
                        let lowered_count = self.lower_expr(&args[0], EvalMode::Value)?;
                        Some(lowered_count.value)
                    }
                    n => {
                        self.context.error(format!(
                            "new({lowered_ty}, ...) expects at most one count argument, got {n}"
                        ));
                        return None;
                    }
                };
                self.push_instruction(IrInstruction::HeapAlloc {
                    dest: dest.clone(),
                    ty: lowered_ty.clone(),
                    count,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: IrType::Pointer(Box::new(lowered_ty)),
                    is_unsigned: false,
                })
            }
            PrimaryExpr::AsmReg { reg } => {
                if mode == EvalMode::Address {
                    return None;
                }
                use crate::ir::IrInstruction;
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::ReadReg {
                    dest: dest.clone(),
                    reg: reg.clone(),
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: IrType::Integer(IntWidth::I64),
                    is_unsigned: false,
                })
            }
            PrimaryExpr::FunctionCall { name, arguments } => {
                // Function call results are rvalues with no addressable storage.
                if mode == EvalMode::Address {
                    return None;
                }
                if name == "free" {
                    if arguments.len() != 1 {
                        self.context
                            .diagnostics
                            .error("free() expects exactly one argument".to_owned());
                        return None;
                    }
                    let arg = self.lower_expr(&arguments[0], EvalMode::Value)?;
                    match arg.value {
                        IrValue::Register(ptr_reg) => {
                            self.push_instruction(IrInstruction::HeapFree { ptr: ptr_reg });
                        }
                        IrValue::Null => {}
                        _ => {
                            self.context
                                .diagnostics
                                .error("free() argument must be a pointer value".to_owned());
                            return None;
                        }
                    }
                    return Some(LoweredValue {
                        value: IrValue::Null,
                        ty: IrType::Void,
                        is_unsigned: false,
                    });
                }

                let mut arg_values = Vec::new();
                for arg in arguments {
                    if let Some(lowered) = self.lower_expr(arg, EvalMode::Value) {
                        arg_values.push(lowered.value);
                    } else {
                        self.context
                            .diagnostics
                            .error(format!("failed to lower argument for call to {name}"));
                        return None;
                    }
                }

                let return_ty = self
                    .function_return_types
                    .get(name)
                    .cloned()
                    .unwrap_or(IrType::Void);

                let dest = if return_ty == IrType::Void {
                    None
                } else {
                    Some(self.new_temp())
                };

                self.push_instruction(IrInstruction::Call {
                    dest: dest.clone(),
                    function: name.clone(),
                    args: arg_values,
                });

                Some(LoweredValue {
                    value: dest.map(IrValue::Register).unwrap_or(IrValue::Null),
                    ty: return_ty,
                    is_unsigned: false,
                })
            }
            PrimaryExpr::ArrayLiteral(elements) => self.lower_array_literal(elements),
            PrimaryExpr::StructLiteral(fields) => {
                let mut lowered_fields = Vec::new();
                for field_init in fields {
                    let field_value = self.lower_expr(&field_init.expr, EvalMode::Value)?;
                    let expected_ty = field_init
                        .ty
                        .as_ref()
                        .map(|ty| self.lower_type(ty))
                        .unwrap_or_else(|| field_value.ty.clone());
                    let expected_resolved = self.resolve_named_type(&expected_ty);
                    let actual_resolved = self.resolve_named_type(&field_value.ty);
                    if actual_resolved != expected_resolved {
                        let declared_ty = field_init
                            .ty
                            .as_ref()
                            .map(|ty| self.lower_type(ty).to_string())
                            .unwrap_or_else(|| field_value.ty.to_string());
                        self.context.error(format!(
                            "struct literal field `{}` type mismatch: declared `{}`, got `{}`",
                            field_init.name, declared_ty, field_value.ty
                        ));
                        return None;
                    }
                    lowered_fields.push((field_init.name.clone(), field_value));
                }

                let struct_fields: Vec<(String, IrType)> = lowered_fields
                    .iter()
                    .map(|(name, val)| (name.clone(), val.ty.clone()))
                    .collect();
                let struct_ty = IrType::Aggregate(struct_fields);
                let dest = self.new_temp();

                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: struct_ty.clone(),
                    count: None,
                });

                let mut offset = 0i64;
                for (_, field_value) in lowered_fields {
                    let align = self.type_alignment_in_bytes(&field_value.ty) as i64;
                    offset = Self::align_to(offset, align);
                    self.push_instruction(IrInstruction::Store {
                        ty: field_value.ty.clone(),
                        value: field_value.value,
                        ptr: dest.clone(),
                        offset: Some(offset),
                    });
                    offset += self.type_size_in_bytes(&field_value.ty) as i64;
                }

                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: struct_ty,
                    is_unsigned: false,
                })
            }
        }
    }

    fn lower_identifier(&mut self, name: &str, mode: EvalMode) -> Option<LoweredValue> {
        let info = self.context.symbols.lookup(name).cloned();
        if let Some(info) = info {
            match mode {
                EvalMode::Address => {
                    // Locals are stored as Pointer(T) stack slots; return the slot pointer directly.
                    Some(LoweredValue {
                        value: info.value,
                        ty: info.ty,
                        is_unsigned: false,
                    })
                }
                EvalMode::Value => {
                    if let IrType::Pointer(inner_ty) = &info.ty {
                        let dest = self.new_temp();
                        if let IrValue::Register(ptr_reg) = info.value {
                            self.push_instruction(IrInstruction::Load {
                                dest: dest.clone(),
                                ty: *inner_ty.clone(),
                                ptr: ptr_reg,
                                offset: None,
                            });
                            return Some(LoweredValue {
                                value: IrValue::Register(dest),
                                ty: *inner_ty.clone(),
                                is_unsigned: self.context.unsigned_vars.contains(name),
                            });
                        }
                    }
                    Some(LoweredValue {
                        value: info.value,
                        ty: info.ty,
                        is_unsigned: false,
                    })
                }
            }
        } else if let Some(const_val) = self.compile_time_consts.get(name).cloned() {
            Some(self.lower_literal(&const_val))
        } else if let Some(gv_ty) = self.global_vars.get(name).cloned() {
            // Global variable: emit `la dest, name` to load its address.
            let addr_reg = self.new_temp();
            self.push_instruction(IrInstruction::GlobalRef {
                dest: addr_reg.clone(),
                name: name.to_owned(),
            });
            match mode {
                super::EvalMode::Address => Some(LoweredValue {
                    value: IrValue::Register(addr_reg),
                    ty: IrType::Pointer(Box::new(gv_ty)),
                    is_unsigned: false,
                }),
                super::EvalMode::Value => {
                    let val_reg = self.new_temp();
                    self.push_instruction(IrInstruction::Load {
                        dest: val_reg.clone(),
                        ty: gv_ty.clone(),
                        ptr: addr_reg,
                        offset: None,
                    });
                    Some(LoweredValue {
                        value: IrValue::Register(val_reg),
                        ty: gv_ty,
                        is_unsigned: false,
                    })
                }
            }
        } else {
            self.context
                .diagnostics
                .error(format!("unknown identifier `{name}`"));
            None
        }
    }

    fn lower_field_access_expr(
        &mut self,
        expr: &Expression,
        field: &str,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        // `@ptr.field`: evaluate the inner pointer in Value mode -- its register is the base pointer.
        // `x.field`: evaluate the base in Address mode -- the slot pointer is the aggregate pointer.
        let base_addr = match expr {
            Expression::Unary {
                op: UnaryOp::Dereference,
                expr: inner,
            } => self.lower_expr(inner, EvalMode::Value)?,
            _ => self.lower_expr(expr, EvalMode::Address)?,
        };
        self.lower_field_access_mode(&base_addr, field, mode)
    }

    pub(super) fn lower_field_access_mode(
        &mut self,
        base_addr: &LoweredValue,
        field: &str,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        let resolved_ty = self.resolve_named_type(&base_addr.ty);

        // Resolve the aggregate pointer and field list from the base type.
        // Three shapes are valid:
        //   Aggregate(fields)           -- base_addr.value is the pointer to the aggregate
        //   Pointer(Aggregate(fields))  -- base_addr.value is the pointer to the aggregate
        //   Pointer(Pointer(Aggregate)) -- load from base_addr.value first (heap-ptr in stack slot)
        let (agg_ptr, fields) = match resolved_ty {
            IrType::Aggregate(fields) => {
                let reg = match &base_addr.value {
                    IrValue::Register(r) => r.clone(),
                    _ => return None,
                };
                (reg, fields)
            }
            IrType::Pointer(inner) => {
                let inner_resolved = self.resolve_named_type(&inner);
                match inner_resolved {
                    IrType::Aggregate(fields) => {
                        let reg = match &base_addr.value {
                            IrValue::Register(r) => r.clone(),
                            _ => return None,
                        };
                        (reg, fields)
                    }
                    IrType::Pointer(inner_inner) => {
                        let inner_inner_resolved = self.resolve_named_type(&inner_inner);
                        if let IrType::Aggregate(fields) = inner_inner_resolved {
                            let slot_reg = match &base_addr.value {
                                IrValue::Register(r) => r.clone(),
                                _ => return None,
                            };
                            let loaded = self.new_temp();
                            self.push_instruction(IrInstruction::Load {
                                dest: loaded.clone(),
                                ty: *inner.clone(),
                                ptr: slot_reg,
                                offset: None,
                            });
                            // Field access yields *field_T; the caller must use @ to load the value.
                            let (offset, field_ty) =
                                self.aggregate_field_offset_and_type(&fields, field)?;
                            let dest = self.new_temp();
                            self.push_instruction(IrInstruction::Offset {
                                dest: dest.clone(),
                                ty: field_ty.clone(),
                                ptr: loaded,
                                bytes: IrValue::Integer(offset),
                            });
                            return Some(LoweredValue {
                                value: IrValue::Register(dest),
                                ty: IrType::Pointer(Box::new(field_ty)),
                                is_unsigned: false,
                            });
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };

        let (offset, field_ty) = self.aggregate_field_offset_and_type(&fields, field)?;

        match mode {
            EvalMode::Value => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Comment(format!(
                    "Access field '{field}' at offset {offset}"
                )));
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: field_ty.clone(),
                    ptr: agg_ptr,
                    offset: Some(offset),
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: field_ty,
                    is_unsigned: false,
                })
            }
            EvalMode::Address => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Comment(format!(
                    "Address of field '{field}' at offset {offset}"
                )));
                self.push_instruction(IrInstruction::Offset {
                    dest: dest.clone(),
                    ty: field_ty.clone(),
                    ptr: agg_ptr,
                    bytes: IrValue::Integer(offset),
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: IrType::Pointer(Box::new(field_ty)),
                    is_unsigned: false,
                })
            }
        }
    }

    fn lower_array_index_expr(
        &mut self,
        expr: &Expression,
        index: &Expression,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        match expr {
            Expression::Unary {
                op: UnaryOp::Dereference,
                expr: inner,
            } => {
                // `@arr[i]`: evaluate arr in Value mode to get the heap pointer.
                let base = self.lower_expr(inner, EvalMode::Value)?;
                let idx = self.lower_expr(index, EvalMode::Value)?;
                self.lower_array_index_mode(&base, &idx, mode)
            }
            _ => {
                // `arr[i]`: always returns the element pointer (Address).
                // Callers must apply `@` to load the value.
                let base = self.lower_expr(expr, EvalMode::Address)?;
                let idx = self.lower_expr(index, EvalMode::Value)?;
                self.lower_array_index_mode(&base, &idx, EvalMode::Address)
            }
        }
    }

    pub(super) fn lower_array_index_mode(
        &mut self,
        base: &LoweredValue,
        index: &LoweredValue,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        let resolved_ty = self.resolve_named_type(&base.ty);

        // Find the pointer that addresses the first element and the element type.
        let (indexable_ptr, element_ty) = match resolved_ty {
            // Direct array (pointer to array storage)
            IrType::Array { element, .. } => {
                let reg = match &base.value {
                    IrValue::Register(r) => r.clone(),
                    _ => return None,
                };
                (reg, *element)
            }
            IrType::Pointer(inner) => {
                let inner_resolved = self.resolve_named_type(&inner);
                match inner_resolved {
                    // Pointer to Array: base.value points directly to the array data.
                    IrType::Array { element, .. } => {
                        let reg = match &base.value {
                            IrValue::Register(r) => r.clone(),
                            _ => return None,
                        };
                        (reg, *element)
                    }
                    // Pointer to Pointer: load the inner pointer first.
                    IrType::Pointer(inner_inner) => {
                        let inner_inner_resolved = self.resolve_named_type(&inner_inner);
                        let element_ty = match inner_inner_resolved {
                            IrType::Array { element, .. } => *element,
                            other => other,
                        };
                        let slot_reg = match &base.value {
                            IrValue::Register(r) => r.clone(),
                            _ => return None,
                        };
                        let loaded = self.new_temp();
                        self.push_instruction(IrInstruction::Load {
                            dest: loaded.clone(),
                            ty: *inner.clone(),
                            ptr: slot_reg,
                            offset: None,
                        });
                        (loaded, element_ty)
                    }
                    // Pointer to T: base.value is a pointer to elements of type T.
                    other => {
                        let reg = match &base.value {
                            IrValue::Register(r) => r.clone(),
                            _ => return None,
                        };
                        (reg, other)
                    }
                }
            }
            _ => return None,
        };

        let elem_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Comment(format!(
            "Compute array element address at index ${}",
            index.value
        )));
        self.push_instruction(IrInstruction::Index {
            dest: elem_ptr.clone(),
            ty: element_ty.clone(),
            base_ptr: indexable_ptr,
            idx: index.value.clone(),
        });

        match mode {
            EvalMode::Address => Some(LoweredValue {
                value: IrValue::Register(elem_ptr),
                ty: IrType::Pointer(Box::new(element_ty)),
                is_unsigned: false,
            }),
            EvalMode::Value => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: element_ty.clone(),
                    ptr: elem_ptr,
                    offset: None,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: element_ty,
                    is_unsigned: false,
                })
            }
        }
    }

    fn lower_unary_expr(
        &mut self,
        op: &UnaryOp,
        expr: &Expression,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        match op {
            UnaryOp::AddressOf => {
                if matches!(
                    expr,
                    Expression::Unary {
                        op: UnaryOp::Dereference,
                        ..
                    }
                ) {
                    self.context.error(
                        "cannot take address of a dereference expression (`&@...` is invalid)"
                            .to_owned(),
                    );
                    return None;
                }
                // `&x`: evaluate x in Address mode; the result is already a Pointer(T).
                self.lower_expr(expr, EvalMode::Address)
            }
            UnaryOp::Dereference => {
                let ptr_val = self.lower_expr(expr, EvalMode::Value)?;
                let pointee_ty = match &ptr_val.ty {
                    IrType::Pointer(inner) => *inner.clone(),
                    other => {
                        self.context.error(format!(
                            "cannot dereference expression of non-pointer type `{other}`"
                        ));
                        return None;
                    }
                };
                match mode {
                    EvalMode::Address => {
                        // `@x` in Address mode: the pointer value is the address of the pointee.
                        Some(LoweredValue {
                            value: ptr_val.value,
                            ty: ptr_val.ty,
                            is_unsigned: false,
                        })
                    }
                    EvalMode::Value => {
                        let ptr_reg = match ptr_val.value {
                            IrValue::Register(r) => r,
                            _ => {
                                self.context
                                    .diagnostics
                                    .error("cannot dereference non-register value".to_owned());
                                return None;
                            }
                        };
                        let dest = self.new_temp();
                        self.push_instruction(IrInstruction::Load {
                            dest: dest.clone(),
                            ty: pointee_ty.clone(),
                            ptr: ptr_reg,
                            offset: None,
                        });
                        Some(LoweredValue {
                            value: IrValue::Register(dest),
                            ty: pointee_ty,
                            is_unsigned: false,
                        })
                    }
                }
            }
            _ => {
                let input = self.lower_expr(expr, EvalMode::Value)?;
                self.lower_unary(op, input)
            }
        }
    }

    fn lower_cast(&mut self, target_ty: &Type, expr: &Expression) -> Option<LoweredValue> {
        let source_value = self.lower_expr(expr, EvalMode::Value)?;
        let target_ir = self.lower_type(target_ty);

        let source_resolved = self.resolve_named_type(&source_value.ty);
        let target_resolved = self.resolve_named_type(&target_ir);

        let cast_mode = match (&source_resolved, &target_resolved) {
            (IrType::Integer(src_width), IrType::Integer(tgt_width)) => {
                let src_bits = match src_width {
                    IntWidth::I1 => 1,
                    IntWidth::I8 => 8,
                    IntWidth::I16 => 16,
                    IntWidth::I32 => 32,
                    IntWidth::I64 => 64,
                };
                let tgt_bits = match tgt_width {
                    IntWidth::I1 => 1,
                    IntWidth::I8 => 8,
                    IntWidth::I16 => 16,
                    IntWidth::I32 => 32,
                    IntWidth::I64 => 64,
                };
                if tgt_bits < src_bits {
                    IrCastMode::Trunc
                } else if source_value.is_unsigned {
                    IrCastMode::Zext
                } else {
                    IrCastMode::Sext
                }
            }
            (IrType::Float(_), IrType::Integer(_)) => IrCastMode::F2i,
            (IrType::Integer(_), IrType::Float(_)) => IrCastMode::I2f,
            (IrType::Float(_), IrType::Float(_)) => IrCastMode::Bitcast,
            (IrType::Pointer(_), IrType::Pointer(_)) => IrCastMode::Bitcast,
            (IrType::Pointer(_), IrType::Integer(_)) => IrCastMode::Bitcast,
            (IrType::Integer(_), IrType::Pointer(_)) => IrCastMode::Bitcast,
            _ if source_resolved == target_resolved => return Some(source_value),
            _ => {
                self.context.error(format!(
                    "Unsupported cast from `{}` to `{}`",
                    source_value.ty, target_ir
                ));
                return None;
            }
        };

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Cast {
            dest: dest.clone(),
            mode: cast_mode,
            value: source_value.value,
            ty: target_ir.clone(),
        });
        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: target_ir,
            is_unsigned: source_value.is_unsigned,
        })
    }

    fn lower_assignment(
        &mut self,
        target: &AssignTarget,
        rvalue: &Expression,
    ) -> Option<LoweredValue> {
        self.push_instruction(IrInstruction::Comment("assignment".to_owned()));

        // Struct destructuring does not have a single lvalue address.
        if let AssignTarget::StructDestructure(fields) = target {
            // Try Address mode first (works for locals, field accesses, etc.).
            if let Some(addr) = self.lower_expr(rvalue, EvalMode::Address) {
                return self.lower_struct_destructuring_from_addr(fields, &addr);
            }
            // Fallback: spill the rvalue to the stack. Evaluate exactly once here.
            let lowered = self.lower_expr(rvalue, EvalMode::Value)?;
            let spill = self.new_temp();
            self.push_instruction(IrInstruction::Alloc {
                dest: spill.clone(),
                ty: lowered.ty.clone(),
                count: None,
            });
            self.push_instruction(IrInstruction::Store {
                ty: lowered.ty.clone(),
                value: lowered.value,
                ptr: spill.clone(),
                offset: None,
            });
            let addr = LoweredValue {
                value: IrValue::Register(spill),
                ty: IrType::Pointer(Box::new(lowered.ty)),
                is_unsigned: false,
            };
            return self.lower_struct_destructuring_from_addr(fields, &addr);
        }

        let lowered = self.lower_expr(rvalue, EvalMode::Value)?;
        let target_expr = Self::assign_target_to_expression(target)?;
        let addr = self.lower_expr(&target_expr, EvalMode::Address)?;

        let (ptr_reg, store_ty) = match (&addr.value, &addr.ty) {
            (IrValue::Register(reg), IrType::Pointer(inner)) => (reg.clone(), *inner.clone()),
            _ => {
                self.context
                    .diagnostics
                    .error("assignment target did not resolve to a register address".to_owned());
                return None;
            }
        };

        self.push_instruction(IrInstruction::Store {
            ty: store_ty,
            value: lowered.value.clone(),
            ptr: ptr_reg,
            offset: None,
        });
        Some(lowered)
    }

    /// Convert an `AssignTarget` back into an `Expression` so it can be lowered with `lower_expr`.
    pub(super) fn assign_target_to_expression(target: &AssignTarget) -> Option<Expression> {
        match target {
            AssignTarget::Identifier(name) => {
                Some(Expression::Primary(PrimaryExpr::Identifier(name.clone())))
            }
            AssignTarget::Dereference(inner) => {
                let inner_expr = Self::assign_target_to_expression(inner)?;
                Some(Expression::Unary {
                    op: UnaryOp::Dereference,
                    expr: Box::new(inner_expr),
                })
            }
            AssignTarget::FieldAccess { expr, field } => {
                let base_expr = Self::assign_target_to_expression(expr)?;
                Some(Expression::Primary(PrimaryExpr::FieldAccess {
                    expr: Box::new(base_expr),
                    field: field.clone(),
                }))
            }
            AssignTarget::ArrayIndex { expr, index } => {
                let base_expr = Self::assign_target_to_expression(expr)?;
                Some(Expression::Primary(PrimaryExpr::ArrayIndex {
                    expr: Box::new(base_expr),
                    index: index.clone(),
                }))
            }
            AssignTarget::StructDestructure(_) => None,
        }
    }
}
