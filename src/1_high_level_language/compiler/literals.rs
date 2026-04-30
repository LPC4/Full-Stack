use super::{
    BinaryOp, Expression, FloatWidth, HighLevelCompiler, IntWidth, IrCmpOp, IrGlobalString,
    IrInstruction, IrMathOp, IrRegister, IrType, IrUnaryOp, IrValue, Literal, LoweredValue,
    UnaryOp,
};

impl HighLevelCompiler {
    pub(super) fn lower_literal(&mut self, literal: &Literal) -> LoweredValue {
        match literal {
            Literal::Integer(value) | Literal::HexInteger(value) => LoweredValue {
                value: IrValue::Integer(*value),
                ty: IrType::Integer(IntWidth::I32),
                is_unsigned: false,
            },
            Literal::Float(value) => LoweredValue {
                value: IrValue::Float(*value),
                ty: IrType::Float(FloatWidth::F64),
                is_unsigned: false,
            },
            Literal::Boolean(value) => LoweredValue {
                value: IrValue::Bool(*value),
                ty: IrType::Integer(IntWidth::I1),
                is_unsigned: false,
            },
            Literal::Null => LoweredValue {
                value: IrValue::Null,
                ty: IrType::Pointer(Box::new(IrType::Named("unknown".to_owned()))),
                is_unsigned: false,
            },
            Literal::String(content) => {
                let string_name = format!("str_{}", self.pending_global_strings.len());
                self.pending_global_strings.push(IrGlobalString {
                    name: string_name.clone(),
                    content: content.clone(),
                });
                let content_len = content.len();

                let struct_fields = vec![
                    (
                        "data".to_owned(),
                        IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                    ),
                    ("length".to_owned(), IrType::Integer(IntWidth::I64)),
                ];
                let struct_ty = IrType::Aggregate(struct_fields);

                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: struct_ty.clone(),
                    count: None,
                });
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                    value: IrValue::GlobalString(string_name),
                    ptr: dest.clone(),
                    offset: Some(0),
                });
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Integer(IntWidth::I64),
                    value: IrValue::Integer(content_len as i64),
                    ptr: dest.clone(),
                    offset: Some(8),
                });

                LoweredValue {
                    value: IrValue::Register(dest),
                    ty: struct_ty,
                    is_unsigned: false,
                }
            }
        }
    }

    pub(super) fn lower_array_literal(&mut self, elements: &[Expression]) -> Option<LoweredValue> {
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
                    index, lowered.ty, element_ty
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
            is_unsigned: false,
        })
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
                    BinaryOp::Div => {
                        if lhs.is_unsigned {
                            IrMathOp::Div
                        } else {
                            IrMathOp::SDiv
                        }
                    }
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
                    is_unsigned: lhs.is_unsigned,
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
                    BinaryOp::Lt => {
                        if lhs.is_unsigned {
                            IrCmpOp::Ult
                        } else {
                            IrCmpOp::Slt
                        }
                    }
                    BinaryOp::Lte => {
                        if lhs.is_unsigned {
                            IrCmpOp::Ule
                        } else {
                            IrCmpOp::Sle
                        }
                    }
                    BinaryOp::Gt => {
                        if lhs.is_unsigned {
                            IrCmpOp::Ugt
                        } else {
                            IrCmpOp::Sgt
                        }
                    }
                    BinaryOp::Gte => {
                        if lhs.is_unsigned {
                            IrCmpOp::Uge
                        } else {
                            IrCmpOp::Sge
                        }
                    }
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
                    is_unsigned: false,
                })
            }
        }
    }

    pub(super) fn lower_unary(
        &mut self,
        op: &UnaryOp,
        input: LoweredValue,
    ) -> Option<LoweredValue> {
        let dest = self.new_temp();
        let ir_op = match op {
            UnaryOp::Negate => IrUnaryOp::Neg,
            UnaryOp::Not => IrUnaryOp::Not,
            _ => unreachable!("lower_unary only handles Negate/Not"),
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
            is_unsigned: input.is_unsigned,
        })
    }

    pub(super) fn lower_struct_destructuring(
        &mut self,
        fields: &[crate::high_level_language::ast::StructDestructureField],
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
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

        let base_ptr = if let IrValue::Register(reg) = &value.value {
            reg.clone()
        } else {
            self.context
                .diagnostics
                .error("struct destructuring requires a register value".to_owned());
            return None;
        };

        let mut field_offsets: std::collections::HashMap<&str, (i64, IrType)> =
            std::collections::HashMap::new();
        let mut running_offset = 0i64;
        for (name, ty) in &agg_fields {
            running_offset =
                Self::align_to(running_offset, self.type_alignment_in_bytes(ty) as i64);
            field_offsets.insert(name.as_str(), (running_offset, ty.clone()));
            running_offset += self.type_size_in_bytes(ty) as i64;
        }

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

    pub(crate) fn align_to(value: i64, alignment: i64) -> i64 {
        let alignment = alignment.max(1);
        (value + alignment - 1) & !(alignment - 1)
    }
}
