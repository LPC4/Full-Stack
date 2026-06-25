use super::{
    BinaryOp, Expression, FloatWidth, HighLevelCompiler, IntWidth, IrCmpOp, IrGlobalString,
    IrInstruction, IrMathOp, IrRegister, IrType, IrUnaryOp, IrValue, Literal, LoweredValue,
    UnaryOp,
};
use crate::ast::StructDestructureField;
use log::warn;

/// Widening rank for integer widths, used to promote mixed-width binary
/// operands to the wider type (i64 outranks i32, etc.).
fn int_width_rank(width: &IntWidth) -> u8 {
    match width {
        IntWidth::I1 => 0,
        IntWidth::I8 => 1,
        IntWidth::I16 => 2,
        IntWidth::I32 => 3,
        IntWidth::I64 => 4,
    }
}

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
                let string_name = format!(
                    "{}{}",
                    self.string_prefix,
                    self.pending_global_strings.len()
                );
                self.pending_global_strings.push(IrGlobalString {
                    name: string_name.clone(),
                    content: content.clone(),
                });
                let content_len = content.len();

                let struct_ty = IrType::Slice(Box::new(IrType::Integer(IntWidth::I8)));

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
                self.context.error(format!(
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

    // Lower an array literal against a known element type. Each element is
    // lowered with the declared element type as its context, so bare struct
    // literals and width-flexible scalar literals are accepted inside `[...]`.
    fn lower_array_literal_for_type(
        &mut self,
        elements: &[Expression],
        element_ty: &IrType,
        len: usize,
    ) -> Option<LoweredValue> {
        if elements.len() != len {
            self.context.error(format!(
                "array literal has {} elements, but the declared type expects {len}",
                elements.len()
            ));
            return None;
        }

        let mut lowered_elements = Vec::with_capacity(elements.len());
        for element in elements {
            lowered_elements.push(self.lower_value_for_type(element, element_ty)?);
        }

        let array_ty = IrType::Array {
            len,
            element: Box::new(element_ty.clone()),
        };
        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest.clone(),
            ty: array_ty.clone(),
            count: None,
        });

        let element_size = self.type_size_in_bytes(&self.resolve_named_type(element_ty)) as i64;
        let mut offset = 0i64;
        for lowered in lowered_elements {
            self.push_instruction(IrInstruction::Store {
                ty: element_ty.clone(),
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

    /// Lower element-scaled pointer arithmetic, or return `None` when neither
    /// operand is a pointer (so the caller falls back to ordinary arithmetic).
    /// Semantic analysis has already rejected illegal forms (pointer-minus-pointer,
    /// integer-minus-pointer, arithmetic on unsized pointees), so here we only
    /// classify `T* + n` / `n + T*` / `T* - n`.
    fn lower_pointer_arith(
        &mut self,
        op: &BinaryOp,
        lhs: &LoweredValue,
        rhs: &LoweredValue,
    ) -> Option<LoweredValue> {
        let lhs_ptr = matches!(self.resolve_named_type(&lhs.ty), IrType::Pointer(_));
        let rhs_ptr = matches!(self.resolve_named_type(&rhs.ty), IrType::Pointer(_));

        // Pick the pointer base and integer offset; `n + ptr` is only valid for Add.
        let (base, offset) = match (lhs_ptr, rhs_ptr) {
            (true, false) => (lhs, rhs),
            (false, true) if matches!(op, BinaryOp::Add) => (rhs, lhs),
            _ => return None,
        };

        let IrType::Pointer(element_ty) = self.resolve_named_type(&base.ty) else {
            return None;
        };
        let element_ty = *element_ty;

        let base_reg = match &base.value {
            IrValue::Register(r) => r.clone(),
            _ => return None,
        };

        // Subtraction steps backward: negate the index before scaling.
        let idx = if matches!(op, BinaryOp::Sub) {
            let neg = self.new_temp();
            self.push_instruction(IrInstruction::Math {
                dest: neg.clone(),
                op: IrMathOp::Sub,
                ty: IrType::Integer(IntWidth::I64),
                lhs: IrValue::Integer(0),
                rhs: offset.value.clone(),
            });
            IrValue::Register(neg)
        } else {
            offset.value.clone()
        };

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Index {
            dest: dest.clone(),
            ty: element_ty.clone(),
            base_ptr: base_reg,
            idx,
        });
        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: IrType::Pointer(Box::new(element_ty)),
            is_unsigned: false,
        })
    }

    pub(super) fn lower_binary(
        &mut self,
        op: &BinaryOp,
        lhs: LoweredValue,
        rhs: LoweredValue,
    ) -> Option<LoweredValue> {
        // Typed pointer arithmetic is element-scaled. `T* +/- n` advances by
        // `n * sizeof(T)`, lowered through the `Index` instruction. Raw byte
        // arithmetic stays available through `u8*` (sizeof(u8) == 1).
        if matches!(op, BinaryOp::Add | BinaryOp::Sub)
            && let Some(result) = self.lower_pointer_arith(op, &lhs, &rhs)
        {
            return Some(result);
        }

        let dest = self.new_temp();
        match op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Shl
            | BinaryOp::Shr
            | BinaryOp::BitwiseAnd
            | BinaryOp::BitwiseXor
            | BinaryOp::BitwiseOr => {
                // Mixed-width integer operands promote to the wider type so an
                // untyped literal (default i32) does not truncate an i64 operand,
                // e.g. `2 * zx` must compute in i64. Shifts keep the left type:
                // the shift amount must not widen the value being shifted.
                let is_shift = matches!(op, BinaryOp::Shl | BinaryOp::Shr);
                let (op_ty, op_unsigned) = match (&lhs.ty, &rhs.ty) {
                    (IrType::Integer(lw), IrType::Integer(rw)) if !is_shift => {
                        match int_width_rank(lw).cmp(&int_width_rank(rw)) {
                            std::cmp::Ordering::Greater => (lhs.ty.clone(), lhs.is_unsigned),
                            std::cmp::Ordering::Less => (rhs.ty.clone(), rhs.is_unsigned),
                            std::cmp::Ordering::Equal => {
                                (lhs.ty.clone(), lhs.is_unsigned || rhs.is_unsigned)
                            }
                        }
                    }
                    _ => (lhs.ty.clone(), lhs.is_unsigned),
                };
                let ir_op = match op {
                    BinaryOp::Add => IrMathOp::Add,
                    BinaryOp::Sub => IrMathOp::Sub,
                    BinaryOp::Mul => IrMathOp::Mul,
                    BinaryOp::Div => {
                        if op_unsigned {
                            IrMathOp::UDiv
                        } else {
                            IrMathOp::SDiv
                        }
                    }
                    BinaryOp::Mod => {
                        if op_unsigned {
                            IrMathOp::UMod
                        } else {
                            IrMathOp::Mod
                        }
                    }
                    BinaryOp::And => IrMathOp::And,
                    BinaryOp::Or => IrMathOp::Or,
                    BinaryOp::Shl => IrMathOp::Shl,
                    BinaryOp::Shr => IrMathOp::Shr,
                    BinaryOp::BitwiseAnd => IrMathOp::And,
                    BinaryOp::BitwiseOr => IrMathOp::Or,
                    BinaryOp::BitwiseXor => IrMathOp::Xor,
                    _ => unreachable!(),
                };
                self.push_instruction(IrInstruction::Math {
                    dest: dest.clone(),
                    op: ir_op,
                    ty: op_ty.clone(),
                    lhs: lhs.value,
                    rhs: rhs.value,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: op_ty,
                    is_unsigned: op_unsigned,
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

    pub(super) fn lower_struct_destructuring_from_addr(
        &mut self,
        fields: &[StructDestructureField],
        addr: &LoweredValue,
    ) -> Option<LoweredValue> {
        // `addr` must be a register-backed pointer to an aggregate.
        let ptr_reg = match &addr.value {
            IrValue::Register(r) => r.clone(),
            other => {
                self.context.diagnostics.error(format!(
                    "destructuring source must be a register pointer, got {other:?}"
                ));
                return None;
            }
        };

        // Resolve the pointee type; it must be an aggregate.
        let pointee_ty = match &addr.ty {
            IrType::Pointer(inner) => self.resolve_named_type(inner),
            other => {
                self.context.error(format!(
                    "destructuring source type must be a pointer to an aggregate, got {other}"
                ));
                return None;
            }
        };

        let agg_fields = match pointee_ty {
            IrType::Aggregate(fields) => fields.clone(),
            other => {
                self.context.error(format!(
                    "destructuring source must point to an aggregate, got pointer to {other}"
                ));
                return None;
            }
        };

        // Build offset map: field name -> (byte_offset, field_type)
        let mut offset_map = std::collections::HashMap::new();
        let mut running_offset = 0i64;
        for (name, ty) in &agg_fields {
            running_offset =
                Self::align_to(running_offset, self.type_alignment_in_bytes(ty) as i64);
            offset_map.insert(name.as_str(), (running_offset, ty.clone()));
            running_offset += self.type_size_in_bytes(ty) as i64;
        }

        // Process each field in the pattern (order independent)
        for field in fields {
            if let Some(name) = &field.name {
                let (field_offset, field_ty) = if let Some(v) = offset_map.get(name.as_str()) {
                    v
                } else {
                    self.context
                        .error(format!("field `{name}` not found in aggregate type"));
                    return None;
                };

                // Load the field value from the source pointer at the computed offset
                let loaded = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: loaded.clone(),
                    ty: field_ty.clone(),
                    ptr: ptr_reg.clone(),
                    offset: Some(*field_offset),
                });

                // Determine the target pointer (existing variable or create a new stack slot)
                let target_ptr = if let Some(var_info) = self.context.symbols.lookup(name) {
                    if let IrValue::Register(var_ptr) = &var_info.value {
                        var_ptr.clone()
                    } else {
                        self.context
                            .error(format!("variable `{name}` is not register-backed"));
                        return None;
                    }
                } else {
                    // Introduce a new stack variable for the destructured field.
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

                // Store the loaded value into the target
                self.push_instruction(IrInstruction::Store {
                    ty: field_ty.clone(),
                    value: IrValue::Register(loaded),
                    ptr: target_ptr,
                    offset: None,
                });
            }
            // If field.name is None (should not happen), skip.
            warn!("field.name is None");
        }

        Some(addr.clone())
    }

    // Lower an expression, folding a bare or negated integer literal to a typed
    // immediate of the target width so `x: i64 = -1` works without a cast.
    pub(super) fn lower_value_for_type(
        &mut self,
        expr: &Expression,
        target_ty: &IrType,
    ) -> Option<LoweredValue> {
        if let Expression::Primary(crate::ast::PrimaryExpr::StructLiteral(fields)) = expr {
            return self.lower_contextual_struct_literal(fields, target_ty);
        }
        // An array literal lowers each element against the declared element
        // type, so contextual struct literals (`Point[N] = [{..}, ..]`) and
        // width-flexible scalar literals get their context from the target.
        if let IrType::Array { element, len } = self.resolve_named_type(target_ty)
            && let Expression::Primary(crate::ast::PrimaryExpr::ArrayLiteral(elems)) = expr
        {
            // `arr: T[N] = []` zero-fills; a non-empty literal sets each element.
            if elems.is_empty() {
                return self.lower_zero_value(target_ty);
            }
            return self.lower_array_literal_for_type(elems, &element, len);
        }
        // A fixed array coerces to a slice: build { ptr: &arr[0], len: N }.
        if let IrType::Slice(elem) = self.resolve_named_type(target_ty) {
            return self.lower_array_to_slice(expr, &elem);
        }
        if matches!(self.resolve_named_type(target_ty), IrType::Integer(_))
            && let Some(v) = Self::fold_int_literal(expr)
        {
            return Some(LoweredValue {
                value: IrValue::Integer(v),
                ty: target_ty.clone(),
                is_unsigned: false,
            });
        }
        self.lower_expression(expr)
    }

    // Build a slice value from a fixed array. The array's address is &arr[0] and
    // its length comes from the static array type. A slice-typed source is passed
    // through unchanged (copy of the fat pointer).
    fn lower_array_to_slice(
        &mut self,
        expr: &Expression,
        elem_ty: &IrType,
    ) -> Option<LoweredValue> {
        let base = self
            .lower_expr(expr, super::EvalMode::Address)
            .or_else(|| self.lower_expr(expr, super::EvalMode::Value))?;
        let resolved = self.resolve_named_type(&base.ty);

        // Already a slice place or value: hand it straight back.
        if matches!(&resolved, IrType::Slice(_))
            || matches!(&resolved, IrType::Pointer(inner) if matches!(self.resolve_named_type(inner), IrType::Slice(_)))
        {
            return Some(base);
        }

        let array_ty = match resolved {
            IrType::Array { .. } => resolved,
            IrType::Pointer(inner) => self.resolve_named_type(&inner),
            _ => {
                self.context
                    .error("only a fixed array can coerce to a slice".to_owned());
                return None;
            }
        };
        let IrType::Array { len, .. } = array_ty else {
            self.context
                .error("only a fixed array can coerce to a slice".to_owned());
            return None;
        };
        let IrValue::Register(data_reg) = base.value else {
            return None;
        };

        let slice_ty = IrType::Slice(Box::new(elem_ty.clone()));
        let slot = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: slot.clone(),
            ty: slice_ty.clone(),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: IrType::Pointer(Box::new(elem_ty.clone())),
            value: IrValue::Register(data_reg),
            ptr: slot.clone(),
            offset: Some(0),
        });
        self.push_instruction(IrInstruction::Store {
            ty: IrType::Integer(IntWidth::I64),
            value: IrValue::Integer(len as i64),
            ptr: slot.clone(),
            offset: Some(8),
        });
        Some(LoweredValue {
            value: IrValue::Register(slot),
            ty: slice_ty,
            is_unsigned: false,
        })
    }

    fn lower_contextual_struct_literal(
        &mut self,
        fields: &[crate::ast::FieldInit],
        target_ty: &IrType,
    ) -> Option<LoweredValue> {
        let resolved = self.resolve_named_type(target_ty);
        let IrType::Aggregate(declared_fields) = resolved else {
            self.context.error(format!(
                "contextual struct literal target `{target_ty}` is not a struct"
            ));
            return None;
        };

        let mut values = std::collections::HashMap::new();
        for field in fields {
            if values.contains_key(&field.name) {
                self.context.error(format!(
                    "duplicate field `{}` in struct literal",
                    field.name
                ));
                return None;
            }
            let Some((_, field_ty)) = declared_fields
                .iter()
                .find(|(field_name, _)| field_name == &field.name)
            else {
                self.context.error(format!(
                    "unknown field `{}` in contextual struct literal",
                    field.name
                ));
                return None;
            };
            let value = self.lower_value_for_type(&field.expr, field_ty)?;
            values.insert(field.name.clone(), value);
        }

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest.clone(),
            ty: target_ty.clone(),
            count: None,
        });
        for (field_name, field_ty) in &declared_fields {
            let (offset, _) = self.aggregate_field_offset_and_type(&declared_fields, field_name)?;
            let value = match values.remove(field_name) {
                Some(value) => value.value,
                None => self.lower_zero_value(field_ty)?.value,
            };
            self.push_instruction(IrInstruction::Store {
                ty: field_ty.clone(),
                value,
                ptr: dest.clone(),
                offset: Some(offset),
            });
        }

        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: target_ty.clone(),
            is_unsigned: false,
        })
    }

    fn lower_zero_value(&mut self, ty: &IrType) -> Option<LoweredValue> {
        let resolved = self.resolve_named_type(ty);
        let value = match resolved {
            IrType::Integer(_) => IrValue::Integer(0),
            IrType::Float(_) => IrValue::Float(0.0),
            IrType::Pointer(_) => IrValue::Null,
            IrType::Aggregate(fields) => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: ty.clone(),
                    count: None,
                });
                for (name, field_ty) in &fields {
                    let (offset, _) = self.aggregate_field_offset_and_type(&fields, name)?;
                    let zero = self.lower_zero_value(field_ty)?;
                    self.push_instruction(IrInstruction::Store {
                        ty: field_ty.clone(),
                        value: zero.value,
                        ptr: dest.clone(),
                        offset: Some(offset),
                    });
                }
                IrValue::Register(dest)
            }
            IrType::Slice(_) => {
                // A zero/empty slice is { ptr: null, len: 0 }.
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: ty.clone(),
                    count: None,
                });
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
                    value: IrValue::Null,
                    ptr: dest.clone(),
                    offset: Some(0),
                });
                self.push_instruction(IrInstruction::Store {
                    ty: IrType::Integer(IntWidth::I64),
                    value: IrValue::Integer(0),
                    ptr: dest.clone(),
                    offset: Some(8),
                });
                IrValue::Register(dest)
            }
            IrType::Array { len, element } => {
                // Zero-fill each element. Local arrays are not implicitly zeroed
                // (unlike .bss globals), so emit explicit element stores.
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: dest.clone(),
                    ty: ty.clone(),
                    count: None,
                });
                let element_size =
                    self.type_size_in_bytes(&self.resolve_named_type(&element)) as i64;
                for i in 0..len {
                    let zero = self.lower_zero_value(&element)?;
                    self.push_instruction(IrInstruction::Store {
                        ty: (*element).clone(),
                        value: zero.value,
                        ptr: dest.clone(),
                        offset: Some(i as i64 * element_size),
                    });
                }
                IrValue::Register(dest)
            }
            IrType::Named(_) | IrType::Void => {
                self.context
                    .error(format!("cannot synthesize a zero value for `{ty}`"));
                return None;
            }
        };
        Some(LoweredValue {
            value,
            ty: ty.clone(),
            is_unsigned: false,
        })
    }

    // Fold a bare or (grouped) negated integer literal to its i64 value.
    fn fold_int_literal(expr: &Expression) -> Option<i64> {
        match expr {
            Expression::Primary(crate::ast::PrimaryExpr::Literal(
                Literal::Integer(v) | Literal::HexInteger(v),
            )) => Some(*v),
            Expression::Primary(crate::ast::PrimaryExpr::Grouped(inner)) => {
                Self::fold_int_literal(inner)
            }
            Expression::Unary {
                op: UnaryOp::Negate,
                expr: inner,
            } => Self::fold_int_literal(inner).map(|v| -v),
            _ => None,
        }
    }

    // Serialize a constant global initializer to little-endian bytes for `ty`.
    // None when it is absent, zero (stays in .bss), or not a constant.
    pub(super) fn const_init_bytes(&self, expr: &Expression, ty: &IrType) -> Option<Vec<u8>> {
        let resolved = self.resolve_named_type(ty);
        match &resolved {
            IrType::Integer(width) => {
                let v = match self.eval_const_expr(expr).ok()? {
                    Literal::Integer(v) | Literal::HexInteger(v) => v,
                    Literal::Boolean(b) => b as i64,
                    _ => return None,
                };
                if v == 0 {
                    return None;
                }
                let size = match width {
                    IntWidth::I1 | IntWidth::I8 => 1,
                    IntWidth::I16 => 2,
                    IntWidth::I32 => 4,
                    IntWidth::I64 => 8,
                };
                Some(v.to_le_bytes()[..size].to_vec())
            }
            IrType::Float(width) => {
                let f = match self.eval_const_expr(expr).ok()? {
                    Literal::Float(f) => f,
                    Literal::Integer(v) => v as f64,
                    _ => return None,
                };
                match width {
                    FloatWidth::F32 => {
                        if f == 0.0 {
                            return None;
                        }
                        Some((f as f32).to_le_bytes().to_vec())
                    }
                    FloatWidth::F64 => {
                        if f == 0.0 {
                            return None;
                        }
                        Some(f.to_le_bytes().to_vec())
                    }
                }
            }
            IrType::Array { len, element } => {
                let elements = match expr {
                    Expression::Primary(crate::ast::PrimaryExpr::ArrayLiteral(elems)) => elems,
                    _ => return None,
                };
                if elements.len() != *len {
                    return None;
                }
                let elem_size = self.type_size_in_bytes(element);
                let mut out = Vec::with_capacity(len * elem_size);
                for elem in elements {
                    // A zeroed element still needs its slot, so fill it explicitly.
                    let bytes = self
                        .const_init_bytes(elem, element)
                        .unwrap_or_else(|| vec![0u8; elem_size]);
                    if bytes.len() != elem_size {
                        return None;
                    }
                    out.extend_from_slice(&bytes);
                }
                if out.iter().all(|&b| b == 0) {
                    return None;
                }
                Some(out)
            }
            _ => None,
        }
    }

    pub(crate) fn align_to(value: i64, alignment: i64) -> i64 {
        let alignment = alignment.max(1);
        (value + alignment - 1) & !(alignment - 1)
    }
}
