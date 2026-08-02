use super::{
    AssignTarget, EvalMode, Expression, HighLevelCompiler, IrInstruction, IrType, IrValue, Literal,
    LoweredValue, UnaryOp,
};
use crate::ast::{FormatSegment, PrimaryExpr, Type};
use crate::ir::{FloatWidth, IntWidth, IrCastMode, IrCmpOp, IrMathOp, IrRegister, IrTerminator};

// Diagnostic exit code for a failed slice bounds check (SIGABRT convention).
const SLICE_BOUNDS_TRAP_CODE: u32 = 134;

// Diagnostic exit code for a failed `assert(cond)`.
const ASSERT_TRAP_CODE: u32 = 101;

// One evaluated `format_into` hole, classified for the sizing/write passes.
enum FormatHole {
    I64(IrValue),
    U64(IrValue),
    F64(IrValue),
    Bool(IrValue),
    Str(IrValue),
    // A `Format` implementor: receiver pointer, its checked byte length, and
    // the flat `Type_format_exact_into` symbol to call in the write pass.
    Custom {
        self_ptr: IrValue,
        len: IrValue,
        write_sym: String,
    },
}

impl HighLevelCompiler {
    /// Evaluate expression in Value mode (rvalue context).
    pub(super) fn lower_expression(&mut self, expression: &Expression) -> Option<LoweredValue> {
        self.lower_expr(expression, EvalMode::Value)
    }

    /// Unified expression lowering: `EvalMode::Value` produces the loaded value,
    /// `EvalMode::Address` a `Pointer(T)` to the storage location.
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
            Expression::Match { .. } => {
                self.context.error(
                    "a value `match` is only supported directly as a binding initializer, \
                     assignment rvalue, or `return` value"
                        .to_owned(),
                );
                None
            }
            Expression::Try(expr) => self.lower_try(expr, mode),
        }
    }

    fn lower_primary(&mut self, primary: &PrimaryExpr, mode: EvalMode) -> Option<LoweredValue> {
        match primary {
            // Folded to literals before lowering; reaching one means the owner
            // was never resolved (e.g. an unknown owner type).
            PrimaryExpr::AssociatedConst { owner, member, .. } => {
                self.context
                    .error(format!("unresolved associated constant `{owner}.{member}`"));
                None
            }
            PrimaryExpr::Identifier(name) => {
                // A name matching a unit enum variant constructs that variant.
                if mode == EvalMode::Value && self.is_unit_variant_name(name) {
                    return self.lower_enum_construct(name, &[]);
                }
                self.lower_identifier(name, mode)
            }
            PrimaryExpr::Literal(literal) => Some(self.lower_literal(literal)),
            PrimaryExpr::Grouped(expr) => self.lower_expr(expr, mode),
            PrimaryExpr::FormatInto {
                alias,
                dest,
                segments,
            } => self.lower_format_into(alias, dest, segments, mode),
            PrimaryExpr::FieldAccess { expr, field } => {
                self.lower_field_access_expr(expr, field, mode)
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                self.lower_array_index_expr(expr, index, mode)
            }
            PrimaryExpr::Slice {
                expr,
                start,
                end,
                inclusive,
            } => self.lower_slice_expr(expr, start.as_deref(), end.as_deref(), *inclusive),
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
            PrimaryExpr::FunctionCall {
                name,
                type_arguments,
                arguments,
            } => {
                // Function call results are rvalues with no addressable storage.
                if mode == EvalMode::Address {
                    return None;
                }
                // A call whose name is an enum variant constructs that variant.
                if !self.variant_candidates(name, None).is_empty() {
                    return self.lower_enum_construct(name, arguments);
                }
                if matches!(name.as_str(), "size_of" | "align_of") {
                    if !arguments.is_empty() {
                        self.context.error(format!(
                            "{name}<T>() is compile-time reflection and accepts no value arguments"
                        ));
                        return None;
                    }
                    let [ty] = type_arguments.as_slice() else {
                        self.context.error(format!(
                            "{name}<T>() expects exactly one type argument, got {}",
                            type_arguments.len()
                        ));
                        return None;
                    };
                    let lowered_ty = self.lower_type(ty);
                    let Some((size, alignment)) = self.reflected_layout(&lowered_ty) else {
                        self.context.error(format!(
                            "{name}<T>() requires a resolved, sized type; `{lowered_ty}` is not sized"
                        ));
                        return None;
                    };
                    let value = if name == "size_of" { size } else { alignment };
                    return Some(LoweredValue {
                        value: IrValue::Integer(value as i64),
                        ty: IrType::Integer(IntWidth::I64),
                        is_unsigned: true,
                    });
                }
                // A user-declared `assert` keeps its own meaning; otherwise this is
                // the checked-failure builtin.
                if name == "assert" && !self.function_return_types.contains_key(name) {
                    return self.lower_assert(arguments);
                }
                // A function-pointer binding calls indirectly; any other bare
                // name stays a direct call so extern functions link as before.
                if name != "free"
                    && !self.function_return_types.contains_key(name)
                    && self.is_function_pointer_binding(name)
                {
                    return self.lower_indirect_call(name, arguments);
                }
                if name == "free" {
                    if arguments.len() != 1 {
                        self.context
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
                let param_types = self.function_param_types.get(name).cloned();
                for (index, arg) in arguments.iter().enumerate() {
                    let lowered = match param_types.as_ref().and_then(|params| params.get(index)) {
                        Some(expected) => self.lower_value_for_type(arg, expected),
                        None => self.lower_expr(arg, EvalMode::Value),
                    };
                    if let Some(lowered) = lowered {
                        arg_values.push(lowered.value);
                    } else {
                        self.context
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
            PrimaryExpr::StructLiteral { fields, rest } => {
                if *rest {
                    self.context.error(
                        "`..` needs a known struct type; annotate the binding or name the type"
                            .to_owned(),
                    );
                    return None;
                }
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
            PrimaryExpr::NamedStructLiteral {
                name,
                type_args,
                fields,
                rest,
            } => self.lower_named_struct_literal(name, type_args, fields, *rest),
            PrimaryExpr::CallExpr { callee, arguments } => {
                if mode == EvalMode::Address {
                    return None;
                }
                // A `recv.name(args)` callee is either a method (free fn `Type_name`)
                // or an indirect call through a fn-pointer field; a field wins.
                if let Expression::Primary(PrimaryExpr::FieldAccess { expr, field }) =
                    callee.as_ref()
                {
                    let recv_addr = self.lower_expr(expr, EvalMode::Address)?;
                    if !self.receiver_has_field(&recv_addr.ty, field)
                        && let Some(type_name) = self.receiver_struct_name(&recv_addr.ty)
                        && let Some(method) = self.resolve_method_symbol(&type_name, field)
                    {
                        return self.lower_method_call(&method, recv_addr, arguments);
                    }
                    let field_val =
                        self.lower_field_access_mode(&recv_addr, field, EvalMode::Value)?;
                    return self.lower_call_through_value(
                        field_val,
                        arguments,
                        &format!("field `{field}`"),
                    );
                }
                let callee_val = self.lower_expr(callee, EvalMode::Value)?;
                self.lower_call_through_value(callee_val, arguments, "callee expression")
            }
        }
    }

    fn lower_named_struct_literal(
        &mut self,
        name: &str,
        type_args: &[crate::ast::Type],
        fields: &[crate::ast::FieldInit],
        rest: bool,
    ) -> Option<LoweredValue> {
        // A generic literal lowers its `<...>`, which specializes and registers
        // the concrete struct before we look it up.
        let nominal_ty = if type_args.is_empty() {
            IrType::Named(name.to_owned())
        } else {
            self.lower_type(&crate::ast::Type::Named {
                name: name.into(),
                args: type_args.to_vec(),
            })
        };
        let resolved = self.resolve_named_type(&nominal_ty);
        let IrType::Aggregate(declared_fields) = resolved else {
            self.context
                .error(format!("named literal type `{name}` is not a struct"));
            return None;
        };

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest.clone(),
            ty: nominal_ty.clone(),
            count: None,
        });

        let mut seen = std::collections::HashSet::new();
        for field in fields {
            if !seen.insert(field.name.clone()) {
                self.context.error(format!(
                    "duplicate field `{}` in `{name}` literal",
                    field.name
                ));
                return None;
            }
            let Some((offset, field_ty)) =
                self.aggregate_field_offset_and_type(&declared_fields, &field.name)
            else {
                self.context.error(format!(
                    "unknown field `{}` in `{name}` literal",
                    field.name
                ));
                return None;
            };
            let value = self.lower_value_for_type(&field.expr, &field_ty)?;
            self.push_instruction(IrInstruction::Store {
                ty: field_ty,
                value: value.value,
                ptr: dest.clone(),
                offset: Some(offset),
            });
        }

        // Unnamed fields are an error unless the literal opted into zero-fill.
        for (field_name, field_ty) in &declared_fields {
            if seen.contains(field_name) {
                continue;
            }
            if !rest {
                self.context.error(format!(
                    "missing field `{field_name}` in `{name}` literal; add `..` to zero-fill"
                ));
                return None;
            }
            let (offset, _) = self.aggregate_field_offset_and_type(&declared_fields, field_name)?;
            let zero = self.lower_zero_value(field_ty)?;
            self.push_instruction(IrInstruction::Store {
                ty: field_ty.clone(),
                value: zero.value,
                ptr: dest.clone(),
                offset: Some(offset),
            });
        }

        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: nominal_ty,
            is_unsigned: false,
        })
    }

    fn lower_identifier(&mut self, name: &str, mode: EvalMode) -> Option<LoweredValue> {
        let info = self.context.symbols.lookup(name).cloned();
        if let Some(info) = info {
            match mode {
                EvalMode::Address => {
                    // Locals are Pointer(T) stack slots, so the slot pointer is the address.
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
        } else if mode == EvalMode::Value
            && let Some(return_ty) = self.function_return_types.get(name).cloned()
        {
            let params = self
                .function_param_types
                .get(name)
                .cloned()
                .unwrap_or_default();
            let dest = self.new_temp();
            self.push_instruction(IrInstruction::FunctionAddr {
                dest: dest.clone(),
                name: name.to_owned(),
            });
            Some(LoweredValue {
                value: IrValue::Register(dest),
                ty: IrType::FunctionPointer {
                    params,
                    return_type: Box::new(return_ty),
                },
                is_unsigned: false,
            })
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
                .error_at_token(name, format!("unknown identifier `{name}`"));
            None
        }
    }

    // True when `name` is a local or global binding of function-pointer type,
    // i.e. callable through an indirect call rather than a direct symbol call.
    fn is_function_pointer_binding(&self, name: &str) -> bool {
        // Locals live in `Pointer(T)` stack slots, so the value type is the
        // pointee; globals store the value type directly.
        let value_ty = if let Some(info) = self.context.symbols.lookup(name) {
            match &info.ty {
                IrType::Pointer(inner) => (**inner).clone(),
                other => other.clone(),
            }
        } else if let Some(gv_ty) = self.global_vars.get(name) {
            gv_ty.clone()
        } else {
            return false;
        };
        matches!(
            self.resolve_named_type(&value_ty),
            IrType::FunctionPointer { .. }
        )
    }

    fn lower_indirect_call(
        &mut self,
        name: &str,
        arguments: &[Expression],
    ) -> Option<LoweredValue> {
        let callee = self.lower_identifier(name, EvalMode::Value)?;
        self.lower_call_through_value(callee, arguments, &format!("`{name}`"))
    }

    // Lower a call whose callee is an already-lowered function-pointer value.
    // `desc` names the callee for diagnostics (e.g. a field path).
    fn lower_call_through_value(
        &mut self,
        callee: LoweredValue,
        arguments: &[Expression],
        desc: &str,
    ) -> Option<LoweredValue> {
        let IrType::FunctionPointer {
            params,
            return_type,
        } = self.resolve_named_type(&callee.ty)
        else {
            self.context.error(format!("{desc} is not callable"));
            return None;
        };

        if arguments.len() != params.len() {
            self.context.error(format!(
                "function pointer {desc} expects {} argument(s), got {}",
                params.len(),
                arguments.len()
            ));
            return None;
        }

        let mut arg_values = Vec::new();
        for (index, (arg, expected)) in arguments.iter().zip(params.iter()).enumerate() {
            let Some(lowered) = self.lower_value_for_type(arg, expected) else {
                self.context.error(format!(
                    "failed to lower argument {} for indirect call {desc}",
                    index + 1
                ));
                return None;
            };
            arg_values.push(lowered.value);
        }

        let dest = if *return_type == IrType::Void {
            None
        } else {
            Some(self.new_temp())
        };
        self.push_instruction(IrInstruction::IndirectCall {
            dest: dest.clone(),
            callee: callee.value,
            callee_ty: IrType::FunctionPointer {
                params,
                return_type: return_type.clone(),
            },
            args: arg_values,
        });

        Some(LoweredValue {
            value: dest.map(IrValue::Register).unwrap_or(IrValue::Null),
            ty: *return_type,
            is_unsigned: false,
        })
    }

    // True when the receiver type, after peeling pointers, is a struct holding `field`.
    fn receiver_has_field(&self, ty: &IrType, field: &str) -> bool {
        let mut resolved = self.resolve_named_type(ty);
        while let IrType::Pointer(inner) = resolved {
            resolved = self.resolve_named_type(&inner);
        }
        matches!(resolved, IrType::Aggregate(fields) if fields.iter().any(|(n, _)| n == field))
    }

    // The link symbol for `Type.method`, accepting the `module__Type_method` spelling
    // an imported `impl` carries.
    pub(super) fn resolve_method_symbol(&self, type_name: &str, method: &str) -> Option<String> {
        let direct = format!("{type_name}_{method}");
        if self.function_param_types.contains_key(&direct) {
            return Some(direct);
        }
        let imported_suffix = format!("__{direct}");
        self.function_param_types
            .keys()
            .find(|name| name.ends_with(&imported_suffix))
            .cloned()
    }

    /// The named struct type of a receiver address, peeling pointer levels. A
    /// receiver already resolved to its field list is matched back to its name.
    pub(super) fn receiver_struct_name(&self, ty: &IrType) -> Option<String> {
        match ty {
            IrType::Named(name) => Some(name.clone()),
            IrType::Pointer(inner) => self.receiver_struct_name(inner),
            IrType::Aggregate(_) => self.context.types.name_of_aggregate(ty).map(str::to_owned),
            _ => None,
        }
    }

    // Lower `recv.method(args)` as a direct call `method(self, args)`. `recv_addr`
    // is the receiver address, reduced to a single `Type*` for the `self` argument.
    fn lower_method_call(
        &mut self,
        method: &str,
        recv_addr: LoweredValue,
        arguments: &[Expression],
    ) -> Option<LoweredValue> {
        let param_types = self
            .function_param_types
            .get(method)
            .cloned()
            .unwrap_or_default();
        let expected = param_types.len().saturating_sub(1);
        if arguments.len() != expected {
            self.context.error_at_token(
                method,
                format!(
                    "method `{method}` expects {expected} argument(s), got {}",
                    arguments.len()
                ),
            );
            return None;
        }

        // Reduce the receiver address down to one pointer level over the struct.
        let mut ptr_ty = recv_addr.ty;
        let mut self_val = recv_addr.value;
        while let IrType::Pointer(inner) = ptr_ty.clone() {
            if !matches!(self.resolve_named_type(&inner), IrType::Pointer(_)) {
                break;
            }
            let IrValue::Register(reg) = self_val else {
                return None;
            };
            let dest = self.new_temp();
            self.push_instruction(IrInstruction::Load {
                dest: dest.clone(),
                ty: (*inner).clone(),
                ptr: reg,
                offset: None,
            });
            ptr_ty = *inner;
            self_val = IrValue::Register(dest);
        }

        let self_param = param_types.first()?;
        if !matches!(self_param, IrType::Pointer(_)) {
            let IrValue::Register(ptr) = self_val else {
                return None;
            };
            let dest = self.new_temp();
            self.push_instruction(IrInstruction::Load {
                dest: dest.clone(),
                ty: self_param.clone(),
                ptr,
                offset: None,
            });
            self_val = IrValue::Register(dest);
        }

        let mut arg_values = vec![self_val];
        for (index, arg) in arguments.iter().enumerate() {
            let Some(lowered) = self.lower_value_for_type(arg, &param_types[index + 1]) else {
                self.context.error(format!(
                    "failed to lower argument {} for method `{method}`",
                    index + 1
                ));
                return None;
            };
            arg_values.push(lowered.value);
        }

        let return_ty = self
            .function_return_types
            .get(method)
            .cloned()
            .unwrap_or(IrType::Void);
        let dest = if return_ty == IrType::Void {
            None
        } else {
            Some(self.new_temp())
        };
        self.push_instruction(IrInstruction::Call {
            dest: dest.clone(),
            function: method.to_owned(),
            args: arg_values,
        });
        Some(LoweredValue {
            value: dest.map(IrValue::Register).unwrap_or(IrValue::Null),
            ty: return_ty,
            is_unsigned: false,
        })
    }

    // Emit a call to a string-module runtime helper by flat symbol name.
    fn emit_format_runtime_call(
        &mut self,
        symbol: &str,
        args: Vec<IrValue>,
    ) -> Option<LoweredValue> {
        let Some(return_ty) = self.function_return_types.get(symbol).cloned() else {
            self.context.error(format!(
                "string interpolation requires the string module runtime (missing `{symbol}`)"
            ));
            return None;
        };
        let dest = if return_ty == IrType::Void {
            None
        } else {
            Some(self.new_temp())
        };
        self.push_instruction(IrInstruction::Call {
            dest: dest.clone(),
            function: symbol.to_owned(),
            args,
        });
        Some(LoweredValue {
            value: dest.map(IrValue::Register).unwrap_or(IrValue::Null),
            ty: return_ty,
            is_unsigned: true,
        })
    }

    // Widen an integer hole value to 64 bits for the format runtime.
    fn format_widen_int(&mut self, value: &LoweredValue, unsigned: bool) -> IrValue {
        if matches!(
            self.resolve_named_type(&value.ty),
            IrType::Integer(IntWidth::I64)
        ) {
            return value.value.clone();
        }
        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Cast {
            dest: dest.clone(),
            mode: if unsigned {
                IrCastMode::Zext
            } else {
                IrCastMode::Sext
            },
            value: value.value.clone(),
            ty: IrType::Integer(IntWidth::I64),
        });
        IrValue::Register(dest)
    }

    // Evaluate one hole expression exactly once and classify it. `Format`
    // implementors are materialized so their impl methods get a receiver.
    fn lower_format_hole(
        &mut self,
        expr: &Expression,
        result_enum: &str,
        result_ptr: &IrRegister,
        done_label: &crate::ir::IrLabel,
    ) -> Option<FormatHole> {
        let value = self.lower_expr(expr, EvalMode::Value)?;
        let resolved = self.resolve_named_type(&value.ty);
        match &resolved {
            IrType::Integer(IntWidth::I1) => Some(FormatHole::Bool(value.value)),
            IrType::Integer(_) => {
                let unsigned = value.is_unsigned;
                let widened = self.format_widen_int(&value, unsigned);
                Some(if unsigned {
                    FormatHole::U64(widened)
                } else {
                    FormatHole::I64(widened)
                })
            }
            IrType::Float(FloatWidth::F64) => Some(FormatHole::F64(value.value)),
            IrType::Float(FloatWidth::F32) => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Cast {
                    dest: dest.clone(),
                    mode: IrCastMode::Bitcast,
                    value: value.value,
                    ty: IrType::Float(FloatWidth::F64),
                });
                Some(FormatHole::F64(IrValue::Register(dest)))
            }
            IrType::Slice(element)
                if matches!(
                    self.resolve_named_type(element),
                    IrType::Integer(IntWidth::I8)
                ) =>
            {
                Some(FormatHole::Str(value.value))
            }
            IrType::Aggregate(_) => {
                let Some(type_name) = self.receiver_struct_name(&value.ty) else {
                    self.context.error(format!(
                        "cannot format a value of type `{}`; implement the `Format` interface",
                        value.ty
                    ));
                    return None;
                };
                let len_sym = format!("{type_name}_formatted_len");
                let write_sym = format!("{type_name}_format_exact_into");
                if !self.function_param_types.contains_key(&len_sym)
                    || !self.function_param_types.contains_key(&write_sym)
                {
                    self.context.error(format!(
                        "type `{type_name}` does not implement `Format` \
                         (needs `formatted_len` and `format_exact_into` impl methods)"
                    ));
                    return None;
                }
                // Materialize the receiver so the impl methods get a pointer.
                let storage = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: storage.clone(),
                    ty: resolved.clone(),
                    count: None,
                });
                self.push_instruction(IrInstruction::Store {
                    ty: value.ty.clone(),
                    value: value.value,
                    ptr: storage.clone(),
                    offset: None,
                });
                let self_ptr = IrValue::Register(storage);
                let sized = self.emit_format_runtime_call(&len_sym, vec![self_ptr.clone()])?;
                let IrType::Named(sized_enum) = &sized.ty else {
                    self.context.error(format!(
                        "`{len_sym}` must return `Result<u64, FormatError>`, found `{}`",
                        sized.ty
                    ));
                    return None;
                };
                if sized_enum != result_enum {
                    self.context.error(format!(
                        "`{len_sym}` must return `Result<u64, FormatError>`, found `{}`",
                        sized.ty
                    ));
                    return None;
                }
                let len = self.format_unwrap_len(&sized, result_enum, result_ptr, done_label)?;
                Some(FormatHole::Custom {
                    self_ptr,
                    len,
                    write_sym,
                })
            }
            _ => {
                self.context.error(format!(
                    "cannot format a value of type `{}` in a format string",
                    value.ty
                ));
                None
            }
        }
    }

    // Unwrap a `Result<u64, FormatError>` value: on `Err`, forward the whole
    // enum into the result slot and jump to `done`; on `Ok`, yield the length.
    fn format_unwrap_len(
        &mut self,
        result: &LoweredValue,
        result_enum: &str,
        result_ptr: &IrRegister,
        done_label: &crate::ir::IrLabel,
    ) -> Option<IrValue> {
        let enum_ty = self.resolve_named_type(&result.ty);
        let storage = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: storage.clone(),
            ty: enum_ty.clone(),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: result.ty.clone(),
            value: result.value.clone(),
            ptr: storage.clone(),
            offset: None,
        });
        let ok_variant = self.find_enum_variant(result_enum, "Ok")?;
        let ok_layout = self.variant_payload_layout(&ok_variant.payload);
        let [(ok_offset, ok_ty)] = ok_layout.as_slice() else {
            self.context
                .error(format!("invalid `Ok` payload layout for `{result_enum}`"));
            return None;
        };
        let tag = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: tag.clone(),
            ty: IrType::Integer(IntWidth::I64),
            ptr: storage.clone(),
            offset: Some(0),
        });
        let is_ok = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: is_ok.clone(),
            op: IrCmpOp::Eq,
            ty: IrType::Integer(IntWidth::I64),
            lhs: IrValue::Register(tag),
            rhs: IrValue::Integer(crate::conv::usize_to_i64(ok_variant.index)),
        });
        let ok_label = self.new_label("result_ok");
        let err_label = self.new_label("result_err");
        self.set_terminator(IrTerminator::Branch {
            cond: IrValue::Register(is_ok),
            then_label: ok_label.clone(),
            else_label: err_label.clone(),
        });

        // Err: the callee's Result is the same enum type; forward it whole.
        self.start_new_block(err_label.0.clone());
        let whole = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: whole.clone(),
            ty: enum_ty.clone(),
            ptr: storage.clone(),
            offset: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: enum_ty,
            value: IrValue::Register(whole),
            ptr: result_ptr.clone(),
            offset: None,
        });
        self.set_terminator(IrTerminator::Jump(done_label.clone()));

        self.start_new_block(ok_label.0.clone());
        let len = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: len.clone(),
            ty: ok_ty.clone(),
            ptr: storage,
            offset: Some(*ok_offset),
        });
        Some(IrValue::Register(len))
    }

    /// Lower `alias.format_into(dest, "...{x}...")` (spec 5.5): evaluate the
    /// destination then each hole once, size transactionally, then write.
    pub(super) fn lower_format_into(
        &mut self,
        alias: &str,
        dest_expr: &Expression,
        segments: &[FormatSegment],
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        if mode == EvalMode::Address {
            self.context
                .error("the value produced by `format_into` is not addressable".to_owned());
            return None;
        }
        if !self.format_aliases.contains(alias) {
            self.context.error(format!(
                "`{alias}.format_into` requires `{alias} := import(\"string\")`; \
                 only the string module carries the format runtime"
            ));
            return None;
        }
        let Some(IrType::Named(result_enum)) =
            self.function_return_types.get("fmt_result_ok").cloned()
        else {
            self.context.error(
                "string interpolation requires the string module's format runtime".to_owned(),
            );
            return None;
        };
        if matches!(
            dest_expr,
            Expression::Primary(PrimaryExpr::Literal(Literal::String(_)))
        ) {
            self.context.error(
                "format_into destination must be a writable u8[] buffer, not a string literal"
                    .to_owned(),
            );
            return None;
        }

        // Destination evaluates exactly once, before any interpolation expression.
        let dest = self.lower_expr(dest_expr, EvalMode::Value)?;
        let dest_resolved = self.resolve_named_type(&dest.ty);
        let dest_is_bytes = matches!(
            &dest_resolved,
            IrType::Slice(element) if matches!(
                self.resolve_named_type(element),
                IrType::Integer(IntWidth::I8)
            )
        );
        if !dest_is_bytes {
            self.context.error(format!(
                "format_into destination must be `u8[]`, found `{}`",
                dest.ty
            ));
            return None;
        }
        // Materialize the slice so `.len` is loadable for the guard.
        let dest_slot = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest_slot.clone(),
            ty: dest_resolved.clone(),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: dest.ty.clone(),
            value: dest.value.clone(),
            ptr: dest_slot.clone(),
            offset: None,
        });

        // Result storage shared by every early-out path.
        let result_ty = self.resolve_named_type(&IrType::Named(result_enum.clone()));
        if !matches!(result_ty, IrType::Aggregate(_)) {
            self.context.error(format!(
                "format runtime result type `{result_enum}` is not instantiated"
            ));
            return None;
        }
        let result_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: result_ptr.clone(),
            ty: result_ty.clone(),
            count: None,
        });
        let done_label = self.new_label("format_done");

        // Evaluate every hole once, left to right.
        let mut holes = Vec::new();
        let mut chunk_len = 0u64;
        for segment in segments {
            match segment {
                FormatSegment::Literal(text) => {
                    chunk_len = chunk_len.saturating_add(text.len() as u64);
                }
                FormatSegment::Hole(expr) => {
                    holes.push(self.lower_format_hole(
                        expr,
                        &result_enum,
                        &result_ptr,
                        &done_label,
                    )?);
                }
            }
        }

        // Sizing pass: literal bytes plus each hole's formatted length.
        let mut acc = IrValue::Integer(chunk_len as i64);
        for hole in &holes {
            let len = match hole {
                FormatHole::I64(value) => {
                    self.emit_format_runtime_call("fmt_len_i64", vec![value.clone()])?
                        .value
                }
                FormatHole::U64(value) => {
                    self.emit_format_runtime_call("fmt_len_u64", vec![value.clone()])?
                        .value
                }
                FormatHole::F64(value) => {
                    self.emit_format_runtime_call("fmt_len_f64", vec![value.clone()])?
                        .value
                }
                FormatHole::Bool(value) => {
                    self.emit_format_runtime_call("fmt_len_bool", vec![value.clone()])?
                        .value
                }
                FormatHole::Str(value) => {
                    self.emit_format_runtime_call("fmt_len_str", vec![value.clone()])?
                        .value
                }
                FormatHole::Custom { len, .. } => len.clone(),
            };
            acc = self
                .emit_format_runtime_call("fmt_len_add", vec![acc, len])?
                .value;
        }

        // Guards: u64 overflow first, then capacity, before any byte is written.
        let overflow = self.emit_format_runtime_call("fmt_is_len_overflow", vec![acc.clone()])?;
        let overflow_label = self.new_label("format_overflow");
        let size_ok_label = self.new_label("format_size_ok");
        self.set_terminator(IrTerminator::Branch {
            cond: overflow.value,
            then_label: overflow_label.clone(),
            else_label: size_ok_label.clone(),
        });
        self.start_new_block(overflow_label.0.clone());
        let overflow_result = self.emit_format_runtime_call("fmt_result_overflow", Vec::new())?;
        self.push_instruction(IrInstruction::Store {
            ty: overflow_result.ty.clone(),
            value: overflow_result.value,
            ptr: result_ptr.clone(),
            offset: None,
        });
        self.set_terminator(IrTerminator::Jump(done_label.clone()));

        self.start_new_block(size_ok_label.0.clone());
        let dest_len = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: dest_len.clone(),
            ty: IrType::Integer(IntWidth::I64),
            ptr: dest_slot,
            offset: Some(8),
        });
        let too_small = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: too_small.clone(),
            op: IrCmpOp::Ugt,
            ty: IrType::Integer(IntWidth::I64),
            lhs: acc.clone(),
            rhs: IrValue::Register(dest_len),
        });
        let too_small_label = self.new_label("format_too_small");
        let write_label = self.new_label("format_write");
        self.set_terminator(IrTerminator::Branch {
            cond: IrValue::Register(too_small),
            then_label: too_small_label.clone(),
            else_label: write_label.clone(),
        });
        self.start_new_block(too_small_label.0.clone());
        let too_small_result =
            self.emit_format_runtime_call("fmt_result_too_small", vec![acc.clone()])?;
        self.push_instruction(IrInstruction::Store {
            ty: too_small_result.ty.clone(),
            value: too_small_result.value,
            ptr: result_ptr.clone(),
            offset: None,
        });
        self.set_terminator(IrTerminator::Jump(done_label.clone()));

        // Write pass: everything fits, so emit the bytes in template order.
        self.start_new_block(write_label.0.clone());
        let mut offset = IrValue::Integer(0);
        let mut hole_iter = holes.into_iter();
        for segment in segments {
            match segment {
                FormatSegment::Literal(text) => {
                    let chunk = self.lower_literal(&Literal::String(text.clone()));
                    offset = self
                        .emit_format_runtime_call(
                            "fmt_write_bytes",
                            vec![dest.value.clone(), offset, chunk.value],
                        )?
                        .value;
                }
                FormatSegment::Hole(_) => {
                    let hole = hole_iter.next()?;
                    offset = match hole {
                        FormatHole::I64(value) => {
                            self.emit_format_runtime_call(
                                "fmt_write_i64",
                                vec![dest.value.clone(), offset, value],
                            )?
                            .value
                        }
                        FormatHole::U64(value) => {
                            self.emit_format_runtime_call(
                                "fmt_write_u64",
                                vec![dest.value.clone(), offset, value],
                            )?
                            .value
                        }
                        FormatHole::F64(value) => {
                            self.emit_format_runtime_call(
                                "fmt_write_f64",
                                vec![dest.value.clone(), offset, value],
                            )?
                            .value
                        }
                        FormatHole::Bool(value) => {
                            self.emit_format_runtime_call(
                                "fmt_write_bool",
                                vec![dest.value.clone(), offset, value],
                            )?
                            .value
                        }
                        FormatHole::Str(value) => {
                            self.emit_format_runtime_call(
                                "fmt_write_bytes",
                                vec![dest.value.clone(), offset, value],
                            )?
                            .value
                        }
                        FormatHole::Custom {
                            self_ptr,
                            len,
                            write_sym,
                        } => {
                            let sub = self.emit_format_runtime_call(
                                "fmt_subslice",
                                vec![dest.value.clone(), offset.clone(), len.clone()],
                            )?;
                            let written = {
                                let dest_reg = self.new_temp();
                                self.push_instruction(IrInstruction::Call {
                                    dest: Some(dest_reg.clone()),
                                    function: write_sym,
                                    args: vec![self_ptr, sub.value],
                                });
                                IrValue::Register(dest_reg)
                            };
                            self.emit_format_runtime_call(
                                "fmt_exact_check",
                                vec![len.clone(), written],
                            )?;
                            let advanced = self.new_temp();
                            self.push_instruction(IrInstruction::Math {
                                dest: advanced.clone(),
                                op: IrMathOp::Add,
                                ty: IrType::Integer(IntWidth::I64),
                                lhs: offset,
                                rhs: len,
                            });
                            IrValue::Register(advanced)
                        }
                    };
                }
            }
        }
        let ok_result = self.emit_format_runtime_call("fmt_result_ok", vec![acc])?;
        self.push_instruction(IrInstruction::Store {
            ty: ok_result.ty.clone(),
            value: ok_result.value,
            ptr: result_ptr.clone(),
            offset: None,
        });
        self.set_terminator(IrTerminator::Jump(done_label.clone()));

        self.start_new_block(done_label.0.clone());
        let final_value = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: final_value.clone(),
            ty: result_ty,
            ptr: result_ptr,
            offset: None,
        });
        Some(LoweredValue {
            value: IrValue::Register(final_value),
            ty: IrType::Named(result_enum),
            is_unsigned: false,
        })
    }

    fn lower_field_access_expr(
        &mut self,
        expr: &Expression,
        field: &str,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        let base_addr = self.lower_expr(expr, EvalMode::Address)?;
        self.lower_field_access_mode(&base_addr, field, mode)
    }

    pub(super) fn lower_field_access_mode(
        &mut self,
        base_addr: &LoweredValue,
        field: &str,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        // Valid base shapes: Aggregate/Slice, Pointer to either, or
        // Pointer(Pointer(Aggregate)) via the shared storage unwrap.
        let (agg_ptr, fields) = {
            let (ptr, storage_ty) = self.resolve_aggregate_ptr_and_fields(base_addr, |t| {
                matches!(t, IrType::Aggregate(_))
            })?;
            match storage_ty {
                IrType::Aggregate(fields) => (ptr, fields),
                // A slice exposes `ptr` and `len` over its { ptr, len } layout.
                IrType::Slice(elem) => (ptr, Self::slice_fields(&elem)),
                _ => return None,
            }
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
        let base = self.lower_expr(expr, EvalMode::Address)?;
        let idx = self.lower_expr(index, EvalMode::Value)?;
        self.lower_array_index_mode(&base, &idx, mode)
    }

    pub(super) fn lower_array_index_mode(
        &mut self,
        base: &LoweredValue,
        index: &LoweredValue,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        let resolved_ty = self.resolve_named_type(&base.ty);

        // Slice operand: index through the { ptr, len } fat pointer. base.value is
        // the address of the slice storage (a place or a slice value's slot).
        let slice_elem = match &resolved_ty {
            IrType::Slice(elem) => Some((**elem).clone()),
            IrType::Pointer(inner) => match self.resolve_named_type(inner) {
                IrType::Slice(elem) => Some(*elem),
                _ => None,
            },
            _ => None,
        };
        if let Some(elem_ty) = slice_elem {
            let IrValue::Register(slot) = base.value.clone() else {
                return None;
            };
            return self.lower_slice_index_mode(&slot, &elem_ty, index, mode);
        }

        // Find the pointer that addresses the first element and the element type.
        let (indexable_ptr, element_ty) = {
            if !matches!(&resolved_ty, IrType::Array { .. } | IrType::Pointer(_)) {
                return None;
            }
            let (ptr, storage_ty) = self.resolve_aggregate_ptr_and_fields(base, |_| true)?;
            let element_ty = match storage_ty {
                IrType::Array { element, .. } => *element,
                // Pointer to T: base.value addresses elements of type T.
                other => other,
            };
            (ptr, element_ty)
        };

        let elem_ptr = self.new_temp();
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

    // Unwrap a base (T, Pointer(T), or Pointer(Pointer(T))) to its storage ptr and
    // resolved type; a double pointer loads the slot first if `accept_double(T)`.
    fn resolve_aggregate_ptr_and_fields(
        &mut self,
        base: &LoweredValue,
        accept_double: impl Fn(&IrType) -> bool,
    ) -> Option<(crate::ir::IrRegister, IrType)> {
        let reg = match &base.value {
            IrValue::Register(r) => r.clone(),
            _ => return None,
        };
        match self.resolve_named_type(&base.ty) {
            IrType::Pointer(inner) => match self.resolve_named_type(&inner) {
                IrType::Pointer(inner_inner) => {
                    let target = self.resolve_named_type(&inner_inner);
                    if !accept_double(&target) {
                        return None;
                    }
                    let loaded = self.new_temp();
                    self.push_instruction(IrInstruction::Load {
                        dest: loaded.clone(),
                        ty: *inner.clone(),
                        ptr: reg,
                        offset: None,
                    });
                    Some((loaded, target))
                }
                inner_resolved => Some((reg, inner_resolved)),
            },
            other => Some((reg, other)),
        }
    }

    // The { ptr, len } fields of a slice, used for `.ptr` / `.len` access.
    fn slice_fields(elem: &IrType) -> Vec<(String, IrType)> {
        vec![
            ("ptr".to_owned(), IrType::Pointer(Box::new(elem.clone()))),
            ("len".to_owned(), IrType::Integer(IntWidth::I64)),
        ]
    }

    // Index a slice: load { ptr, len } from the fat pointer, bounds-check the
    // index against len, then compute the element address.
    fn lower_slice_index_mode(
        &mut self,
        slot: &crate::ir::IrRegister,
        elem_ty: &IrType,
        index: &LoweredValue,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        let data_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: data_ptr.clone(),
            ty: IrType::Pointer(Box::new(elem_ty.clone())),
            ptr: slot.clone(),
            offset: Some(0),
        });
        let len = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: len.clone(),
            ty: IrType::Integer(IntWidth::I64),
            ptr: slot.clone(),
            offset: Some(8),
        });
        self.emit_bounds_check(&index.value, &IrValue::Register(len));

        let elem_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Index {
            dest: elem_ptr.clone(),
            ty: elem_ty.clone(),
            base_ptr: data_ptr,
            idx: index.value.clone(),
        });

        match mode {
            EvalMode::Address => Some(LoweredValue {
                value: IrValue::Register(elem_ptr),
                ty: IrType::Pointer(Box::new(elem_ty.clone())),
                is_unsigned: false,
            }),
            EvalMode::Value => {
                let dest = self.new_temp();
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: elem_ty.clone(),
                    ptr: elem_ptr,
                    offset: None,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: elem_ty.clone(),
                    is_unsigned: false,
                })
            }
        }
    }

    // `assert(cond)`: trap with a stable diagnostic code when `cond` is false.
    fn lower_assert(&mut self, arguments: &[Expression]) -> Option<LoweredValue> {
        let [condition] = arguments else {
            self.context.error(format!(
                "assert(cond) expects exactly one argument, got {}",
                arguments.len()
            ));
            return None;
        };
        let condition = self.lower_expr(condition, EvalMode::Value)?;
        let failed = self.new_temp();
        self.push_instruction(IrInstruction::Unary {
            dest: failed.clone(),
            op: crate::ir::IrUnaryOp::Not,
            ty: condition.ty,
            value: condition.value,
        });
        self.emit_trap_if(IrValue::Register(failed), ASSERT_TRAP_CODE);
        Some(LoweredValue {
            value: IrValue::Null,
            ty: IrType::Void,
            is_unsigned: false,
        })
    }

    // Trap when `idx >= len` (unsigned, so a negative index is out of bounds too).
    fn emit_bounds_check(&mut self, idx: &IrValue, len: &IrValue) {
        let oob = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: oob.clone(),
            op: crate::ir::IrCmpOp::Uge,
            ty: IrType::Integer(IntWidth::I64),
            lhs: idx.clone(),
            rhs: len.clone(),
        });
        self.emit_trap_if(IrValue::Register(oob), SLICE_BOUNDS_TRAP_CODE);
    }

    // Trap (coded exit) when `cond` is true, splitting the current block. Phi
    // correctness holds: the backend reconciles values by shared storage.
    fn emit_trap_if(&mut self, cond: IrValue, code: u32) {
        let trap_label = self.new_label("trap");
        let ok_label = self.new_label("trap_ok");
        self.set_terminator(crate::ir::IrTerminator::Branch {
            cond,
            then_label: trap_label.clone(),
            else_label: ok_label.clone(),
        });

        self.start_new_block(trap_label.0.clone());
        self.set_terminator(crate::ir::IrTerminator::Trap { code });

        self.start_new_block(ok_label.0.clone());
    }

    // Build the data pointer, element type, and length of a sliceable base. The
    // base must be an array or a slice; raw pointers have no known length.
    fn slice_base_parts(
        &mut self,
        base: &LoweredValue,
    ) -> Option<(crate::ir::IrRegister, IrType, IrValue)> {
        let resolved = self.resolve_named_type(&base.ty);
        let IrValue::Register(slot) = base.value.clone() else {
            return None;
        };

        // Load { ptr, len } out of a slice fat pointer stored at `slot`.
        let load_slice = |this: &mut Self, elem: IrType| {
            let ptr = this.new_temp();
            this.push_instruction(IrInstruction::Load {
                dest: ptr.clone(),
                ty: IrType::Pointer(Box::new(elem.clone())),
                ptr: slot.clone(),
                offset: Some(0),
            });
            let len = this.new_temp();
            this.push_instruction(IrInstruction::Load {
                dest: len.clone(),
                ty: IrType::Integer(IntWidth::I64),
                ptr: slot.clone(),
                offset: Some(8),
            });
            (ptr, elem, IrValue::Register(len))
        };

        match resolved {
            IrType::Array { element, len } => Some((slot, *element, IrValue::Integer(len as i64))),
            IrType::Slice(elem) => Some(load_slice(self, *elem)),
            IrType::Pointer(inner) => match self.resolve_named_type(&inner) {
                IrType::Array { element, len } => {
                    Some((slot, *element, IrValue::Integer(len as i64)))
                }
                IrType::Slice(elem) => Some(load_slice(self, *elem)),
                _ => {
                    self.context
                        .error("only arrays and slices can be range-sliced".to_owned());
                    None
                }
            },
            _ => {
                self.context
                    .error("only arrays and slices can be range-sliced".to_owned());
                None
            }
        }
    }

    // `base[start..end]` yields `{ &base[start], end - start }`. Open endpoints default to 0
    // and the base length, `..=` adds one to `end`, and `end > len` or `start > end` traps.
    fn lower_slice_expr(
        &mut self,
        base_expr: &Expression,
        start: Option<&Expression>,
        end: Option<&Expression>,
        inclusive: bool,
    ) -> Option<LoweredValue> {
        let base = self.lower_expr(base_expr, EvalMode::Address)?;
        let (data_ptr, elem_ty, len_val) = self.slice_base_parts(&base)?;

        let i64_ty = IrType::Integer(IntWidth::I64);
        let start_val = match start {
            Some(expr) => self.lower_expr(expr, EvalMode::Value)?.value,
            None => IrValue::Integer(0),
        };
        let end_raw = match end {
            Some(expr) => self.lower_expr(expr, EvalMode::Value)?.value,
            None => len_val.clone(),
        };
        // `..=` is inclusive only when an explicit end is given.
        let end_val = if inclusive && end.is_some() {
            let t = self.new_temp();
            self.push_instruction(IrInstruction::Math {
                dest: t.clone(),
                op: crate::ir::IrMathOp::Add,
                ty: i64_ty.clone(),
                lhs: end_raw,
                rhs: IrValue::Integer(1),
            });
            IrValue::Register(t)
        } else {
            end_raw
        };

        // end > len  ->  trap
        let end_oob = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: end_oob.clone(),
            op: crate::ir::IrCmpOp::Ugt,
            ty: i64_ty.clone(),
            lhs: end_val.clone(),
            rhs: len_val,
        });
        self.emit_trap_if(IrValue::Register(end_oob), SLICE_BOUNDS_TRAP_CODE);
        // start > end  ->  trap
        let start_oob = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: start_oob.clone(),
            op: crate::ir::IrCmpOp::Ugt,
            ty: i64_ty.clone(),
            lhs: start_val.clone(),
            rhs: end_val.clone(),
        });
        self.emit_trap_if(IrValue::Register(start_oob), SLICE_BOUNDS_TRAP_CODE);

        // new length = end - start
        let new_len = self.new_temp();
        self.push_instruction(IrInstruction::Math {
            dest: new_len.clone(),
            op: crate::ir::IrMathOp::Sub,
            ty: i64_ty.clone(),
            lhs: end_val,
            rhs: start_val.clone(),
        });
        // new data pointer = &base[start] (element-scaled)
        let new_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Index {
            dest: new_ptr.clone(),
            ty: elem_ty.clone(),
            base_ptr: data_ptr,
            idx: start_val,
        });

        let slice_ty = IrType::Slice(Box::new(elem_ty.clone()));
        let slot = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: slot.clone(),
            ty: slice_ty.clone(),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: IrType::Pointer(Box::new(elem_ty)),
            value: IrValue::Register(new_ptr),
            ptr: slot.clone(),
            offset: Some(0),
        });
        self.push_instruction(IrInstruction::Store {
            ty: i64_ty,
            value: IrValue::Register(new_len),
            ptr: slot.clone(),
            offset: Some(8),
        });
        Some(LoweredValue {
            value: IrValue::Register(slot),
            ty: slice_ty,
            is_unsigned: false,
        })
    }

    fn lower_unary_expr(
        &mut self,
        op: &UnaryOp,
        expr: &Expression,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        match op {
            UnaryOp::AddressOf => {
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
                        let ptr_reg = if let IrValue::Register(r) = ptr_val.value {
                            r
                        } else {
                            self.context
                                .error("cannot dereference non-register value".to_owned());
                            return None;
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

    // Bit width of an integer type, for cast narrowing/widening decisions.
    fn int_width_bits(width: IntWidth) -> u32 {
        match width {
            IntWidth::I1 => 1,
            IntWidth::I8 => 8,
            IntWidth::I16 => 16,
            IntWidth::I32 => 32,
            IntWidth::I64 => 64,
        }
    }

    fn lower_cast(&mut self, target_ty: &Type, expr: &Expression) -> Option<LoweredValue> {
        let source_value = self.lower_expr(expr, EvalMode::Value)?;
        let target_ir = self.lower_type(target_ty);

        let source_resolved = self.resolve_named_type(&source_value.ty);
        let target_resolved = self.resolve_named_type(&target_ir);

        let cast_mode = match (&source_resolved, &target_resolved) {
            (IrType::Integer(src_width), IrType::Integer(tgt_width)) => {
                let src_bits = Self::int_width_bits(*src_width);
                let tgt_bits = Self::int_width_bits(*tgt_width);
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

        // Resolve the target address first so the rvalue can take the slot's width.
        let target_expr = Self::assign_target_to_expression(target)?;
        let addr = self.lower_expr(&target_expr, EvalMode::Address)?;

        let (ptr_reg, store_ty) =
            if let (IrValue::Register(reg), IrType::Pointer(inner)) = (&addr.value, &addr.ty) {
                (reg.clone(), *inner.clone())
            } else {
                self.context
                    .error("assignment target did not resolve to a register address".to_owned());
                return None;
            };

        let lowered = self.lower_value_for_type(rvalue, &store_ty)?;

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
