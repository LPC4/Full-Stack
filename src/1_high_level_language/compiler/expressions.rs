use super::*;

impl HighLevelCompiler {
    pub(super) fn lower_expression(&mut self, expression: &Expression) -> Option<LoweredValue> {
        match expression {
            Expression::Primary(primary) => match primary {
                crate::high_level_language::ast::PrimaryExpr::Identifier(name) => {
                    let info = self.context.symbols.lookup(name).cloned();
                    if let Some(info) = info {
                        if let IrType::Pointer(ref inner_ty) = info.ty {
                            // Locals are stack slots (`T*`) in the symbol table; expressions use loaded `T`.
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
                                });
                            }
                        }

                        return Some(LoweredValue {
                            value: info.value,
                            ty: info.ty,
                        });
                    } else if let Some(const_val) = self.compile_time_consts.get(name) {
                        Some(self.lower_literal(const_val))
                    } else {
                        self.context
                            .diagnostics
                            .error(format!("unknown identifier `{name}`"));
                        None
                    }
                }
                crate::high_level_language::ast::PrimaryExpr::Literal(literal) => {
                    Some(self.lower_literal(literal))
                }
                crate::high_level_language::ast::PrimaryExpr::FieldAccess { expr, field } => {
                    // Parser commonly represents `@ptr.field` as `FieldAccess(expr = Dereference(ptr), ...)`.
                    // Lower from the pointer base directly so we don't try to load fields from aggregate values.
                    if let Expression::Unary {
                        op: UnaryOp::Dereference,
                        expr: inner,
                    } = &**expr
                    {
                        let base_ptr = self.lower_expression(inner)?;
                        return self.lower_field_access(&base_ptr, field);
                    }

                    let base = self.lower_expression(expr)?;
                    self.lower_field_access(&base, field)
                }
                crate::high_level_language::ast::PrimaryExpr::ArrayIndex { expr, index } => {
                    // Parser currently represents `@arr[i]` as `ArrayIndex(expr = Dereference(arr), ...)`.
                    // Normalize that form here so it lowers as "index through pointer then load value".
                    if let Expression::Unary {
                        op: UnaryOp::Dereference,
                        expr: inner,
                    } = &**expr
                    {
                        let base_ptr = self.lower_expression(inner)?;
                        let idx = self.lower_expression(index)?;
                        let element_ptr = self.lower_array_index(&base_ptr, &idx)?;
                        if let IrType::Pointer(ref element_ty) = element_ptr.ty {
                            if let IrValue::Register(ptr_reg) = element_ptr.value {
                                let dest = self.new_temp();
                                self.push_instruction(IrInstruction::Load {
                                    dest: dest.clone(),
                                    ty: *element_ty.clone(),
                                    ptr: ptr_reg,
                                    offset: None,
                                });
                                return Some(LoweredValue {
                                    value: IrValue::Register(dest),
                                    ty: *element_ty.clone(),
                                });
                            }
                        }
                        return Some(element_ptr);
                    }

                    let should_load_indexed_value = self.is_deref_based_index_expr(expr);
                    let base = match &**expr {
                        Expression::Unary {
                            op: UnaryOp::Dereference,
                            expr: inner,
                        } => self.lower_expression(inner)?,
                        _ => self.lower_expression(expr)?,
                    };
                    let idx = self.lower_expression(index)?;
                    let element_ptr = self.lower_array_index(&base, &idx)?;

                    if should_load_indexed_value {
                        if let (IrType::Pointer(element_ty), IrValue::Register(ptr_reg)) =
                            (&element_ptr.ty, &element_ptr.value)
                        {
                            let dest = self.new_temp();
                            self.push_instruction(IrInstruction::Load {
                                dest: dest.clone(),
                                ty: *element_ty.clone(),
                                ptr: ptr_reg.clone(),
                                offset: None,
                            });
                            return Some(LoweredValue {
                                value: IrValue::Register(dest),
                                ty: *element_ty.clone(),
                            });
                        }
                    }

                    Some(element_ptr)
                }
                crate::high_level_language::ast::PrimaryExpr::New { ty, args } => {
                    let dest = self.new_temp();
                    let lowered_ty = self.lower_type(ty);
                    let count = match args.len() {
                        0 => None,
                        1 => match &args[0] {
                            Expression::Primary(
                                crate::high_level_language::ast::PrimaryExpr::Literal(
                                    Literal::Integer(v),
                                ),
                            )
                            | Expression::Primary(
                                crate::high_level_language::ast::PrimaryExpr::Literal(
                                    Literal::HexInteger(v),
                                ),
                            ) if *v > 0 => Some(*v as usize),
                            other => {
                                self.context.diagnostics.error(format!(
                                        "new({}, count) requires a positive integer literal count; got `{}`",
                                        lowered_ty,
                                        self.format_expression(other)
                                    ));
                                return None;
                            }
                        },
                        n => {
                            self.context.diagnostics.error(format!(
                                "new({}, ...) expects at most one count argument, got {}",
                                lowered_ty, n
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
                    })
                }
                crate::high_level_language::ast::PrimaryExpr::FunctionCall { name, arguments } => {
                    let mut arg_values = Vec::new();
                    for arg in arguments {
                        if let Some(lowered) = self.lower_expression(arg) {
                            arg_values.push(lowered.value);
                        } else {
                            self.context
                                .diagnostics
                                .error(format!("failed to lower argument for call to {}", name));
                            return None;
                        }
                    }
                    let dest = self.new_temp();
                    self.push_instruction(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        function: name.clone(),
                        args: arg_values,
                    });

                    // Look up the function's return type
                    let return_ty = self
                        .function_return_types
                        .get(name)
                        .cloned()
                        .unwrap_or(IrType::Void);

                    Some(LoweredValue {
                        value: IrValue::Register(dest),
                        ty: return_ty,
                    })
                }
                crate::high_level_language::ast::PrimaryExpr::TupleLiteral(elements) => {
                    // Lower tuple literal similar to Expression::Tuple
                    let mut lowered_fields = Vec::new();
                    for elem in elements {
                        match self.lower_expression(elem) {
                            Some(lowered) => lowered_fields.push(lowered),
                            None => {
                                self.context
                                    .diagnostics
                                    .error("failed to lower tuple element".to_string());
                                return None;
                            }
                        }
                    }

                    // Create aggregate type and allocate space for tuple
                    let tuple_fields: Vec<(String, IrType)> = lowered_fields
                        .iter()
                        .enumerate()
                        .map(|(idx, f)| (idx.to_string(), f.ty.clone()))
                        .collect();
                    let tuple_ty = IrType::Aggregate(tuple_fields);
                    let dest = self.new_temp();

                    self.push_instruction(IrInstruction::Alloc {
                        dest: dest.clone(),
                        ty: tuple_ty.clone(),
                        count: None,
                    });

                    // Store each element in the tuple
                    let mut offset = 0i64;
                    for field in lowered_fields {
                        self.push_instruction(IrInstruction::Store {
                            ty: field.ty.clone(),
                            value: field.value,
                            ptr: dest.clone(),
                            offset: Some(offset),
                        });
                        offset += self.type_size_in_bytes(&field.ty) as i64;
                    }

                    Some(LoweredValue {
                        value: IrValue::Register(dest),
                        ty: tuple_ty,
                    })
                }
                crate::high_level_language::ast::PrimaryExpr::StructLiteral(fields) => {
                    // Lower struct literal
                    let mut lowered_fields = Vec::new();
                    for field_init in fields {
                        let field_value = self.lower_expression(&field_init.expr)?;
                        let field_name = field_init
                            .name
                            .clone()
                            .unwrap_or_else(|| format!("field_{}", lowered_fields.len()));
                        lowered_fields.push((field_name, field_value));
                    }

                    // Create aggregate type
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

                    // Store each field
                    let mut offset = 0i64;
                    for (_, field_value) in lowered_fields {
                        self.push_instruction(IrInstruction::Store {
                            ty: field_value.ty.clone(),
                            value: field_value.value,
                            ptr: dest.clone(),
                            offset: Some(offset),
                        });
                        // Use actual type size for offset calculation
                        offset += self.type_size_in_bytes(&field_value.ty) as i64;
                    }

                    Some(LoweredValue {
                        value: IrValue::Register(dest),
                        ty: struct_ty,
                    })
                }
                unsupported => {
                    self.context.diagnostics.error(format!(
                        "primary expression lowering not implemented: {unsupported:?}"
                    ));
                    None
                }
            },
            Expression::Binary { op, left, right } => {
                let lhs = self.lower_expression(left)?;
                let rhs = self.lower_expression(right)?;
                self.lower_binary(op, lhs, rhs)
            }
            Expression::Unary { op, expr } => {
                match op {
                    UnaryOp::AddressOf => {
                        // Address of specifically bypasses the load for identifiers
                        if let Expression::Primary(
                            crate::high_level_language::ast::PrimaryExpr::Identifier(name),
                        ) = &**expr
                        {
                            if let Some(info) = self.context.symbols.lookup(name) {
                                return Some(LoweredValue {
                                    value: info.value.clone(),
                                    ty: info.ty.clone(),
                                });
                            }
                        }
                        self.context
                            .diagnostics
                            .error(format!("address-of requires an identifier l-value"));
                        None
                    }
                    _ => {
                        let input = self.lower_expression(expr)?;
                        self.lower_unary(op, input)
                    }
                }
            }
            Expression::Assignment { target, rvalue } => {
                self.push_instruction(IrInstruction::Comment("assignment".to_string()));
                let lowered = self.lower_expression(rvalue)?;
                match &**target {
                    AssignTarget::Identifier(name) => {
                        let ptr_info = self.context.symbols.lookup(name)?;
                        if let IrType::Pointer(inner_ty) = &ptr_info.ty {
                            if let IrValue::Register(ptr_reg) = &ptr_info.value {
                                self.push_instruction(IrInstruction::Store {
                                    ty: *inner_ty.clone(),
                                    value: lowered.value.clone(),
                                    ptr: ptr_reg.clone(),
                                    offset: None,
                                });
                                return Some(lowered);
                            }
                        }
                        self.context
                            .diagnostics
                            .error(format!("cannot assign to non-pointer target `{name}`"));
                        None
                    }
                    AssignTarget::Dereference(target) => self.lower_deref_assign(target, &lowered),
                    AssignTarget::FieldAccess { expr, field } => {
                        self.lower_field_assign(expr, field, &lowered)
                    }
                    AssignTarget::ArrayIndex { expr, index } => {
                        self.lower_array_index_assign(expr, index, &lowered)
                    }
                    AssignTarget::Tuple(targets) => {
                        // Tuple destructuring: load each field from the aggregate and assign
                        self.lower_tuple_destructuring(targets, &lowered)
                    }
                }
            }
            Expression::Tuple(elements) => {
                let mut lowered_fields = Vec::new();
                for elem in elements {
                    match self.lower_expression(elem) {
                        Some(lowered) => lowered_fields.push(lowered),
                        None => {
                            self.context
                                .diagnostics
                                .error("failed to lower tuple element".to_string());
                            return None;
                        }
                    }
                }

                // Create aggregate type and allocate space for tuple
                let tuple_fields: Vec<(String, IrType)> = lowered_fields
                    .iter()
                    .enumerate()
                    .map(|(idx, f)| (idx.to_string(), f.ty.clone()))
                    .collect();
                let tuple_ty = IrType::Aggregate(tuple_fields);
                let dest = self.new_temp();

                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: tuple_ty.clone(),
                    count: None,
                });

                // Store each element in the tuple
                let mut offset = 0i64;
                for field in lowered_fields {
                    self.push_instruction(IrInstruction::Store {
                        ty: field.ty.clone(),
                        value: field.value,
                        ptr: dest.clone(),
                        offset: Some(offset),
                    });
                    // Use actual type size for offset calculation
                    offset += self.type_size_in_bytes(&field.ty) as i64;
                }

                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: tuple_ty,
                })
            }
        }
    }

    pub(super) fn lower_literal(&self, literal: &Literal) -> LoweredValue {
        match literal {
            Literal::Integer(value) | Literal::HexInteger(value) => LoweredValue {
                value: IrValue::Integer(*value),
                ty: IrType::Integer(IntWidth::I32),
            },
            Literal::Float(value) => LoweredValue {
                value: IrValue::Float(*value),
                ty: IrType::Float(FloatWidth::F64),
            },
            Literal::Boolean(value) => LoweredValue {
                value: IrValue::Bool(*value),
                ty: IrType::Integer(IntWidth::I1),
            },
            Literal::Null => LoweredValue {
                value: IrValue::Null,
                ty: IrType::Pointer(Box::new(IrType::Named("unknown".to_owned()))),
            },
            Literal::StringLit(text) => LoweredValue {
                value: IrValue::Register(IrRegister::Named(format!("str_{}", text.len()))),
                ty: IrType::Named("Str".to_owned()),
            },
        }
    }

    pub(super) fn lower_binary(
        &mut self,
        op: &BinaryOp,
        lhs: LoweredValue,
        rhs: LoweredValue,
    ) -> Option<LoweredValue> {
        let dest = self.new_temp();
        match op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::And
            | BinaryOp::Or => {
                let ir_op = match op {
                    BinaryOp::Add => IrMathOp::Add,
                    BinaryOp::Sub => IrMathOp::Sub,
                    BinaryOp::Mul => IrMathOp::Mul,
                    BinaryOp::Div => IrMathOp::SDiv,
                    BinaryOp::Mod => IrMathOp::Mod,
                    BinaryOp::And => IrMathOp::And,
                    BinaryOp::Or => IrMathOp::Or,
                    _ => unreachable!(),
                };
                self.push_instruction(IrInstruction::Math {
                    dest: dest.clone(),
                    op: ir_op,
                    ty: lhs.ty.clone(),
                    lhs: lhs.value,
                    rhs: rhs.value,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: lhs.ty,
                })
            }
            BinaryOp::Eq
            | BinaryOp::Neq
            | BinaryOp::Lt
            | BinaryOp::Lte
            | BinaryOp::Gt
            | BinaryOp::Gte => {
                let cmp = match op {
                    BinaryOp::Eq => IrCmpOp::Eq,
                    BinaryOp::Neq => IrCmpOp::Ne,
                    BinaryOp::Lt => IrCmpOp::Slt,
                    BinaryOp::Lte => IrCmpOp::Sle,
                    BinaryOp::Gt => IrCmpOp::Sgt,
                    BinaryOp::Gte => IrCmpOp::Sge,
                    _ => unreachable!(),
                };
                self.push_instruction(IrInstruction::Cmp {
                    dest: dest.clone(),
                    op: cmp,
                    ty: lhs.ty,
                    lhs: lhs.value,
                    rhs: rhs.value,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: IrType::Integer(IntWidth::I1),
                })
            }
        }
    }

    pub(super) fn lower_unary(
        &mut self,
        op: &UnaryOp,
        input: LoweredValue,
    ) -> Option<LoweredValue> {
        match op {
            UnaryOp::Negate | UnaryOp::Not => {
                let dest = self.new_temp();
                let ir_op = match op {
                    UnaryOp::Negate => IrUnaryOp::Neg,
                    UnaryOp::Not => IrUnaryOp::Not,
                    _ => unreachable!(),
                };
                self.push_instruction(IrInstruction::Unary {
                    dest: dest.clone(),
                    op: ir_op,
                    ty: input.ty.clone(),
                    value: input.value,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: input.ty,
                })
            }
            UnaryOp::Dereference => {
                let pointee_ty = match &input.ty {
                    IrType::Pointer(inner) => *inner.clone(),
                    other => {
                        self.context.diagnostics.error(format!(
                            "cannot dereference expression of non-pointer type `{}`",
                            other
                        ));
                        return None;
                    }
                };

                let dest = self.new_temp();
                let ptr_reg = match input.value {
                    IrValue::Register(reg) => reg,
                    _ => {
                        self.context
                            .diagnostics
                            .error("cannot dereference non-register value".to_string());
                        return None;
                    }
                };
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: pointee_ty.clone(),
                    ptr: ptr_reg,
                    offset: None,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: pointee_ty,
                })
            }
            UnaryOp::AddressOf => match input.value {
                IrValue::Register(reg) => Some(LoweredValue {
                    value: IrValue::Register(reg),
                    ty: IrType::Pointer(Box::new(input.ty)),
                }),
                _ => {
                    self.context
                        .diagnostics
                        .error("cannot take address of non-register".to_string());
                    None
                }
            },
        }
    }
}
