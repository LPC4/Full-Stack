use super::{HighLevelCompiler, LoweringContext, Program, IrProgram, CompilerError, SemanticAnalyzer, Declaration, DeclNode, GenericTypeDef, IrTypeAlias, IrFunction, IrRegister, IrParam, IrInstruction, IrValue, IrType, IrTerminator, Expression, Literal};

impl HighLevelCompiler {
    pub fn new() -> Self {
        Self {
            context: LoweringContext::new(),
            next_temp: 0,
            next_label: 0,
            current_blocks: Vec::new(),
            current_block: None,
            defers: Vec::new(),
            compile_time_consts: std::collections::HashMap::new(),
            loop_labels: Vec::new(),
            generic_type_cache: std::collections::HashMap::new(),
            generic_type_defs: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            pending_global_strings: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> &[crate::high_level_language::compiler::Diagnostic] {
        self.context.diagnostics.entries()
    }

    pub fn compile_program(&mut self, program: &Program) -> Result<IrProgram, CompilerError> {
        log::info!(
            "Starting IR compilation for {} declarations",
            program.declarations.len()
        );

        let mut semantic_analyzer = SemanticAnalyzer::new();
        if let Err(_) = semantic_analyzer.analyze_program(program) {
            // Collect semantic errors and emit them as diagnostics
            for diagnostic in semantic_analyzer.diagnostics() {
                self.context.diagnostics.error(diagnostic.message.clone());
            }
            log::warn!(
                "Semantic analysis found errors, continuing with compilation for diagnostics"
            );
        }

        self.context.reset_for_program();
        self.next_temp = 0;
        self.next_label = 0;
        self.pending_global_strings.clear();
        let mut ir_program = IrProgram::new("kryon_module");

        for declaration in &program.declarations {
            self.lower_declaration(&mut ir_program, declaration)?;
        }

        // Add all pending global strings to the IR program
        for global_string in self.pending_global_strings.drain(..) {
            ir_program.push_global_string(global_string);
        }

        Ok(ir_program)
    }

    pub(super) fn lower_declaration(
        &mut self,
        ir_program: &mut IrProgram,
        declaration: &Declaration,
    ) -> Result<(), CompilerError> {
        log::debug!("lowering declaration: {:?}", declaration.decl);
        match &declaration.decl {
            DeclNode::Type { name, ty, generics } => {
                if !generics.is_empty() {
                    // This is a generic type definition, store it for later specialization
                    self.generic_type_defs.insert(
                        name.clone(),
                        GenericTypeDef {
                            params: generics.clone(),
                            ty: ty.clone(),
                        },
                    );
                    log::debug!(
                        "Registered generic type `{}` with {} params",
                        name,
                        generics.len()
                    );
                    Ok(())
                } else {
                    // Non-generic type, lower directly
                    let lowered = self.lower_type(ty);
                    self.context
                        .types
                        .register_type(name.clone(), lowered.clone());
                    ir_program.push_type_alias(IrTypeAlias {
                        name: name.clone(),
                        ty: lowered,
                    });
                    Ok(())
                }
            }
            DeclNode::Variable {
                name,
                ty: _ty,
                init: _init,
            } => {
                self.context.diagnostics.warn(format!(
                    "global variable `{name}` lowering emitted as static placeholder"
                ));
                Ok(())
            }
            DeclNode::Const { name, init } => {
                match self.eval_const_expr(init) {
                    Ok(literal) => {
                        self.compile_time_consts.insert(name.clone(), literal);
                    }
                    Err(err) => {
                        self.context
                            .diagnostics
                            .error(format!("const `{name}` initialization failed: {err}"));
                    }
                }
                Ok(())
            }
            DeclNode::Function {
                name,
                generics,
                params,
                return_type,
                body,
                is_extern,
                ..
            } => {
                let final_name = if generics.is_empty() {
                    name.clone()
                } else {
                    format!("{}<{}>", name, generics.join(", "))
                };
                if *is_extern {
                    self.context.diagnostics.warn(format!(
                        "extern function `{final_name}` lowered as placeholder"
                    ));
                }

                self.context.begin_function();
                self.next_temp = 0;
                self.current_blocks.clear();
                self.current_block = None;
                self.defers.clear();

                let mut function = IrFunction::new(
                    final_name.clone(),
                    self.lower_return_type(return_type.as_ref()),
                );

                // Store the function's return type for later lookup during calls
                let return_ty = self.lower_return_type(return_type.as_ref());
                self.function_return_types
                    .insert(final_name.clone(), return_ty);
                if final_name != *name {
                    // Keep source-name lookup working at call-sites until full function monomorphization is added.
                    let return_ty = self.lower_return_type(return_type.as_ref());
                    self.function_return_types.insert(name.clone(), return_ty);
                }

                self.start_new_block("entry");

                for param in params {
                    let lowered_ty = self.lower_type(&param.ty);
                    let register = IrRegister::Named(param.name.clone());
                    function.push_param(IrParam {
                        ty: lowered_ty.clone(),
                        register: register.clone(),
                    });

                    self.push_instruction(IrInstruction::Comment(format!(
                        "bind parameter: {}",
                        param.name
                    )));
                    let ptr_reg = IrRegister::Named(format!("{}_ptr", param.name));
                    self.push_instruction(IrInstruction::Alloc {
                        dest: ptr_reg.clone(),
                        ty: lowered_ty.clone(),
                        count: None,
                    });
                    self.push_instruction(IrInstruction::Store {
                        ty: lowered_ty.clone(),
                        value: IrValue::Register(register),
                        ptr: ptr_reg.clone(),
                        offset: None,
                    });
                    self.context.symbols.insert(
                        param.name.clone(),
                        IrType::Pointer(Box::new(lowered_ty)),
                        IrValue::Register(ptr_reg),
                    );
                }

                match body {
                    Some(body) => self.lower_block(ir_program, body),
                    None => {
                        self.push_instruction(IrInstruction::Comment(format!(
                            "no body for function `{name}`"
                        )));
                    }
                }

                if let Some(b) = self.current_block.take() {
                    if b.terminator.is_none() {
                        self.current_block = Some(b);

                        let defers = std::mem::take(&mut self.defers);
                        if !defers.is_empty() {
                            self.push_instruction(IrInstruction::Comment(
                                "executing deferred cleanup before return".to_owned(),
                            ));
                        }
                        for action in defers.into_iter().rev() {
                            self.emit_deferred_action(action);
                        }

                        let mut final_b = self.current_block.take().unwrap();
                        final_b.set_terminator(IrTerminator::Return(None));
                        self.current_blocks.push(final_b);
                    } else {
                        self.current_blocks.push(b);
                    }
                }

                for b in self.current_blocks.drain(..) {
                    function.push_block(b);
                }

                ir_program.push_function(function);
                self.context.end_function();
                Ok(())
            }
        }
    }

    pub(super) fn eval_const_expr(&self, expr: &Expression) -> Result<Literal, String> {
        match expr {
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Literal(lit)) => {
                Ok(lit.clone())
            }
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Identifier(
                ident,
            )) => {
                if let Some(lit) = self.compile_time_consts.get(ident) {
                    Ok(lit.clone())
                } else {
                    Err(format!(
                        "Identifier `{ident}` is not a compile-time constant"
                    ))
                }
            }
            Expression::Unary { op, expr: inner } => {
                let lit = self.eval_const_expr(inner)?;
                match (op, lit) {
                    (crate::high_level_language::ast::UnaryOp::Negate, Literal::Integer(i)) => {
                        Ok(Literal::Integer(-i))
                    }
                    (crate::high_level_language::ast::UnaryOp::Negate, Literal::Float(f)) => {
                        Ok(Literal::Float(-f))
                    }
                    (crate::high_level_language::ast::UnaryOp::Not, Literal::Boolean(b)) => {
                        Ok(Literal::Boolean(!b))
                    }
                    _ => Err("Unsupported compile-time unary operation".to_owned()),
                }
            }
            Expression::Binary { op, left, right } => {
                let l = self.eval_const_expr(left)?;
                let r = self.eval_const_expr(right)?;
                match (op, l, r) {
                    (
                        crate::high_level_language::ast::BinaryOp::Add,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Integer(a + b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Sub,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Integer(a - b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Mul,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Integer(a * b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Div,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Integer(a / b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Mod,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Integer(a % b)),
                    _ => Err("Unsupported compile-time binary operation".to_owned()),
                }
            }
            _ => Err("Expression is not a valid compile-time constant".to_owned()),
        }
    }
}
