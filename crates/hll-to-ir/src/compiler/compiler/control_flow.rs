use super::{
    AssignTarget, BinaryOp, Block, DeferredAction, EvalMode, Expression, HighLevelCompiler,
    IrInstruction, IrProgram, IrRegister, IrTerminator, IrType, IrValue, Literal, Statement,
};
use crate::ast::{MatchArm, PrimaryExpr};

/// Rewrite each `continue` in `stmts` (but not inside nested loops) to run the
/// loop `step` first, so a `for`-loop desugared to `while` still advances its
/// counter on `continue`.
fn rewrite_continue_with_step(stmts: &[Statement], step: &Statement) -> Vec<Statement> {
    stmts.iter().map(|s| rewrite_stmt(s, step)).collect()
}

fn rewrite_stmt(stmt: &Statement, step: &Statement) -> Statement {
    match stmt {
        Statement::Continue => Statement::Block(Block {
            statements: vec![step.clone(), Statement::Continue],
        }),
        Statement::Block(block) => Statement::Block(Block {
            statements: rewrite_continue_with_step(&block.statements, step),
        }),
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => Statement::If {
            cond: cond.clone(),
            then_block: Block {
                statements: rewrite_continue_with_step(&then_block.statements, step),
            },
            else_branch: else_branch
                .as_ref()
                .map(|branch| Box::new(rewrite_stmt(branch, step))),
        },
        // Nested `while`/`for` own their own `continue`; do not descend.
        other => other.clone(),
    }
}

impl HighLevelCompiler {
    pub(super) fn lower_block(&mut self, ir_program: &mut IrProgram, block: &Block) {
        for statement in &block.statements {
            if let Some(b) = &self.current_block {
                if b.terminator.is_some() {
                    self.context
                        .diagnostics
                        .warn("statement appears after terminator - ignored");
                    break;
                }
            }
            self.lower_statement(ir_program, statement);
        }
    }

    pub(super) fn lower_statement(&mut self, ir_program: &mut IrProgram, statement: &Statement) {
        log::trace!("Lowering statement: {statement:?}");
        match statement {
            Statement::Expression(expr) => {
                // `match` lowers here where `ir_program` is available: bare as a
                // statement, or as the rvalue of `place = match ...`.
                match expr {
                    Expression::Match { scrutinee, arms } => {
                        self.lower_match(ir_program, scrutinee, arms);
                    }
                    Expression::Assignment { target, rvalue }
                        if matches!(rvalue.as_ref(), Expression::Match { .. }) =>
                    {
                        let Expression::Match { scrutinee, arms } = rvalue.as_ref() else {
                            unreachable!()
                        };
                        self.lower_assignment_match(ir_program, target, scrutinee, arms);
                    }
                    _ => {
                        let _ = self.lower_expression(expr);
                    }
                }
            }
            Statement::Return(expr) => {
                // Check if we're in a function that returns an aggregate (has sret)
                let has_sret = self.context.symbols.lookup("__sret_ptr").is_some();

                // A scalar/pointer `return match ...` lowers the match into a result
                // slot; aggregate (sret) returns fall through to the normal path.
                if !has_sret {
                    if let Some(Expression::Match { scrutinee, arms }) = expr.as_ref() {
                        let target = self.current_return_ty.clone();
                        let value = self
                            .lower_match_value(ir_program, scrutinee, arms, target)
                            .map(|l| l.value);
                        let defers = self.defers.clone();
                        for action in defers.into_iter().rev() {
                            self.emit_deferred_action(action);
                        }
                        self.set_terminator(IrTerminator::Return(value));
                        return;
                    }
                }

                if has_sret {
                    // For functions returning aggregates, we need to copy the value to sret
                    if let Some(return_expr) = expr {
                        if let Some(lowered) = self.lower_expression(return_expr) {
                            // The lowered value is a register pointing to the local aggregate
                            // We need to copy it to the sret location
                            let sret_ptr_info = self.context.symbols.lookup("__sret_ptr").cloned();

                            if let Some(sret_info) = sret_ptr_info {
                                // Get the sret pointer value (used by backend in lower_terminator)
                                let _sret_ptr_val = sret_info.value;

                                // Get the source address (where the aggregate is currently stored)
                                // Backend uses this to copy aggregate to sret location
                                let _src_addr = if let IrValue::Register(reg) = &lowered.value {
                                    reg.clone()
                                } else {
                                    self.context.error(
                                        "Aggregate return value must be a register".to_owned(),
                                    );
                                    return;
                                };

                                // Emit code to copy the aggregate from src_addr to sret_ptr
                                // We'll use a series of Load/Store instructions to copy field by field
                                let agg_ty = lowered.ty.clone();

                                self.push_instruction(IrInstruction::Comment(format!(
                                    "copying aggregate return ({agg_ty}) to sret location"
                                )));

                                // The backend already handles copying from src to sret in lower_terminator
                                self.set_terminator(IrTerminator::Return(Some(lowered.value)));
                            } else {
                                self.context.error(
                                    "Internal error: __sret_ptr not found in symbol table"
                                        .to_owned(),
                                );
                            }
                        }
                    } else {
                        self.context.error(
                            "Function returning aggregate must have a return value".to_owned(),
                        );
                    }
                } else {
                    // Normal return (non-aggregate); literals take the return width.
                    let return_ty = self.current_return_ty.clone();
                    let value = expr
                        .as_ref()
                        .and_then(|e| match &return_ty {
                            Some(rt) => self.lower_value_for_type(e, rt),
                            None => self.lower_expression(e),
                        })
                        .map(|l| l.value);

                    // Emulate proper defer by emitting cleanup instructions at exit points
                    let defers = self.defers.clone();
                    if !defers.is_empty() {
                        self.push_instruction(IrInstruction::Comment(
                            "executing deferred cleanup before return".to_owned(),
                        ));
                    }
                    for action in defers.into_iter().rev() {
                        self.emit_deferred_action(action);
                    }

                    self.set_terminator(IrTerminator::Return(value));
                }
            }
            Statement::VariableDecl { name, ty, init } => {
                self.push_instruction(IrInstruction::Comment(format!("local var: {name}")));
                let lowered_ty = match self.lower_type_with_program(ir_program, ty) {
                    Ok(ty) => ty,
                    Err(e) => {
                        self.context
                            .diagnostics
                            .error(format!("failed to lower type for variable `{name}`: {e:?}"));
                        return;
                    }
                };

                // Track unsigned variables
                if Self::is_unsigned_primitive_type(ty) {
                    self.context.unsigned_vars.insert(name.clone());
                }

                let ptr_reg = IrRegister::Named(name.clone());

                self.push_instruction(IrInstruction::Alloc {
                    dest: ptr_reg.clone(),
                    ty: lowered_ty.clone(),
                    count: None,
                });

                if let Some(init_expr) = init {
                    let lowered = if let Expression::Match { scrutinee, arms } = init_expr {
                        self.lower_match_value(
                            ir_program,
                            scrutinee,
                            arms,
                            Some(lowered_ty.clone()),
                        )
                    } else {
                        self.lower_value_for_type(init_expr, &lowered_ty)
                    };
                    if let Some(lowered) = lowered {
                        self.push_instruction(IrInstruction::Store {
                            ty: lowered_ty.clone(),
                            value: lowered.value,
                            ptr: ptr_reg.clone(),
                            offset: None,
                        });
                    }
                }

                self.context.symbols.insert(
                    name.clone(),
                    IrType::Pointer(Box::new(lowered_ty)),
                    IrValue::Register(ptr_reg),
                );
            }
            Statement::InferredVariableDecl { name, init } => {
                if let Expression::Primary(crate::ast::PrimaryExpr::New { ty, .. }) = init {
                    if let Err(error) = self.lower_type_with_program(ir_program, ty) {
                        self.context.error(format!(
                            "failed to resolve inferred type for `{name}`: {error:?}"
                        ));
                        return;
                    }
                }
                // Evaluate the initializer before introducing the symbol, which
                // preserves V2's no-self-reference rule for inferred bindings.
                let lowered = if let Expression::Match { scrutinee, arms } = init {
                    self.lower_match_value(ir_program, scrutinee, arms, None)
                } else {
                    self.lower_expression(init)
                };
                let Some(lowered) = lowered else {
                    self.context
                        .error(format!("failed to infer initializer type for `{name}`"));
                    return;
                };
                if lowered.ty == IrType::Void
                    || matches!(&lowered.ty, IrType::Named(n) if n == "unknown")
                {
                    self.context.error(format!(
                        "initializer for `{name}` has no concrete value type"
                    ));
                    return;
                }

                if lowered.is_unsigned {
                    self.context.unsigned_vars.insert(name.clone());
                }

                let ptr_reg = IrRegister::Named(name.clone());
                self.push_instruction(IrInstruction::Comment(format!(
                    "inferred local var: {name}"
                )));
                self.push_instruction(IrInstruction::Alloc {
                    dest: ptr_reg.clone(),
                    ty: lowered.ty.clone(),
                    count: None,
                });
                self.push_instruction(IrInstruction::Store {
                    ty: lowered.ty.clone(),
                    value: lowered.value,
                    ptr: ptr_reg.clone(),
                    offset: None,
                });
                self.context.symbols.insert(
                    name.clone(),
                    IrType::Pointer(Box::new(lowered.ty)),
                    IrValue::Register(ptr_reg),
                );
            }
            Statement::Block(block) => {
                self.lower_block(ir_program, block);
            }
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                self.lower_if(ir_program, cond, then_block, else_branch.as_deref());
            }
            Statement::While { cond, body } => {
                self.lower_while(ir_program, cond, body);
            }
            Statement::For { var, iter, body } => {
                self.lower_for(ir_program, var, iter, body);
            }
            Statement::Break => {
                if let Some((_, break_label)) = self.loop_labels.last() {
                    self.set_terminator(IrTerminator::Jump(break_label.clone()));
                } else {
                    self.context
                        .diagnostics
                        .error("break outside of loop".to_owned());
                }
            }
            Statement::Continue => {
                if let Some((continue_label, _)) = self.loop_labels.last() {
                    self.set_terminator(IrTerminator::Jump(continue_label.clone()));
                } else {
                    self.context
                        .diagnostics
                        .error("continue outside of loop".to_owned());
                }
            }
            Statement::AsmBlock { lines } => {
                self.push_instruction(IrInstruction::InlineAsm {
                    lines: lines.clone(),
                });
            }
            Statement::Defer(expr) => {
                if let Expression::Primary(crate::ast::PrimaryExpr::FunctionCall {
                    name,
                    arguments,
                    ..
                }) = expr
                {
                    let mut captured_args = Vec::new();
                    for arg in arguments {
                        let lowered = if let Some(v) = self.lower_expression(arg) {
                            v
                        } else {
                            self.context.error(format!(
                                "failed to capture defer argument `{}` for call `{}`",
                                self.format_expression(arg),
                                name
                            ));
                            return;
                        };
                        captured_args.push(lowered.value);
                    }

                    self.push_instruction(IrInstruction::Comment(format!(
                        "defer: captured call {} with {} args",
                        name,
                        captured_args.len()
                    )));
                    self.defers.push(DeferredAction::Call {
                        function: name.clone(),
                        args: captured_args,
                    });
                } else {
                    self.push_instruction(IrInstruction::Comment(
                        "defer: register cleanup logic".to_owned(),
                    ));
                    self.context.warn(
                        "defer on non-call expression is not capture-safe yet; evaluating at exit",
                    );
                    self.defers.push(DeferredAction::Expr(expr.clone()));
                }
            }
        }
    }

    // Lower `place = match ...`: resolve the target address, then store each
    // matched arm's value into it through the slot-based value match.
    fn lower_assignment_match(
        &mut self,
        ir_program: &mut IrProgram,
        target: &AssignTarget,
        scrutinee: &Expression,
        arms: &[MatchArm],
    ) {
        let Some(target_expr) = Self::assign_target_to_expression(target) else {
            self.context
                .error("unsupported assignment target for a `match` value".to_owned());
            return;
        };
        let Some(addr) = self.lower_expr(&target_expr, EvalMode::Address) else {
            return;
        };
        let (ptr_reg, store_ty) = match (&addr.value, &addr.ty) {
            (IrValue::Register(reg), IrType::Pointer(inner)) => (reg.clone(), *inner.clone()),
            _ => {
                self.context
                    .error("assignment target did not resolve to an address".to_owned());
                return;
            }
        };
        if let Some(lowered) =
            self.lower_match_value(ir_program, scrutinee, arms, Some(store_ty.clone()))
        {
            self.push_instruction(IrInstruction::Store {
                ty: store_ty,
                value: lowered.value,
                ptr: ptr_reg,
                offset: None,
            });
        }
    }

    pub(super) fn lower_if(
        &mut self,
        ir_program: &mut IrProgram,
        cond: &Expression,
        then_block: &Block,
        else_branch: Option<&Statement>,
    ) {
        self.push_instruction(IrInstruction::Comment("if condition".to_owned()));
        let cond_value = if let Some(lowered) = self.lower_expression(cond) {
            lowered.value
        } else {
            self.context.error(format!(
                "failed to lower if condition `{}` (see previous diagnostics for root cause)",
                self.format_expression(cond)
            ));
            IrValue::Bool(false)
        };

        let then_label = self.new_label();
        let else_label = self.new_label();
        let merge_label = self.new_label();

        // Snapshot environment before branching
        let merge_env = self.context.snapshot_env();

        let branch_else = if else_branch.is_some() {
            else_label.clone()
        } else {
            merge_label.clone()
        };

        self.set_terminator(IrTerminator::Branch {
            cond: cond_value,
            then_label: then_label.clone(),
            else_label: branch_else.clone(),
        });

        // Lower then branch
        self.start_new_block(then_label.0.clone());
        self.context.restore_env(merge_env.clone());
        self.lower_block(ir_program, then_block);
        let then_exit_env = self.context.snapshot_env();
        self.context.save_block_exit_values(then_label.clone());
        self.set_terminator(IrTerminator::Jump(merge_label.clone()));

        // Lower else branch
        let else_exit_env = if let Some(else_stmt) = else_branch {
            self.start_new_block(else_label.0.clone());
            self.context.restore_env(merge_env.clone());
            self.lower_statement(ir_program, else_stmt);
            let env = self.context.snapshot_env();
            self.context.save_block_exit_values(else_label.clone());
            self.set_terminator(IrTerminator::Jump(merge_label.clone()));
            env
        } else {
            merge_env.clone()
        };

        // Start merge block and emit phi nodes for diverging variables
        self.start_new_block(merge_label.0.clone());
        self.context.restore_env(merge_env.clone());

        let mut then_vars: Vec<_> = then_exit_env.iter().collect();
        then_vars.sort_by_key(|(k, _)| *k);
        for (var_name, then_value) in then_vars {
            if let Some(else_value) = else_exit_env.get(var_name) {
                if then_value != else_value {
                    // Emit phi node to merge diverging values
                    let phi_dest = self.new_temp();
                    let ty = self.infer_type_from_value(then_value);
                    self.push_instruction(IrInstruction::Phi {
                        dest: phi_dest.clone(),
                        ty,
                        incoming: vec![
                            (then_value.clone(), then_label.clone()),
                            (else_value.clone(), else_label.clone()),
                        ],
                    });
                    self.context
                        .ssa_env
                        .insert(var_name.clone(), IrValue::Register(phi_dest));
                } else {
                    self.context
                        .ssa_env
                        .insert(var_name.clone(), then_value.clone());
                }
            } else {
                self.context
                    .ssa_env
                    .insert(var_name.clone(), then_value.clone());
            }
        }
    }

    pub(super) fn lower_while(
        &mut self,
        ir_program: &mut IrProgram,
        cond: &Expression,
        body: &Block,
    ) {
        let cond_label = self.new_label();
        let body_label = self.new_label();
        let exit_label = self.new_label();

        // Snapshot environment before loop
        let pre_loop_env = self.context.snapshot_env();

        self.set_terminator(IrTerminator::Jump(cond_label.clone()));

        self.start_new_block(cond_label.0.clone());
        self.push_instruction(IrInstruction::Comment("while condition".to_owned()));
        let cond_value = if let Some(lowered) = self.lower_expression(cond) {
            lowered.value
        } else {
            self.context.error(format!(
                "failed to lower while condition `{}` (see previous diagnostics for root cause)",
                self.format_expression(cond)
            ));
            IrValue::Bool(false)
        };

        self.set_terminator(IrTerminator::Branch {
            cond: cond_value,
            then_label: body_label.clone(),
            else_label: exit_label.clone(),
        });

        self.start_new_block(body_label.0.clone());
        self.context.restore_env(pre_loop_env.clone());
        self.loop_labels
            .push((cond_label.clone(), exit_label.clone()));
        self.lower_block(ir_program, body);
        let loop_exit_env = self.context.snapshot_env();
        self.context.save_block_exit_values(body_label.clone());
        self.loop_labels.pop();
        self.set_terminator(IrTerminator::Jump(cond_label.clone()));

        self.start_new_block(exit_label.0.clone());

        // Emit phi nodes for variables that changed in the loop
        let mut pre_loop_vars: Vec<_> = pre_loop_env.iter().collect();
        pre_loop_vars.sort_by_key(|(k, _)| *k);
        for (var_name, pre_loop_value) in pre_loop_vars {
            if let Some(post_loop_value) = loop_exit_env.get(var_name) {
                if pre_loop_value != post_loop_value {
                    let phi_dest = self.new_temp();
                    let ty = self.infer_type_from_value(pre_loop_value);
                    self.push_instruction(IrInstruction::Phi {
                        dest: phi_dest.clone(),
                        ty,
                        incoming: vec![
                            (pre_loop_value.clone(), cond_label.clone()),
                            (post_loop_value.clone(), body_label.clone()),
                        ],
                    });
                    self.context
                        .ssa_env
                        .insert(var_name.clone(), IrValue::Register(phi_dest));
                } else {
                    self.context
                        .ssa_env
                        .insert(var_name.clone(), pre_loop_value.clone());
                }
            } else {
                self.context
                    .ssa_env
                    .insert(var_name.clone(), pre_loop_value.clone());
            }
        }
    }

    // Desugar a `for` loop to a `while` loop, reusing the existing `while`
    // lowering (phi/break/continue) -- no new IR.
    pub(super) fn lower_for(
        &mut self,
        ir_program: &mut IrProgram,
        var: &str,
        iter: &crate::ast::ForIter,
        body: &Block,
    ) {
        match iter {
            crate::ast::ForIter::Range {
                start,
                end,
                inclusive,
            } => self.lower_for_range(ir_program, var, start, end, *inclusive, body),
            crate::ast::ForIter::Each(seq) => self.lower_for_each(ir_program, var, seq, body),
        }
    }

    // `for var in start..end { body }` becomes:
    //   var := start
    //   __for_end := end                 ; end evaluated once
    //   while var < __for_end {          ; `<=` when inclusive
    //       <body, continue -> step+continue>
    //       var = var + 1                ; the step
    //   }
    fn lower_for_range(
        &mut self,
        ir_program: &mut IrProgram,
        var: &str,
        start: &Expression,
        end: &Expression,
        inclusive: bool,
        body: &Block,
    ) {
        let id = self.for_loop_id;
        self.for_loop_id += 1;
        let end_name = format!("__for_end_{id}");

        let ident = |name: &str| Expression::Primary(PrimaryExpr::Identifier(name.to_owned()));

        let decl_var = Statement::InferredVariableDecl {
            name: var.to_owned(),
            init: start.clone(),
        };
        let decl_end = Statement::InferredVariableDecl {
            name: end_name.clone(),
            init: end.clone(),
        };

        let cond = Expression::Binary {
            op: if inclusive {
                BinaryOp::Lte
            } else {
                BinaryOp::Lt
            },
            left: Box::new(ident(var)),
            right: Box::new(ident(&end_name)),
        };

        let step = Self::increment_stmt(var);

        let mut body_stmts = rewrite_continue_with_step(&body.statements, &step);
        body_stmts.push(step);

        let while_stmt = Statement::While {
            cond,
            body: Block {
                statements: body_stmts,
            },
        };

        let desugared = Block {
            statements: vec![decl_var, decl_end, while_stmt],
        };
        self.lower_block(ir_program, &desugared);
    }

    // `for var in arr { body }` over a fixed array `T[N]` becomes:
    //   __for_ptr := &arr[0]                 ; base element pointer, once
    //   __for_i   := 0
    //   while __for_i < N {
    //       var := __for_ptr[__for_i]        ; element value, element-scaled
    //       <body, continue -> step+continue>
    //       __for_i = __for_i + 1
    //   }
    fn lower_for_each(
        &mut self,
        ir_program: &mut IrProgram,
        var: &str,
        seq: &Expression,
        body: &Block,
    ) {
        if self.seq_is_slice(seq) {
            self.lower_for_each_slice(ir_program, var, seq, body);
            return;
        }
        let Some(len) = self.for_each_array_len(seq) else {
            self.context
                .error("`for ... in <array>` requires a fixed-size array operand".to_owned());
            return;
        };

        let id = self.for_loop_id;
        self.for_loop_id += 1;
        let ptr_name = format!("__for_ptr_{id}");
        let idx_name = format!("__for_i_{id}");

        let ident = |name: &str| Expression::Primary(PrimaryExpr::Identifier(name.to_owned()));
        let int = |n: i64| Expression::Primary(PrimaryExpr::Literal(Literal::Integer(n)));

        // __for_ptr := &arr[0]
        let decl_ptr = Statement::InferredVariableDecl {
            name: ptr_name.clone(),
            init: Expression::Unary {
                op: crate::ast::UnaryOp::AddressOf,
                expr: Box::new(Expression::Primary(PrimaryExpr::ArrayIndex {
                    expr: Box::new(seq.clone()),
                    index: Box::new(int(0)),
                })),
            },
        };
        // __for_i := 0
        let decl_idx = Statement::InferredVariableDecl {
            name: idx_name.clone(),
            init: int(0),
        };

        let cond = Expression::Binary {
            op: BinaryOp::Lt,
            left: Box::new(ident(&idx_name)),
            right: Box::new(int(len)),
        };

        // var := __for_ptr[__for_i]
        let bind_elem = Statement::InferredVariableDecl {
            name: var.to_owned(),
            init: Expression::Primary(PrimaryExpr::ArrayIndex {
                expr: Box::new(ident(&ptr_name)),
                index: Box::new(ident(&idx_name)),
            }),
        };

        let step = Self::increment_stmt(&idx_name);

        let mut body_stmts = vec![bind_elem];
        body_stmts.extend(rewrite_continue_with_step(&body.statements, &step));
        body_stmts.push(step);

        let while_stmt = Statement::While {
            cond,
            body: Block {
                statements: body_stmts,
            },
        };

        let desugared = Block {
            statements: vec![decl_ptr, decl_idx, while_stmt],
        };
        self.lower_block(ir_program, &desugared);
    }

    // True when `seq` is a slice: an identifier of slice type or a range-slice
    // expression (`arr[a..b]`).
    fn seq_is_slice(&self, seq: &Expression) -> bool {
        match seq {
            Expression::Primary(PrimaryExpr::Slice { .. }) => true,
            Expression::Primary(PrimaryExpr::Identifier(name)) => {
                let Some(info) = self.context.symbols.lookup(name) else {
                    return false;
                };
                let ty = match self.resolve_named_type(&info.ty) {
                    IrType::Pointer(inner) => self.resolve_named_type(&inner),
                    other => other,
                };
                matches!(ty, IrType::Slice(_))
            }
            _ => false,
        }
    }

    // `for var in <slice> { body }` over a slice becomes:
    //   __for_len := slice.len           ; length read once
    //   __for_i := 0
    //   while __for_i < __for_len {
    //       var := slice[__for_i]        ; bounds-checked element read
    //       <body, continue -> step+continue>
    //       __for_i = __for_i + 1
    //   }
    fn lower_for_each_slice(
        &mut self,
        ir_program: &mut IrProgram,
        var: &str,
        seq: &Expression,
        body: &Block,
    ) {
        let id = self.for_loop_id;
        self.for_loop_id += 1;
        let len_name = format!("__for_len_{id}");
        let idx_name = format!("__for_i_{id}");
        let seq_name = format!("__for_seq_{id}");

        let ident = |name: &str| Expression::Primary(PrimaryExpr::Identifier(name.to_owned()));
        let int = |n: i64| Expression::Primary(PrimaryExpr::Literal(Literal::Integer(n)));

        // Bind a non-identifier source (e.g. `arr[a..b]`) once so it is not
        // re-evaluated per iteration; identifiers are read directly.
        let (seq, bind_seq) = match seq {
            Expression::Primary(PrimaryExpr::Identifier(_)) => (seq.clone(), None),
            other => (
                ident(&seq_name),
                Some(Statement::InferredVariableDecl {
                    name: seq_name.clone(),
                    init: other.clone(),
                }),
            ),
        };
        let seq = &seq;

        let decl_len = Statement::InferredVariableDecl {
            name: len_name.clone(),
            init: Expression::Primary(PrimaryExpr::FieldAccess {
                expr: Box::new(seq.clone()),
                field: "len".to_owned(),
            }),
        };
        let decl_idx = Statement::InferredVariableDecl {
            name: idx_name.clone(),
            init: int(0),
        };

        let cond = Expression::Binary {
            op: BinaryOp::Lt,
            left: Box::new(ident(&idx_name)),
            right: Box::new(ident(&len_name)),
        };

        let bind_elem = Statement::InferredVariableDecl {
            name: var.to_owned(),
            init: Expression::Primary(PrimaryExpr::ArrayIndex {
                expr: Box::new(seq.clone()),
                index: Box::new(ident(&idx_name)),
            }),
        };

        let step = Self::increment_stmt(&idx_name);

        let mut body_stmts = vec![bind_elem];
        body_stmts.extend(rewrite_continue_with_step(&body.statements, &step));
        body_stmts.push(step);

        let while_stmt = Statement::While {
            cond,
            body: Block {
                statements: body_stmts,
            },
        };

        let mut statements = Vec::new();
        statements.extend(bind_seq);
        statements.push(decl_len);
        statements.push(decl_idx);
        statements.push(while_stmt);
        let desugared = Block { statements };
        self.lower_block(ir_program, &desugared);
    }

    // `<name> = <name> + 1` as a statement, the loop step.
    fn increment_stmt(name: &str) -> Statement {
        Statement::Expression(Expression::Assignment {
            target: Box::new(AssignTarget::Identifier(name.to_owned())),
            rvalue: Box::new(Expression::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expression::Primary(PrimaryExpr::Identifier(
                    name.to_owned(),
                ))),
                right: Box::new(Expression::Primary(PrimaryExpr::Literal(Literal::Integer(
                    1,
                )))),
            }),
        })
    }

    // Static element count of an array-typed identifier operand, or `None` if
    // the operand is not a resolvable fixed array.
    fn for_each_array_len(&self, seq: &Expression) -> Option<i64> {
        let Expression::Primary(PrimaryExpr::Identifier(name)) = seq else {
            return None;
        };
        let info = self.context.symbols.lookup(name)?;
        let resolved = self.resolve_named_type(&info.ty);
        let array_ty = match resolved {
            IrType::Array { .. } => resolved,
            IrType::Pointer(inner) => self.resolve_named_type(&inner),
            _ => return None,
        };
        match array_ty {
            IrType::Array { len, .. } => Some(len as i64),
            _ => None,
        }
    }

    pub(super) fn emit_deferred_action(&mut self, action: DeferredAction) {
        match action {
            DeferredAction::Call { function, args } => {
                let dest = if self
                    .function_return_types
                    .get(&function)
                    .is_some_and(|ty| *ty != IrType::Void)
                {
                    Some(self.new_temp())
                } else {
                    None
                };
                self.push_instruction(IrInstruction::Call {
                    dest,
                    function,
                    args,
                });
            }
            DeferredAction::Expr(expr) => {
                let _ = self.lower_expression(&expr);
            }
        }
    }
}
