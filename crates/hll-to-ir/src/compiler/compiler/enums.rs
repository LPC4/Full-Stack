use super::{
    ENUM_PAYLOAD_BASE, EnumLayout, EvalMode, HighLevelCompiler, IntWidth, IrInstruction, IrProgram,
    IrRegister, IrTerminator, IrType, IrTypeAlias, IrValue, LoweredValue, VariantInfo,
};
use crate::ast::{Expression, Literal, MatchArm, Pattern, Type, Variant};
use crate::conv::usize_to_i64;
use crate::ir::IrCmpOp;

// Diagnostic exit code for an unmatched (non-exhaustive) enum value.
const MATCH_TRAP_CODE: u32 = 134;

impl HighLevelCompiler {
    pub(super) fn lower_try(
        &mut self,
        expression: &Expression,
        mode: EvalMode,
    ) -> Option<LoweredValue> {
        if mode == EvalMode::Address {
            self.context
                .error("the value produced by `?` is not addressable".to_owned());
            return None;
        }

        let (enum_ptr, enum_name) =
            if let Some(address) = self.lower_expr(expression, EvalMode::Address) {
                let IrType::Pointer(inner) = &address.ty else {
                    return None;
                };
                let IrType::Named(name) = inner.as_ref() else {
                    return None;
                };
                let IrValue::Register(ptr) = address.value else {
                    return None;
                };
                (ptr, name.clone())
            } else {
                let value = self.lower_expr(expression, EvalMode::Value)?;
                let IrType::Named(name) = &value.ty else {
                    return None;
                };
                let name = name.clone();
                // Aggregate-producing IR values live in backend value slots, not pointer
                // registers. Materialize one addressable copy before reading the tag/payload.
                let storage = self.new_temp();
                self.push_instruction(IrInstruction::Alloc {
                    dest: storage.clone(),
                    ty: value.ty.clone(),
                    count: None,
                });
                self.push_instruction(IrInstruction::Store {
                    ty: value.ty,
                    value: value.value,
                    ptr: storage.clone(),
                    offset: None,
                });
                (storage, name)
            };

        let (success_prefix, failure_prefix) = if enum_name.starts_with("Result__") {
            ("Ok", "Err")
        } else if enum_name.starts_with("Option__") {
            ("Some", "None")
        } else {
            self.context.error(format!(
                "`?` requires `Result<T, E>` or `Option<T>`, found `{enum_name}`"
            ));
            return None;
        };
        let success = self.find_enum_variant(&enum_name, success_prefix)?;
        let failure = self.find_enum_variant(&enum_name, failure_prefix)?;
        let success_layout = self.variant_payload_layout(&success.payload);
        let [(success_offset, success_ty)] = success_layout.as_slice() else {
            self.context.error(format!(
                "invalid `{success_prefix}` payload layout for `{enum_name}`"
            ));
            return None;
        };

        let tag = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: tag.clone(),
            ty: IrType::Integer(IntWidth::I64),
            ptr: enum_ptr.clone(),
            offset: Some(0),
        });
        let is_success = self.new_temp();
        self.push_instruction(IrInstruction::Cmp {
            dest: is_success.clone(),
            op: IrCmpOp::Eq,
            ty: IrType::Integer(IntWidth::I64),
            lhs: IrValue::Register(tag),
            rhs: IrValue::Integer(usize_to_i64(success.index)),
        });
        let success_label = self.new_label();
        let failure_label = self.new_label();
        self.set_terminator(IrTerminator::Branch {
            cond: IrValue::Register(is_success),
            then_label: success_label.clone(),
            else_label: failure_label.clone(),
        });

        self.start_new_block(failure_label.0.clone());
        let return_enum = if let Some(IrType::Named(name)) = self.current_return_ty.clone() {
            name
        } else {
            self.context
                .error("`?` requires an enum return type".to_owned());
            return None;
        };
        let return_failure = self.find_enum_variant(&return_enum, failure_prefix)?;
        let return_layout = self.enum_layouts.get(&return_enum).cloned()?;
        let return_ptr = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: return_ptr.clone(),
            ty: self.enum_ir_type(return_layout.payload_bytes),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: IrType::Integer(IntWidth::I64),
            value: IrValue::Integer(usize_to_i64(return_failure.index)),
            ptr: return_ptr.clone(),
            offset: Some(0),
        });
        let source_failure_layout = self.variant_payload_layout(&failure.payload);
        let target_failure_layout = self.variant_payload_layout(&return_failure.payload);
        for ((source_offset, source_ty), (target_offset, target_ty)) in
            source_failure_layout.into_iter().zip(target_failure_layout)
        {
            let value = self.new_temp();
            self.push_instruction(IrInstruction::Load {
                dest: value.clone(),
                ty: source_ty,
                ptr: enum_ptr.clone(),
                offset: Some(source_offset),
            });
            self.push_instruction(IrInstruction::Store {
                ty: target_ty,
                value: IrValue::Register(value),
                ptr: return_ptr.clone(),
                offset: Some(target_offset),
            });
        }
        self.set_terminator(IrTerminator::Return(Some(IrValue::Register(return_ptr))));

        self.start_new_block(success_label.0.clone());
        let value = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: value.clone(),
            ty: success_ty.clone(),
            ptr: enum_ptr,
            offset: Some(*success_offset),
        });
        Some(LoweredValue {
            value: IrValue::Register(value),
            ty: success_ty.clone(),
            is_unsigned: Self::is_unsigned_primitive_type(&success.payload[0]),
        })
    }

    fn find_enum_variant(&self, enum_name: &str, prefix: &str) -> Option<VariantInfo> {
        self.enum_variants
            .iter()
            .find(|(name, info)| {
                info.enum_name == enum_name
                    && (name.as_str() == prefix || name.starts_with(&format!("{prefix}__")))
            })
            .map(|(_, info)| info.clone())
    }

    // Compute an enum's runtime layout and record its variant constructors.
    pub(super) fn register_enum(
        &mut self,
        ir_program: &mut IrProgram,
        name: &str,
        variants: &[Variant],
    ) {
        let payload_bytes = self.enum_payload_bytes(variants);
        let ir_ty = self.enum_ir_type(payload_bytes);
        self.context
            .types
            .register_type(name.to_owned(), ir_ty.clone());
        ir_program.push_type_alias(IrTypeAlias {
            name: name.to_owned(),
            ty: ir_ty,
        });

        for (index, variant) in variants.iter().enumerate() {
            if self.enum_variants.contains_key(&variant.name) {
                self.context.error(format!(
                    "duplicate enum variant constructor `{}`",
                    variant.name
                ));
            }
            self.enum_variants.insert(
                variant.name.clone(),
                VariantInfo {
                    enum_name: name.to_owned(),
                    index,
                    payload: variant.payload.clone(),
                },
            );
        }
        self.enum_layouts
            .insert(name.to_owned(), EnumLayout { payload_bytes });
    }

    // Largest variant payload size, rounded up to 8 (the payload area is 8-aligned).
    fn enum_payload_bytes(&mut self, variants: &[Variant]) -> usize {
        let mut max = 0i64;
        for variant in variants {
            let mut offset = 0i64;
            for ty in &variant.payload {
                let ir_ty = self.lower_type(ty);
                let align = usize_to_i64(self.type_alignment_in_bytes(&ir_ty));
                offset = Self::align_to(offset, align);
                offset += usize_to_i64(self.type_size_in_bytes(&ir_ty));
            }
            max = max.max(offset);
        }
        Self::align_to(max, 8) as usize
    }

    // `{ tag: i64, payload: u8[N] }`; the payload field is omitted when N is 0.
    fn enum_ir_type(&self, payload_bytes: usize) -> IrType {
        let mut fields = vec![("tag".to_owned(), IrType::Integer(IntWidth::I64))];
        if payload_bytes > 0 {
            fields.push((
                "payload".to_owned(),
                IrType::Array {
                    len: payload_bytes,
                    element: Box::new(IrType::Integer(IntWidth::I8)),
                },
            ));
        }
        IrType::Aggregate(fields)
    }

    // Absolute byte offset + IR type of each payload slot, relative to the enum base.
    fn variant_payload_layout(&mut self, payload: &[Type]) -> Vec<(i64, IrType)> {
        let mut out = Vec::with_capacity(payload.len());
        let mut offset = 0i64;
        for ty in payload {
            let ir_ty = self.lower_type(ty);
            let align = usize_to_i64(self.type_alignment_in_bytes(&ir_ty));
            offset = Self::align_to(offset, align);
            out.push((ENUM_PAYLOAD_BASE + offset, ir_ty.clone()));
            offset += usize_to_i64(self.type_size_in_bytes(&ir_ty));
        }
        out
    }

    // Build an enum value: allocate the aggregate, store the tag, then each payload.
    pub(super) fn lower_enum_construct(
        &mut self,
        ctor: &str,
        args: &[Expression],
    ) -> Option<LoweredValue> {
        let info = self.enum_variants.get(ctor).cloned()?;
        if args.len() != info.payload.len() {
            self.context.error(format!(
                "enum variant `{ctor}` expects {} payload value(s), got {}",
                info.payload.len(),
                args.len()
            ));
            return None;
        }
        let layout = self.enum_layouts.get(&info.enum_name).cloned()?;
        let enum_ty = self.enum_ir_type(layout.payload_bytes);

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: dest.clone(),
            ty: enum_ty,
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: IrType::Integer(IntWidth::I64),
            value: IrValue::Integer(usize_to_i64(info.index)),
            ptr: dest.clone(),
            offset: Some(0),
        });

        let slots = self.variant_payload_layout(&info.payload);
        for (arg, (offset, field_ty)) in args.iter().zip(slots) {
            let value = self.lower_value_for_type(arg, &field_ty)?;
            self.push_instruction(IrInstruction::Store {
                ty: field_ty,
                value: value.value,
                ptr: dest.clone(),
                offset: Some(offset),
            });
        }

        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: IrType::Named(info.enum_name),
            is_unsigned: false,
        })
    }

    // Resolve the scrutinee to a pointer at the enum aggregate's bytes. An
    // enum-by-value rvalue (a call result) is spilled to a slot to be addressable.
    fn enum_scrutinee_ptr(&mut self, scrutinee: &Expression) -> Option<IrRegister> {
        if let Some(base) = self.lower_expr(scrutinee, EvalMode::Address)
            && let IrValue::Register(reg) = base.value
        {
            return Some(reg);
        }
        let base = self.lower_expr(scrutinee, EvalMode::Value)?;
        let IrValue::Register(reg) = base.value else {
            return None;
        };
        if matches!(self.resolve_named_type(&base.ty), IrType::Aggregate(_)) {
            let slot = self.new_temp();
            self.push_instruction(IrInstruction::Alloc {
                dest: slot.clone(),
                ty: base.ty.clone(),
                count: None,
            });
            self.push_instruction(IrInstruction::Store {
                ty: base.ty,
                value: IrValue::Register(reg),
                ptr: slot.clone(),
                offset: None,
            });
            return Some(slot);
        }
        Some(reg)
    }

    // Lower a `match` in statement position: arms run for effect and any value
    // expressions are discarded.
    pub(super) fn lower_match(
        &mut self,
        ir_program: &mut IrProgram,
        scrutinee: &Expression,
        arms: &[MatchArm],
    ) {
        self.lower_match_core(ir_program, scrutinee, arms, None);
    }

    // Lower a value-producing `match` into a fresh result slot, returning the
    // loaded value. `target_ty` None (inferred `:=`) is discovered and back-patched.
    pub(super) fn lower_match_value(
        &mut self,
        ir_program: &mut IrProgram,
        scrutinee: &Expression,
        arms: &[MatchArm],
        target_ty: Option<IrType>,
    ) -> Option<LoweredValue> {
        let slot = IrRegister::Named(format!("__match_res{}", self.match_id));
        let placeholder = target_ty.clone().unwrap_or(IrType::Integer(IntWidth::I64));
        self.push_instruction(IrInstruction::Alloc {
            dest: slot.clone(),
            ty: placeholder,
            count: None,
        });

        let discovered = self.lower_match_core(ir_program, scrutinee, arms, Some(slot.clone()));
        let result_ty = target_ty
            .clone()
            .or(discovered)
            .unwrap_or(IrType::Integer(IntWidth::I64));
        if target_ty.is_none() {
            self.patch_alloc_ty(&slot, &result_ty);
        }

        let dest = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: dest.clone(),
            ty: result_ty.clone(),
            ptr: slot,
            offset: None,
        });
        Some(LoweredValue {
            value: IrValue::Register(dest),
            ty: result_ty,
            is_unsigned: false,
        })
    }

    // Set the element type of the `Alloc` writing `slot`, searching the open block
    // first and then the finished blocks. Used to back-patch an inferred slot.
    fn patch_alloc_ty(&mut self, slot: &IrRegister, ty: &IrType) {
        let patch = |block: &mut crate::ir::IrBlock| {
            for inst in &mut block.instructions {
                if let IrInstruction::Alloc {
                    dest, ty: alloc_ty, ..
                } = inst
                    && dest == slot
                {
                    *alloc_ty = ty.clone();
                    return true;
                }
            }
            false
        };
        if let Some(block) = self.current_block.as_mut()
            && patch(block)
        {
            return;
        }
        for block in self.current_blocks.iter_mut().rev() {
            if patch(block) {
                return;
            }
        }
    }

    // Lower a `match` to a tag load and per-arm compare/branch chain. With a
    // `result_slot`, each arm's value is stored into it (returning the store type).
    fn lower_match_core(
        &mut self,
        ir_program: &mut IrProgram,
        scrutinee: &Expression,
        arms: &[MatchArm],
        result_slot: Option<IrRegister>,
    ) -> Option<IrType> {
        // Literal or integer-const arms drive a scalar value match; otherwise the
        // scrutinee is an enum matched on its tag.
        if arms.iter().any(|a| self.is_scalar_pattern(&a.pattern)) {
            return self.lower_match_scalar(ir_program, scrutinee, arms, result_slot);
        }
        let Some(enum_ptr) = self.enum_scrutinee_ptr(scrutinee) else {
            self.context
                .error("`match` scrutinee must be an enum value".to_owned());
            return None;
        };
        let mut store_ty: Option<IrType> = None;

        let tag = self.new_temp();
        self.push_instruction(IrInstruction::Load {
            dest: tag.clone(),
            ty: IrType::Integer(IntWidth::I64),
            ptr: enum_ptr.clone(),
            offset: Some(0),
        });

        let merge_label = self.new_label();
        let pre_env = self.context.snapshot_env();
        let match_id = self.match_id;
        self.match_id += 1;

        let mut saw_catch_all = false;
        for arm in arms {
            match &arm.pattern {
                Pattern::Wildcard | Pattern::Binding(_) => {
                    self.context.restore_env(pre_env.clone());
                    self.context.symbols.enter_scope();
                    if let Pattern::Binding(name) = &arm.pattern {
                        // Bind the whole enum value (its address) under `name`.
                        self.context.symbols.insert(
                            name.clone(),
                            IrType::Pointer(Box::new(IrType::Integer(IntWidth::I64))),
                            IrValue::Register(enum_ptr.clone()),
                        );
                    }
                    self.lower_block(ir_program, &arm.body);
                    self.store_arm_value(&arm.value, &result_slot, &mut store_ty);
                    self.set_terminator(IrTerminator::Jump(merge_label.clone()));
                    self.context.symbols.exit_scope();
                    saw_catch_all = true;
                    break;
                }
                Pattern::Variant {
                    variant, bindings, ..
                } => {
                    let Some(info) = self.enum_variants.get(variant).cloned() else {
                        self.context
                            .error(format!("unknown enum variant `{variant}` in match arm"));
                        return None;
                    };
                    if bindings.len() != info.payload.len() {
                        self.context.error(format!(
                            "variant `{variant}` binds {} value(s) but has {}",
                            bindings.len(),
                            info.payload.len()
                        ));
                        return None;
                    }

                    let cond = self.new_temp();
                    self.push_instruction(IrInstruction::Cmp {
                        dest: cond.clone(),
                        op: IrCmpOp::Eq,
                        ty: IrType::Integer(IntWidth::I64),
                        lhs: IrValue::Register(tag.clone()),
                        rhs: IrValue::Integer(usize_to_i64(info.index)),
                    });

                    let then_label = self.new_label();
                    let else_label = self.new_label();
                    self.set_terminator(IrTerminator::Branch {
                        cond: IrValue::Register(cond),
                        then_label: then_label.clone(),
                        else_label: else_label.clone(),
                    });

                    self.start_new_block(then_label.0.clone());
                    self.context.restore_env(pre_env.clone());
                    self.context.symbols.enter_scope();
                    self.bind_variant_payload(&enum_ptr, &info, bindings, match_id);
                    self.lower_block(ir_program, &arm.body);
                    self.store_arm_value(&arm.value, &result_slot, &mut store_ty);
                    self.set_terminator(IrTerminator::Jump(merge_label.clone()));
                    self.context.symbols.exit_scope();

                    self.start_new_block(else_label.0.clone());
                }
                Pattern::Literal(_) => {
                    // A literal arm on an enum scrutinee; scalar dispatch handles
                    // pure-literal matches, so this is a type error.
                    self.context
                        .error("literal pattern in a `match` on an enum value".to_owned());
                    return None;
                }
            }
        }

        if !saw_catch_all {
            // Exhaustiveness is enforced in semantic analysis; this guards the
            // residual fall-through path defensively.
            self.set_terminator(IrTerminator::Trap {
                code: MATCH_TRAP_CODE,
            });
        }

        self.start_new_block(merge_label.0.clone());
        self.context.restore_env(pre_env);
        store_ty
    }

    // Lower a `match` whose arms are integer/char literals. The scrutinee is
    // evaluated once to a value and each literal arm compares against it.
    fn lower_match_scalar(
        &mut self,
        ir_program: &mut IrProgram,
        scrutinee: &Expression,
        arms: &[MatchArm],
        result_slot: Option<IrRegister>,
    ) -> Option<IrType> {
        let value = self.lower_expr(scrutinee, EvalMode::Value)?;
        let cmp_ty = match self.resolve_named_type(&value.ty) {
            IrType::Integer(w) => IrType::Integer(w),
            other => {
                self.context.error(format!(
                    "literal `match` patterns require an integer scrutinee, found {other:?}"
                ));
                return None;
            }
        };
        let mut store_ty: Option<IrType> = None;
        let merge_label = self.new_label();
        let pre_env = self.context.snapshot_env();

        let mut saw_catch_all = false;
        for arm in arms {
            match &arm.pattern {
                Pattern::Wildcard | Pattern::Binding(_) => {
                    self.context.restore_env(pre_env.clone());
                    self.context.symbols.enter_scope();
                    if let Pattern::Binding(name) = &arm.pattern {
                        self.bind_scalar_value(name, &value.value, &cmp_ty);
                    }
                    self.lower_block(ir_program, &arm.body);
                    self.store_arm_value(&arm.value, &result_slot, &mut store_ty);
                    self.set_terminator(IrTerminator::Jump(merge_label.clone()));
                    self.context.symbols.exit_scope();
                    saw_catch_all = true;
                    break;
                }
                Pattern::Literal(_) | Pattern::Variant { .. } => {
                    let rhs = self.scalar_pattern_value(&arm.pattern)?;
                    let cond = self.new_temp();
                    self.push_instruction(IrInstruction::Cmp {
                        dest: cond.clone(),
                        op: IrCmpOp::Eq,
                        ty: cmp_ty.clone(),
                        lhs: value.value.clone(),
                        rhs,
                    });
                    let then_label = self.new_label();
                    let else_label = self.new_label();
                    self.set_terminator(IrTerminator::Branch {
                        cond: IrValue::Register(cond),
                        then_label: then_label.clone(),
                        else_label: else_label.clone(),
                    });

                    self.start_new_block(then_label.0.clone());
                    self.context.restore_env(pre_env.clone());
                    self.context.symbols.enter_scope();
                    self.lower_block(ir_program, &arm.body);
                    self.store_arm_value(&arm.value, &result_slot, &mut store_ty);
                    self.set_terminator(IrTerminator::Jump(merge_label.clone()));
                    self.context.symbols.exit_scope();

                    self.start_new_block(else_label.0.clone());
                }
            }
        }

        if !saw_catch_all {
            // A scalar match cannot be exhaustive; semantic analysis requires a
            // catch-all, so this fall-through only guards a residual path.
            self.set_terminator(IrTerminator::Trap {
                code: MATCH_TRAP_CODE,
            });
        }

        self.start_new_block(merge_label.0.clone());
        self.context.restore_env(pre_env);
        store_ty
    }

    // Bind a scalar catch-all name to a fresh slot holding the scrutinee value,
    // following the normal pointer-to-slot local convention.
    fn bind_scalar_value(&mut self, name: &str, value: &IrValue, ty: &IrType) {
        let slot = self.new_temp();
        self.push_instruction(IrInstruction::Alloc {
            dest: slot.clone(),
            ty: ty.clone(),
            count: None,
        });
        self.push_instruction(IrInstruction::Store {
            ty: ty.clone(),
            value: value.clone(),
            ptr: slot.clone(),
            offset: None,
        });
        self.context.symbols.insert(
            name.to_owned(),
            IrType::Pointer(Box::new(ty.clone())),
            IrValue::Register(slot),
        );
    }

    // True for arm patterns that match a scalar by value: a literal, or a bare
    // name that is an integer const rather than an enum variant.
    fn is_scalar_pattern(&self, pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Literal(_) => true,
            Pattern::Variant {
                variant, bindings, ..
            } => {
                bindings.is_empty()
                    && !self.enum_variants.contains_key(variant)
                    && self.compile_time_consts.contains_key(variant)
            }
            _ => false,
        }
    }

    // Resolve a non-catch-all scalar arm pattern to its compared integer value,
    // folding a named integer const when the pattern is a bare name.
    fn scalar_pattern_value(&mut self, pattern: &Pattern) -> Option<IrValue> {
        match pattern {
            Pattern::Literal(lit) => Self::literal_match_value(lit).or_else(|| {
                self.context
                    .error("`match` literal pattern must be an integer or char".to_owned());
                None
            }),
            Pattern::Variant {
                variant, bindings, ..
            } if bindings.is_empty() => {
                let Some(lit) = self.compile_time_consts.get(variant).cloned() else {
                    self.context
                        .error(format!("`{variant}` is not an integer constant"));
                    return None;
                };
                Self::literal_match_value(&lit).or_else(|| {
                    self.context.error(format!(
                        "const `{variant}` is not an integer literal pattern"
                    ));
                    None
                })
            }
            _ => {
                self.context
                    .error("enum variant pattern in a literal `match`".to_owned());
                None
            }
        }
    }

    // Fold an integer/char/bool literal pattern to its compared integer value.
    fn literal_match_value(lit: &Literal) -> Option<IrValue> {
        match lit {
            Literal::Integer(v) | Literal::HexInteger(v) => Some(IrValue::Integer(*v)),
            Literal::Boolean(b) => Some(IrValue::Integer(i64::from(*b))),
            _ => None,
        }
    }

    // Lower an arm's `-> expr` value (if any) and store it into the result slot,
    // recording the store type so later arms and the slot share it.
    fn store_arm_value(
        &mut self,
        value: &Option<Expression>,
        slot: &Option<IrRegister>,
        store_ty: &mut Option<IrType>,
    ) {
        let (Some(slot), Some(value)) = (slot, value) else {
            return;
        };
        let lowered = match store_ty.clone() {
            Some(ty) => self.lower_value_for_type(value, &ty),
            None => self.lower_expression(value),
        };
        if let Some(l) = lowered {
            let ty = store_ty.clone().unwrap_or_else(|| l.ty.clone());
            store_ty.get_or_insert(ty.clone());
            self.push_instruction(IrInstruction::Store {
                ty,
                value: l.value,
                ptr: slot.clone(),
                offset: None,
            });
        }
    }

    // Load each payload slot into a fresh local named after its pattern binding
    // (`_` discards). The binding's slot follows the normal local convention.
    fn bind_variant_payload(
        &mut self,
        enum_ptr: &IrRegister,
        info: &VariantInfo,
        bindings: &[String],
        match_id: usize,
    ) {
        let slots = self.variant_payload_layout(&info.payload);
        for (binding, (offset, field_ty)) in bindings.iter().zip(slots) {
            if binding == "_" {
                continue;
            }
            let loaded = self.new_temp();
            self.push_instruction(IrInstruction::Load {
                dest: loaded.clone(),
                ty: field_ty.clone(),
                ptr: enum_ptr.clone(),
                offset: Some(offset),
            });
            let slot = IrRegister::Named(format!("__m{match_id}_{binding}"));
            self.push_instruction(IrInstruction::Alloc {
                dest: slot.clone(),
                ty: field_ty.clone(),
                count: None,
            });
            self.push_instruction(IrInstruction::Store {
                ty: field_ty.clone(),
                value: IrValue::Register(loaded),
                ptr: slot.clone(),
                offset: None,
            });
            self.context.symbols.insert(
                binding.clone(),
                IrType::Pointer(Box::new(field_ty)),
                IrValue::Register(slot),
            );
        }
    }
}
