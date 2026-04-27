use super::{
    AssignTarget, BinaryOp, Expression, FloatWidth, HighLevelCompiler, IntWidth, IrCmpOp,
    IrGlobalString, IrInstruction, IrMathOp, IrRegister, IrType, IrUnaryOp, IrValue, Literal,
    LoweredValue, UnaryOp,
};

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
                    } else if let Some(const_val) = self.compile_time_consts.get(name).cloned() {
                        Some(self.lower_literal(&const_val))
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
                crate::high_level_language::ast::PrimaryExpr::Grouped(expr) => {
                    self.lower_expression(expr)
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
                                    Literal::Integer(v) | Literal::HexInteger(v),
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
                                .error(format!("failed to lower argument for call to {name}"));
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
                crate::high_level_language::ast::PrimaryExpr::ArrayLiteral(elements) => {
                    self.lower_array_literal(elements)
                }
                crate::high_level_language::ast::PrimaryExpr::StructLiteral(fields) => {
                    // Lower struct literal
                    let mut lowered_fields = Vec::new();
                    for field_init in fields {
                        let field_value = self.lower_expression(&field_init.expr)?;
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
                            self.context.diagnostics.error(format!(
                                "struct literal field `{}` type mismatch: declared `{}`, got `{}`",
                                field_init.name, declared_ty, field_value.ty
                            ));
                            return None;
                        }
                        lowered_fields.push((field_init.name.clone(), field_value));
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
            },
            Expression::Binary { op, left, right } => {
                let lhs = self.lower_expression(left)?;
                let rhs = self.lower_expression(right)?;
                self.lower_binary(op, lhs, rhs)
            }
            Expression::Unary { op, expr } => {
                if op == &UnaryOp::AddressOf {
                    // `&` accepts l-values like identifiers and array elements, but never `&@ptr`.
                    if matches!(
                        &**expr,
                        Expression::Unary {
                            op: UnaryOp::Dereference,
                            ..
                        }
                    ) {
                        self.context.diagnostics.error(
                            "cannot take address of a dereference expression (`&@...` is invalid)"
                                .to_owned(),
                        );
                        return None;
                    }

                    if let Some(target) = self.expression_to_assign_target(expr) {
                        if let Some((ptr_reg, value_ty)) = self.resolve_assign_lvalue(&target) {
                            return Some(LoweredValue {
                                value: IrValue::Register(ptr_reg),
                                ty: IrType::Pointer(Box::new(value_ty)),
                            });
                        }
                    }

                    self.context.diagnostics.error(
                        "address-of requires an assignable l-value (identifier or array element)"
                            .to_owned(),
                    );
                    None
                } else {
                    let input = self.lower_expression(expr)?;
                    self.lower_unary(op, input)
                }
            }
            Expression::Assignment { target, rvalue } => {
                self.push_instruction(IrInstruction::Comment("assignment".to_owned()));
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
                    AssignTarget::StructDestructure(fields) => {
                        // Struct destructuring: extract each field from the aggregate value
                        self.lower_struct_destructuring(fields, &lowered)
                    }
                }
            }
        }
    }

    pub(super) fn lower_literal(&mut self, literal: &Literal) -> LoweredValue {
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
            Literal::String(content) => {
                // Create a global string constant and add it to pending list
                let string_name = format!("str_{}", self.pending_global_strings.len());
                self.pending_global_strings.push(IrGlobalString {
                    name: string_name.clone(),
                    content: content.clone(),
                });
                let content_len = content.len();

                // Represent strings as an inline struct `{ data: u8*, length: u64 }`.
                let struct_fields = vec![
                    (
                        "data".to_owned(),
                        IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                    ),
                    ("length".to_owned(), IrType::Integer(IntWidth::I64)),
                ];
                let struct_ty = IrType::Aggregate(struct_fields);

                // Allocate space for the inline struct on the stack.
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: struct_ty.clone(),
                    count: None,
                });

                // Store the pointer to the global string (field 0)
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                    value: IrValue::GlobalString(string_name),
                    ptr: dest.clone(),
                    offset: Some(0),
                });

                // Store the length (field 1)
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Integer(IntWidth::I64),
                    value: IrValue::Integer(content_len as i64),
                    ptr: dest.clone(),
                    offset: Some(8), // byte pointers are 8 bytes
                });

                LoweredValue {
                    value: IrValue::Register(dest),
                    ty: struct_ty,
                }
            }
        }
    }

    pub(super) fn lower_array_literal(
        &mut self,
        elements: &[Expression],
    ) -> Option<LoweredValue> {
        if elements.is_empty() {
            self.context
                .diagnostics
                .error("empty array literals are not supported yet".to_owned());
            return None;
        }

        let mut lowered_elements = Vec::with_capacity(elements.len());
        for element in elements {
            lowered_elements.push(self.lower_expression(element)?);
        }

        let element_ty = lowered_elements[0].ty.clone();
        for (index, lowered) in lowered_elements.iter().enumerate().skip(1) {
            if self.resolve_named_type(&lowered.ty) != self.resolve_named_type(&element_ty) {
                self.context.diagnostics.error(format!(
                    "array literal element {} has type `{}`, but expected `{}`",
                    index,
                    lowered.ty,
                    element_ty
                ));
                return None;
            }
        }

        let array_ty = IrType::Array {
            len: lowered_elements.len(),
            element: Box::new(element_ty.clone()),
        };
        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest.clone(),
            ty: array_ty.clone(),
            count: None,
        });

        let mut offset = 0i64;
        for lowered in lowered_elements {
            let element_size = self.type_size_in_bytes(&lowered.ty) as i64;
            self.push_instruction(IrInstruction::Store {
                ty: lowered.ty.clone(),
                value: lowered.value,
                ptr: dest.clone(),
                offset: Some(offset),
            });
            offset += element_size;
        }

        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: array_ty,
        })
    }

    fn expression_to_assign_target(&self, expr: &Expression) -> Option<AssignTarget> {
        match expr {
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Identifier(name)) => {
                Some(AssignTarget::Identifier(name.clone()))
            }
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Grouped(expr)) => {
                self.expression_to_assign_target(expr)
            }
            Expression::Unary {
                op: UnaryOp::Dereference,
                expr,
            } => Some(AssignTarget::Dereference(Box::new(
                self.expression_to_assign_target(expr)?,
            ))),
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::FieldAccess {
                expr,
                field,
            }) => Some(AssignTarget::FieldAccess {
                expr: Box::new(self.expression_to_assign_target(expr)?),
                field: field.clone(),
            }),
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::ArrayIndex {
                expr,
                index,
            }) => Some(AssignTarget::ArrayIndex {
                expr: Box::new(self.expression_to_assign_target(expr)?),
                index: index.clone(),
            }),
            _ => None,
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
                            "cannot dereference expression of non-pointer type `{other}`"
                        ));
                        return None;
                    }
                };

                let dest = self.new_temp();
                let ptr_reg = if let IrValue::Register(reg) = input.value {
                    reg
                } else {
                    self.context
                        .diagnostics
                        .error("cannot dereference non-register value".to_owned());
                    return None;
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
            UnaryOp::AddressOf => {
                if let IrValue::Register(reg) = input.value {
                    Some(LoweredValue {
                        value: IrValue::Register(reg),
                        ty: IrType::Pointer(Box::new(input.ty)),
                    })
                } else {
                    self.context
                        .diagnostics
                        .error("cannot take address of non-register".to_owned());
                    None
                }
            }
        }
    }

    pub(super) fn lower_struct_destructuring(
        &mut self,
        fields: &[crate::high_level_language::ast::StructDestructureField],
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        // Get the aggregate type fields
        let resolved_value_ty = self.resolve_named_type(&value.ty);
        let agg_fields = match &resolved_value_ty {
            IrType::Aggregate(fields) => fields.clone(),
            IrType::Pointer(inner) => {
                if let IrType::Aggregate(fields) = inner.as_ref() {
                    fields.clone()
                } else {
                    self.context
                        .diagnostics
                        .error("struct destructuring requires an aggregate type".to_owned());
                    return None;
                }
            }
            _ => {
                self.context
                    .diagnostics
                    .error("struct destructuring requires an aggregate type".to_owned());
                return None;
            }
        };

        // Base pointer for the aggregate
        let base_ptr = if let IrValue::Register(reg) = &value.value {
            reg.clone()
        } else {
            self.context
                .diagnostics
                .error("struct destructuring requires a register value".to_owned());
            return None;
        };

        // Build lookup by field name so partial/reordered destructuring follows source names.
        let mut field_offsets: std::collections::HashMap<&str, (i64, IrType)> =
            std::collections::HashMap::new();
        let mut running_offset = 0i64;
        for (name, ty) in &agg_fields {
            field_offsets.insert(name.as_str(), (running_offset, ty.clone()));
            running_offset += self.type_size_in_bytes(ty) as i64;
        }

        // Extract each requested field and assign to target variables.
        for field in fields {
            if let Some(ref name) = field.name {
                let Some((field_offset, field_ty)) = field_offsets.get(name.as_str()).cloned()
                else {
                    self.context.diagnostics.error(format!(
                        "struct destructuring field `{name}` not found in aggregate type"
                    ));
                    return None;
                };

                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: field_ty.clone(),
                    ptr: base_ptr.clone(),
                    offset: Some(field_offset),
                });

                let target_ptr = if let Some(var_info) = self.context.symbols.lookup(name) {
                    if let IrValue::Register(var_ptr) = &var_info.value {
                        var_ptr.clone()
                    } else {
                        self.context.diagnostics.error(format!(
                            "struct destructuring target `{name}` is not register-backed"
                        ));
                        return None;
                    }
                } else {
                    let var_ptr = IrRegister::Named(name.clone());
                    self.push_instruction(IrInstruction::Alloc {
                        dest: var_ptr.clone(),
                        ty: field_ty.clone(),
                        count: None,
                    });
                    self.context.symbols.insert(
                        name.clone(),
                        IrType::Pointer(Box::new(field_ty.clone())),
                        IrValue::Register(var_ptr.clone()),
                    );
                    var_ptr
                };

                self.push_instruction(IrInstruction::Store {
                    ty: field_ty,
                    value: IrValue::Register(dest),
                    ptr: target_ptr,
                    offset: None,
                });
            }
        }

        Some(value.clone())
    }
}
