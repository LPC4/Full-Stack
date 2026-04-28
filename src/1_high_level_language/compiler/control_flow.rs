use super::{
    Block, DeferredAction, Expression, HighLevelCompiler, IrInstruction, IrProgram, IrRegister,
    IrTerminator, IrType, IrValue, Statement,
};

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
                let _ = self.lower_expression(expr);
            }
            Statement::Return(expr) => {
                // Check if we're in a function that returns an aggregate (has sret)
                let has_sret = self.context.symbols.lookup("__sret_ptr").is_some();
                
                if has_sret {
                    // For functions returning aggregates, we need to copy the value to sret
                    if let Some(return_expr) = expr {
                        if let Some(lowered) = self.lower_expression(return_expr) {
                            // The lowered value is a register pointing to the local aggregate
                            // We need to copy it to the sret location
                            let sret_ptr_info = self.context.symbols.lookup("__sret_ptr").cloned();
                            
                            if let Some(sret_info) = sret_ptr_info {
                                // Get the sret pointer value
                                let sret_ptr_val = sret_info.value;
                                
                                // Get the source address (where the aggregate is currently stored)
                                let src_addr = match &lowered.value {
                                    IrValue::Register(reg) => reg.clone(),
                                    _ => {
                                        self.context.diagnostics.error(
                                            "Aggregate return value must be a register".to_owned()
                                        );
                                        return;
                                    }
                                };
                                
                                // Emit code to copy the aggregate from src_addr to sret_ptr
                                // We'll use a series of Load/Store instructions to copy field by field
                                let agg_ty = lowered.ty.clone();
                                
                                self.push_instruction(IrInstruction::Comment(
                                    format!("copying aggregate return ({}) to sret location", agg_ty)
                                ));

                                // The backend already handles copying from src to sret in lower_terminator
                                self.set_terminator(IrTerminator::Return(Some(lowered.value)));
                            } else {
                                self.context.diagnostics.error(
                                    "Internal error: __sret_ptr not found in symbol table".to_owned()
                                );
                            }
                        }
                    } else {
                        self.context.diagnostics.error(
                            "Function returning aggregate must have a return value".to_owned()
                        );
                    }
                } else {
                    // Normal return (non-aggregate)
                    let value = expr
                        .as_ref()
                        .and_then(|e| self.lower_expression(e))
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
                let ptr_reg = IrRegister::Named(name.clone());

                self.push_instruction(IrInstruction::Alloc {
                    dest: ptr_reg.clone(),
                    ty: lowered_ty.clone(),
                    count: None,
                });

                if let Some(init_expr) = init {
                    if let Some(lowered) = self.lower_expression(init_expr) {
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
            Statement::Defer(expr) => {
                if let Expression::Primary(
                    crate::high_level_language::ast::PrimaryExpr::FunctionCall { name, arguments },
                ) = expr
                {
                    let mut captured_args = Vec::new();
                    for arg in arguments {
                        let lowered = if let Some(v) = self.lower_expression(arg) {
                            v
                        } else {
                            self.context.diagnostics.error(format!(
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
                    self.context.diagnostics.warn(
                        "defer on non-call expression is not capture-safe yet; evaluating at exit",
                    );
                    self.defers.push(DeferredAction::Expr(expr.clone()));
                }
            }
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
            self.context.diagnostics.error(format!(
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

        for (var_name, then_value) in &then_exit_env {
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
            self.context.diagnostics.error(format!(
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
        for (var_name, pre_loop_value) in &pre_loop_env {
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
