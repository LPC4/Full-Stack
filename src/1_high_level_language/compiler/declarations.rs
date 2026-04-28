use super::{
    Block, CompilerError, DeclNode, Declaration, Expression, FunctionDecl, GenericTypeDef,
    HighLevelCompiler, IrFunction, IrInstruction, IrParam, IrProgram, IrRegister, IrTerminator,
    IrType, IrTypeAlias, IrValue, Literal, LoweringContext, Program, SemanticAnalyzer, Statement,
};

#[derive(Debug, Clone)]
pub struct ConstEvalContext {
    pub max_depth: usize,
    pub current_depth: usize,
}

impl Default for ConstEvalContext {
    fn default() -> Self {
        Self {
            max_depth: 128,
            current_depth: 0,
        }
    }
}

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
            function_declarations: std::collections::HashMap::new(),
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
        let mut ir_program = IrProgram::new("ir_program");

        for declaration in &program.declarations {
            if let DeclNode::Function {
                name,
                generics,
                return_type,
                ..
            } = &declaration.decl
            {
                let final_name = if generics.is_empty() {
                    name.clone()
                } else {
                    format!("{}<{}>", name, generics.join(", "))
                };
                let return_ty = self.lower_return_type(return_type.as_ref());
                self.function_return_types
                    .insert(final_name.clone(), return_ty.clone());
                if final_name != *name {
                    self.function_return_types.insert(name.clone(), return_ty);
                }
            }
        }

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

                // Store function declaration for compile-time evaluation
                if body.is_some() && generics.is_empty() {
                    self.function_declarations.insert(
                        name.clone(),
                        FunctionDecl {
                            name: name.clone(),
                            generics: generics.clone(),
                            params: params.clone(),
                            return_type: return_type.clone(),
                            body: body.clone(),
                        },
                    );
                    log::debug!("Stored function `{name}` for compile-time evaluation");
                }

                self.context.begin_function();
                self.next_temp = 0;
                self.current_blocks.clear();
                self.current_block = None;
                self.defers.clear();

                let return_ty = self.lower_return_type(return_type.as_ref());

                // For functions returning aggregates, determine return strategy:
                // - Small structs (≤16 bytes): returned in registers a0/a1
                // - Large structs (>16 bytes): use sret pattern with hidden pointer
                let is_aggregate = matches!(return_ty, IrType::Aggregate(_) | IrType::Array { .. });
                let needs_sret = if is_aggregate {
                    let size = self.type_size_in_bytes(&return_ty);
                    size > 16
                } else {
                    false
                };

                let ir_return_ty = if needs_sret {
                    IrType::Void // Change return type to void for sret
                } else {
                    return_ty.clone()
                };

                let mut function = IrFunction::new(final_name.clone(), ir_return_ty);

                // Store the function's ACTUAL return type (before sret transformation) for call-site lookup
                self.function_return_types
                    .insert(final_name.clone(), return_ty.clone());
                if final_name != *name {
                    self.function_return_types
                        .insert(name.clone(), return_ty.clone());
                }

                self.start_new_block("entry");

                // If this function returns an aggregate, inject a hidden sret parameter ONLY for large structs
                // Small structs (≤16 bytes) are returned directly in registers a0/a1
                if needs_sret {
                    let sret_reg = IrRegister::Named("__sret".to_owned());
                    function.push_param(IrParam {
                        ty: IrType::Pointer(Box::new(return_ty.clone())),
                        register: sret_reg.clone(),
                    });

                    self.push_instruction(IrInstruction::Comment(
                        "hidden sret parameter for large aggregate return".to_owned(),
                    ));

                    // Store the sret pointer in a local variable so we can use it in returns
                    let sret_ptr_reg = IrRegister::Named("__sret_ptr".to_owned());
                    self.push_instruction(IrInstruction::Alloc {
                        dest: sret_ptr_reg.clone(),
                        ty: IrType::Pointer(Box::new(return_ty.clone())),
                        count: None,
                    });
                    self.push_instruction(IrInstruction::Store {
                        ty: IrType::Pointer(Box::new(return_ty.clone())),
                        value: IrValue::Register(sret_reg),
                        ptr: sret_ptr_reg.clone(),
                        offset: None,
                    });

                    // Make the sret pointer available in the symbol table for return statements
                    self.context.symbols.insert(
                        "__sret_ptr".to_owned(),
                        IrType::Pointer(Box::new(IrType::Pointer(Box::new(return_ty.clone())))),
                        IrValue::Register(sret_ptr_reg),
                    );
                }

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
        self.eval_const_expr_with_env(expr, &std::collections::HashMap::new())
    }

    // Unified evaluation engine that accepts an optional environment
    fn eval_const_expr_with_env(
        &self,
        expr: &Expression,
        env: &std::collections::HashMap<String, Literal>,
    ) -> Result<Literal, String> {
        self.eval_const_expr_with_env_and_context(expr, env, &mut ConstEvalContext::default())
    }

    fn eval_const_expr_with_env_and_context(
        &self,
        expr: &Expression,
        env: &std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Literal, String> {
        if context.current_depth >= context.max_depth {
            return Err("Const evaluation exceeded maximum recursion depth".to_owned());
        }
        context.current_depth += 1;

        let result = self.eval_const_expr_inner_with_env(expr, env, context);
        context.current_depth -= 1;
        result
    }

    fn eval_const_expr_inner_with_env(
        &self,
        expr: &Expression,
        env: &std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Literal, String> {
        match expr {
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Literal(lit)) => {
                Ok(lit.clone())
            }
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Identifier(
                ident,
            )) => {
                // First check env, then compile_time_consts
                if let Some(lit) = env.get(ident) {
                    Ok(lit.clone())
                } else if let Some(lit) = self.compile_time_consts.get(ident) {
                    Ok(lit.clone())
                } else {
                    Err(format!(
                        "Identifier `{ident}` is not a compile-time constant"
                    ))
                }
            }
            Expression::Unary { op, expr: inner } => {
                let lit = self.eval_const_expr_with_env_and_context(inner, env, context)?;
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
                let l = self.eval_const_expr_with_env_and_context(left, env, context)?;
                let r = self.eval_const_expr_with_env_and_context(right, env, context)?;
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
                    ) => {
                        if b == 0 {
                            Err("Division by zero in compile-time evaluation".to_owned())
                        } else {
                            Ok(Literal::Integer(a / b))
                        }
                    }
                    (
                        crate::high_level_language::ast::BinaryOp::Mod,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => {
                        if b == 0 {
                            Err("Modulo by zero in compile-time evaluation".to_owned())
                        } else {
                            Ok(Literal::Integer(a % b))
                        }
                    }
                    // Comparison operators for integers
                    (
                        crate::high_level_language::ast::BinaryOp::Eq,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a == b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Neq,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a != b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Lt,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a < b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Lte,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a <= b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Gt,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a > b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Gte,
                        Literal::Integer(a),
                        Literal::Integer(b),
                    ) => Ok(Literal::Boolean(a >= b)),
                    // Float comparisons
                    (
                        crate::high_level_language::ast::BinaryOp::Eq,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean((a - b).abs() < f64::EPSILON)),
                    (
                        crate::high_level_language::ast::BinaryOp::Neq,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean((a - b).abs() >= f64::EPSILON)),
                    (
                        crate::high_level_language::ast::BinaryOp::Lt,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean(a < b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Lte,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean(a <= b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Gt,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean(a > b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Gte,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Boolean(a >= b)),
                    // Boolean operations
                    (
                        crate::high_level_language::ast::BinaryOp::And,
                        Literal::Boolean(a),
                        Literal::Boolean(b),
                    ) => Ok(Literal::Boolean(a && b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Or,
                        Literal::Boolean(a),
                        Literal::Boolean(b),
                    ) => Ok(Literal::Boolean(a || b)),
                    // Float arithmetic
                    (
                        crate::high_level_language::ast::BinaryOp::Add,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Float(a + b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Sub,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Float(a - b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Mul,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => Ok(Literal::Float(a * b)),
                    (
                        crate::high_level_language::ast::BinaryOp::Div,
                        Literal::Float(a),
                        Literal::Float(b),
                    ) => {
                        if b.abs() < f64::EPSILON {
                            Err("Division by zero in compile-time evaluation".to_owned())
                        } else {
                            Ok(Literal::Float(a / b))
                        }
                    }
                    _ => Err("Unsupported compile-time binary operation".to_owned()),
                }
            }
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::Grouped(inner)) => {
                self.eval_const_expr_with_env_and_context(inner, env, context)
            }
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::FunctionCall {
                name,
                arguments,
            }) => self.eval_const_function_call(name, arguments, env, context),
            Expression::Primary(crate::high_level_language::ast::PrimaryExpr::FieldAccess {
                expr: inner,
                field,
            }) => self.eval_const_field_access(inner, field, env, context),
            Expression::Assignment { target: _, rvalue } => {
                // For const eval, we only care about the value being assigned
                self.eval_const_expr_with_env_and_context(rvalue, env, context)
            }
            _ => Err("Expression is not a valid compile-time constant".to_owned()),
        }
    }

    fn eval_const_function_call(
        &self,
        name: &str,
        arguments: &[Expression],
        env: &std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Literal, String> {
        // Evaluate all arguments first
        let mut arg_values = Vec::new();
        for arg in arguments {
            arg_values.push(self.eval_const_expr_with_env_and_context(arg, env, context)?);
        }

        // Look up the function declaration
        let func_decl = self
            .function_declarations
            .get(name)
            .ok_or_else(|| format!("Function `{name}` not found for compile-time evaluation"))?;

        // Check that the function has a body
        let body = func_decl
            .body
            .as_ref()
            .ok_or_else(|| format!("Function `{name}` has no body for compile-time evaluation"))?;

        // Verify parameter count matches
        if func_decl.params.len() != arg_values.len() {
            return Err(format!(
                "Function `{}` expects {} arguments but got {}",
                name,
                func_decl.params.len(),
                arg_values.len()
            ));
        }

        log::debug!(
            "Evaluating function `{}` with {} args at compile-time",
            name,
            arg_values.len()
        );

        // Create a local scope for function evaluation
        let mut local_consts = self.compile_time_consts.clone();

        // Bind parameters to argument values
        for (param, value) in func_decl.params.iter().zip(arg_values.iter()) {
            log::debug!("  Binding param `{}` = {:?}", param.name, value);
            local_consts.insert(param.name.clone(), value.clone());
        }

        // Evaluate the function body
        let result = self.eval_const_block(body, &local_consts, context);
        log::debug!("Function `{name}` returned: {result:?}");
        result
    }

    fn eval_const_field_access(
        &self,
        expr: &Expression,
        field: &str,
        env: &std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Literal, String> {
        // Evaluate the base expression
        let _base = self.eval_const_expr_with_env_and_context(expr, env, context)?;

        // For struct literals, we'd extract the field value
        // This would require storing struct literal information during const eval
        Err(format!(
            "Field access `.{field}` on compile-time values is not yet supported"
        ))
    }

    fn eval_const_block(
        &self,
        block: &Block,
        env: &std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Literal, String> {
        // Evaluate statements in the block sequentially
        let mut result = None;
        let mut mutable_env = env.clone();

        for stmt in &block.statements {
            if let Some(value) = self.eval_const_statement(stmt, &mut mutable_env, context)? {
                result = Some(value);
                break; // Statement returned a value, exit block
            }
        }

        result.ok_or_else(|| "Block did not return a value".to_owned())
    }

    // Evaluate a single statement, returning Some(value) if it produces a return value
    fn eval_const_statement(
        &self,
        stmt: &Statement,
        mutable_env: &mut std::collections::HashMap<String, Literal>,
        context: &mut ConstEvalContext,
    ) -> Result<Option<Literal>, String> {
        match stmt {
            Statement::VariableDecl { name, ty: _, init } => {
                // Evaluate and bind the variable
                if let Some(init_expr) = init {
                    let value =
                        self.eval_const_expr_with_env_and_context(init_expr, mutable_env, context)?;
                    mutable_env.insert(name.clone(), value);
                }
                Ok(None) // Variable declaration doesn't return a value
            }
            Statement::Return(expr) => {
                // Return the evaluated expression
                if let Some(return_expr) = expr {
                    let value = self.eval_const_expr_with_env_and_context(
                        return_expr,
                        mutable_env,
                        context,
                    )?;
                    Ok(Some(value))
                } else {
                    Ok(Some(Literal::Integer(0))) // void return
                }
            }
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                // Evaluate condition
                let cond_value =
                    self.eval_const_expr_with_env_and_context(cond, mutable_env, context)?;
                match cond_value {
                    Literal::Boolean(true) => {
                        // Evaluate then block
                        let block_result =
                            self.eval_const_block(then_block, mutable_env, context)?;
                        Ok(Some(block_result))
                    }
                    Literal::Boolean(false) => {
                        // Evaluate else branch if present
                        if let Some(else_branch) = else_branch {
                            match &**else_branch {
                                Statement::Block(else_block) => {
                                    let block_result =
                                        self.eval_const_block(else_block, mutable_env, context)?;
                                    Ok(Some(block_result))
                                }
                                Statement::If { .. } => {
                                    // Nested if (else-if) - evaluate it
                                    self.eval_const_statement(else_branch, mutable_env, context)
                                }
                                _ => {
                                    // Unknown else branch type
                                    Ok(None)
                                }
                            }
                        } else {
                            // No else branch - continue to next statement
                            Ok(None)
                        }
                    }
                    _ => Err("Condition must evaluate to boolean".to_owned()),
                }
            }
            Statement::While { cond, body } => {
                // Evaluate while loop at compile time
                let mut iterations = 0;
                let max_iterations = 1000; // Prevent infinite loops

                loop {
                    if iterations >= max_iterations {
                        return Err(
                            "Compile-time while loop exceeded maximum iterations".to_owned()
                        );
                    }

                    let cond_value =
                        self.eval_const_expr_with_env_and_context(cond, mutable_env, context)?;
                    match cond_value {
                        Literal::Boolean(true) => {
                            // Execute loop body statements
                            let mut should_break = false;
                            for stmt in &body.statements {
                                match stmt {
                                    Statement::Break => {
                                        should_break = true;
                                        break;
                                    }
                                    Statement::Continue => {
                                        // Continue to next iteration
                                        break;
                                    }
                                    _ => {
                                        // Evaluate other statements recursively
                                        if let Some(_return_value) =
                                            self.eval_const_statement(stmt, mutable_env, context)?
                                        {
                                            // If a statement returns a value (like Return), exit the loop
                                            should_break = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if should_break {
                                break;
                            }
                            iterations += 1;
                        }
                        Literal::Boolean(false) => {
                            // Exit loop
                            break;
                        }
                        _ => return Err("While condition must evaluate to boolean".to_owned()),
                    }
                }
                Ok(None) // While loop doesn't return a value
            }
            Statement::Expression(expr) => {
                if let Expression::Assignment { target, rvalue } = expr {
                    // Evaluate the right-hand side
                    let value =
                        self.eval_const_expr_with_env_and_context(rvalue, mutable_env, context)?;

                    // Update the variable in the environment
                    match &**target {
                        crate::high_level_language::ast::AssignTarget::Identifier(name) => {
                            mutable_env.insert(name.clone(), value);
                        }
                        _ => {
                            return Err("Only simple variable assignments are supported in compile-time evaluation".to_owned());
                        }
                    }
                    Ok(None)
                } else {
                    // For non-assignment expressions, just evaluate them (side effects ignored)
                    let _ =
                        self.eval_const_expr_with_env_and_context(expr, mutable_env, context)?;
                    Ok(None)
                }
            }
            _ => Err(format!(
                "Statement type {stmt:?} not supported in compile-time evaluation"
            )),
        }
    }
}
