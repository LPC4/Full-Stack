use crate::high_level_language::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Declaration, Expression, Literal, Program, ReturnType,
    Statement, Type, UnaryOp,
};
use crate::high_level_language::compiler::lowering_context::LoweringContext;
use crate::intermediate_language::{
    FloatWidth, IntWidth, IrBlock, IrCmpOp, IrFunction, IrInstruction, IrLabel, IrMathOp, IrParam,
    IrProgram, IrRegister, IrTerminator, IrType, IrTypeAlias, IrUnaryOp, IrValue,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CompilerError {
    UnsupportedDeclaration(String),
    UnsupportedFeature(&'static str),
}

#[derive(Debug, Clone)]
struct LoweredValue {
    value: IrValue,
    ty: IrType,
}

#[derive(Debug, Default)]
pub struct HighLevelCompiler {
    context: LoweringContext,
    next_temp: u32,
    next_label: u32,
    current_blocks: Vec<IrBlock>,
    current_block: Option<IrBlock>,
    defers: Vec<Expression>,
    compile_time_consts: std::collections::HashMap<String, Literal>,
    loop_labels: Vec<(IrLabel, IrLabel)>,
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
        }
    }

    pub fn diagnostics(&self) -> &[crate::high_level_language::compiler::Diagnostic] {
        self.context.diagnostics.entries()
    }

    fn start_new_block(&mut self, label: impl Into<String>) {
        if let Some(block) = self.current_block.take() {
            self.current_blocks.push(block);
        }
        self.current_block = Some(IrBlock::new(label));
    }

    fn push_instruction(&mut self, inst: IrInstruction) {
        if let Some(b) = self.current_block.as_mut() {
            b.push_instruction(inst);
        } else {
            log::warn!("Instruction pushed without active block: {:?}", inst);
        }
    }

    fn set_terminator(&mut self, term: IrTerminator) {
        if let Some(b) = self.current_block.as_mut() {
            if b.terminator.is_none() {
                b.set_terminator(term);
            }
        }
    }

    pub fn compile_program(&mut self, program: &Program) -> Result<IrProgram, CompilerError> {
        log::info!(
            "Starting IR compilation for {} declarations",
            program.declarations.len()
        );
        self.context.reset_for_program();
        self.next_temp = 0;
        self.next_label = 0;
        let mut ir_program = IrProgram::new("kryon_module");

        for declaration in &program.declarations {
            self.lower_declaration(&mut ir_program, declaration)?;
        }

        Ok(ir_program)
    }

    fn lower_declaration(
        &mut self,
        ir_program: &mut IrProgram,
        declaration: &Declaration,
    ) -> Result<(), CompilerError> {
        log::debug!("lowering declaration: {:?}", declaration.decl);
        match &declaration.decl {
            DeclNode::Type { name, ty, generics } => {
                let mut final_name = name.clone();
                if !generics.is_empty() {
                    for _ in generics {
                        final_name.push_str("_gen");
                    }
                }
                let lowered = self.lower_type(ty);
                self.context
                    .types
                    .register_type(final_name.clone(), lowered.clone());
                ir_program.push_type_alias(IrTypeAlias {
                    name: final_name.clone(),
                    ty: lowered,
                });
                Ok(())
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
                            .error(format!("const `{name}` initialization failed: {}", err));
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
                let mut final_name = name.clone();
                if !generics.is_empty() {
                    for _ in generics {
                        final_name.push_str("_gen");
                    }
                }
                if *is_extern {
                    self.context.diagnostics.warn(format!(
                        "extern function `{}` lowered as placeholder",
                        final_name
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
                    Some(body) => self.lower_block(body),
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
                                "executing deferred cleanup before return".to_string(),
                            ));
                        }
                        for defer_expr in defers.into_iter().rev() {
                            let _ = self.lower_expression(&defer_expr);
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

    fn eval_const_expr(&self, expr: &Expression) -> Result<Literal, String> {
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
                    _ => Err(format!("Unsupported compile-time unary operation")),
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
                    _ => Err("Unsupported compile-time binary operation".to_string()),
                }
            }
            _ => Err("Expression is not a valid compile-time constant".to_string()),
        }
    }

    fn lower_return_type(&mut self, return_type: Option<&ReturnType>) -> IrType {
        match return_type {
            Some(ReturnType::Single(ty)) => self.lower_type(ty),
            Some(ReturnType::Tuple(fields)) => {
                IrType::Aggregate(fields.iter().map(|f| self.lower_type(&f.ty)).collect())
            }
            None => IrType::Void,
        }
    }

    fn lower_type(&mut self, ty: &Type) -> IrType {
        match ty {
            Type::Primitive(name) => self.lower_primitive_type(name),
            Type::Pointer(inner) => IrType::Pointer(Box::new(self.lower_type(inner))),
            Type::Array(len, inner) => IrType::Array {
                len: *len,
                element: Box::new(self.lower_type(inner)),
            },
            Type::Struct(fields) => {
                IrType::Aggregate(fields.iter().map(|f| self.lower_type(&f.ty)).collect())
            }
            Type::Named { name, args } => {
                if !args.is_empty() {
                    let mut mangled_name = name.clone();
                    for _ in args {
                        mangled_name.push_str("_gen"); // generic type mangling
                    }
                    return IrType::Named(mangled_name);
                }
                IrType::Named(name.clone())
            }
        }
    }

    fn lower_primitive_type(&self, name: &str) -> IrType {
        match name {
            "i8" => IrType::Integer(IntWidth::I8),
            "i16" => IrType::Integer(IntWidth::I16),
            "i32" => IrType::Integer(IntWidth::I32),
            "i64" => IrType::Integer(IntWidth::I64),
            "u8" => IrType::Integer(IntWidth::I8),
            "u16" => IrType::Integer(IntWidth::I16),
            "u32" => IrType::Integer(IntWidth::I32),
            "u64" => IrType::Integer(IntWidth::I64),
            "f32" => IrType::Float(FloatWidth::F32),
            "f64" => IrType::Float(FloatWidth::F64),
            "bool" => IrType::Integer(IntWidth::I1),
            "Str" => IrType::Named("Str".to_owned()),
            other => IrType::Named(other.to_owned()),
        }
    }

    fn lower_block(&mut self, block: &Block) {
        for statement in &block.statements {
            if let Some(b) = &self.current_block {
                if b.terminator.is_some() {
                    self.context
                        .diagnostics
                        .warn("statement appears after terminator - ignored");
                    break;
                }
            }
            self.lower_statement(statement);
        }
    }

    fn lower_statement(&mut self, statement: &Statement) {
        log::trace!("Lowering statement: {:?}", statement);
        match statement {
            Statement::Expression(expr) => {
                let _ = self.lower_expression(expr);
            }
            Statement::Return(expr) => {
                let value = expr
                    .as_ref()
                    .and_then(|e| self.lower_expression(e))
                    .map(|l| l.value);

                // Emulate proper defer by emitting cleanup instructions at exit points
                let defers = self.defers.clone();
                if !defers.is_empty() {
                    self.push_instruction(IrInstruction::Comment(
                        "executing deferred cleanup before return".to_string(),
                    ));
                }
                for defer_expr in defers.into_iter().rev() {
                    let _ = self.lower_expression(&defer_expr);
                }

                self.set_terminator(IrTerminator::Return(value));
            }
            Statement::VariableDecl { name, ty, init } => {
                self.push_instruction(IrInstruction::Comment(format!("local var: {}", name)));
                let lowered_ty = self.lower_type(ty);
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
                self.lower_block(block);
            }
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                self.lower_if(cond, then_block, else_branch.as_deref());
            }
            Statement::While { cond, body } => {
                self.lower_while(cond, body);
            }
            Statement::Break => {
                if let Some((_, break_label)) = self.loop_labels.last() {
                    self.set_terminator(IrTerminator::Jump(break_label.clone()));
                } else {
                    self.context
                        .diagnostics
                        .error("break outside of loop".to_string());
                }
            }
            Statement::Continue => {
                if let Some((continue_label, _)) = self.loop_labels.last() {
                    self.set_terminator(IrTerminator::Jump(continue_label.clone()));
                } else {
                    self.context
                        .diagnostics
                        .error("continue outside of loop".to_string());
                }
            }
            Statement::Defer(expr) => {
                // Record the cleanup logic instead of emitting here
                self.push_instruction(IrInstruction::Comment(
                    "defer: register cleanup logic".to_string(),
                ));
                self.defers.push(expr.clone());
            }
        }
    }

    fn lower_if(&mut self, cond: &Expression, then_block: &Block, else_branch: Option<&Statement>) {
        self.push_instruction(IrInstruction::Comment("if condition".to_string()));
        let cond_value = match self.lower_expression(cond) {
            Some(lowered) => lowered.value,
            None => {
                self.context
                    .diagnostics
                    .error("failed to lower if condition".to_string());
                IrValue::Bool(false)
            }
        };

        let then_label = self.new_label();
        let else_label = self.new_label();
        let merge_label = self.new_label();

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

        self.start_new_block(then_label.0);
        self.lower_block(then_block);
        self.set_terminator(IrTerminator::Jump(merge_label.clone()));

        if let Some(else_stmt) = else_branch {
            self.start_new_block(else_label.0);
            self.lower_statement(else_stmt);
            self.set_terminator(IrTerminator::Jump(merge_label.clone()));
        }

        self.start_new_block(merge_label.0);
    }

    fn lower_while(&mut self, cond: &Expression, body: &Block) {
        let cond_label = self.new_label();
        let body_label = self.new_label();
        let exit_label = self.new_label();

        self.set_terminator(IrTerminator::Jump(cond_label.clone()));

        self.start_new_block(cond_label.0.clone());
        self.push_instruction(IrInstruction::Comment("while condition".to_string()));
        let cond_value = match self.lower_expression(cond) {
            Some(lowered) => lowered.value,
            None => {
                self.context
                    .diagnostics
                    .error("failed to lower while condition".to_string());
                IrValue::Bool(false)
            }
        };

        self.set_terminator(IrTerminator::Branch {
            cond: cond_value,
            then_label: body_label.clone(),
            else_label: exit_label.clone(),
        });

        self.start_new_block(body_label.0);
        self.loop_labels
            .push((cond_label.clone(), exit_label.clone()));
        self.lower_block(body);
        self.loop_labels.pop();
        self.set_terminator(IrTerminator::Jump(cond_label.clone()));

        self.start_new_block(exit_label.0);
    }

    fn lower_expression(&mut self, expression: &Expression) -> Option<LoweredValue> {
        match expression {
            Expression::Primary(primary) => match primary {
                crate::high_level_language::ast::PrimaryExpr::Identifier(name) => {
                    let info = self.context.symbols.lookup(name).cloned();
                    if let Some(info) = info {
                        if let IrType::Pointer(ref inner_ty) = info.ty {
                            // If it's a pointer to local variable, we need to load its value!
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

                        Some(LoweredValue {
                            value: info.value,
                            ty: info.ty,
                        })
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
                    let base = self.lower_expression(expr)?;
                    self.lower_field_access(&base, field)
                }
                crate::high_level_language::ast::PrimaryExpr::ArrayIndex { expr, index } => {
                    let base = self.lower_expression(expr)?;
                    let idx = self.lower_expression(index)?;
                    self.lower_array_index(&base, &idx)
                }
                crate::high_level_language::ast::PrimaryExpr::New { ty, args: _ } => {
                    let dest = self.new_temp();
                    let lowered_ty = self.lower_type(ty);
                    self.push_instruction(IrInstruction::HeapAlloc {
                        dest: dest.clone(),
                        ty: lowered_ty.clone(),
                        count: None,
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
                    Some(LoweredValue {
                        value: IrValue::Register(dest),
                        ty: IrType::Void,
                    }) // Simplified void
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
                    unsupported => {
                        self.context.diagnostics.error(format!(
                            "assignment target lowering not implemented: {unsupported:?}"
                        ));
                        None
                    }
                }
            }
            Expression::Tuple(_) => {
                self.context
                    .diagnostics
                    .error(format!("tuple expression lowering not implemented"));
                None
            }
        }
    }

    fn lower_literal(&self, literal: &Literal) -> LoweredValue {
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

    fn lower_binary(
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

    fn lower_unary(&mut self, op: &UnaryOp, input: LoweredValue) -> Option<LoweredValue> {
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
                let dest = self.new_temp();
                let ptr_reg = match input.value {
                    IrValue::Register(reg) => reg,
                    _ => self.new_temp(),
                };
                self.push_instruction(IrInstruction::Load {
                    dest: dest.clone(),
                    ty: match &input.ty {
                        IrType::Pointer(inner) => *inner.clone(),
                        other => other.clone(),
                    },
                    ptr: ptr_reg,
                    offset: None,
                });
                Some(LoweredValue {
                    value: IrValue::Register(dest),
                    ty: match input.ty {
                        IrType::Pointer(inner) => *inner,
                        _ => IrType::Named("unknown".to_owned()),
                    },
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

    fn lower_field_access(&mut self, base: &LoweredValue, field: &str) -> Option<LoweredValue> {
        match &base.ty {
            IrType::Aggregate(fields) => {
                if let IrValue::Register(ptr_reg) = &base.value {
                    let mut offset = 0i64;
                    let mut found_type = None;
                    for (idx, field_ty) in fields.iter().enumerate() {
                        if idx.to_string() == field {
                            found_type = Some(field_ty.clone());
                            break;
                        }
                        offset += 8;
                    }
                    if let Some(ty) = found_type {
                        let dest = self.new_temp();
                        self.push_instruction(IrInstruction::Load {
                            dest: dest.clone(),
                            ty: ty.clone(),
                            ptr: ptr_reg.clone(),
                            offset: Some(offset),
                        });
                        return Some(LoweredValue {
                            value: IrValue::Register(dest),
                            ty,
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn lower_array_index(
        &mut self,
        base: &LoweredValue,
        index: &LoweredValue,
    ) -> Option<LoweredValue> {
        match &base.ty {
            IrType::Array { element, .. } | IrType::Pointer(element) => {
                if let IrValue::Register(ptr_reg) = &base.value {
                    let dest = self.new_temp();
                    self.push_instruction(IrInstruction::Offset {
                        dest: dest.clone(),
                        ty: element.as_ref().clone(),
                        ptr: ptr_reg.clone(),
                        bytes: index.value.clone(),
                    });
                    let result = self.new_temp();
                    self.push_instruction(IrInstruction::Load {
                        dest: result.clone(),
                        ty: element.as_ref().clone(),
                        ptr: dest,
                        offset: None,
                    });
                    return Some(LoweredValue {
                        value: IrValue::Register(result),
                        ty: element.as_ref().clone(),
                    });
                }
                None
            }
            _ => None,
        }
    }

    fn lower_deref_assign(
        &mut self,
        target: &AssignTarget,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        match target {
            AssignTarget::Identifier(name) => {
                let ptr_info = self.context.symbols.lookup(name)?;
                if let IrValue::Register(ptr_reg) = &ptr_info.value {
                    self.push_instruction(IrInstruction::Store {
                        ty: value.ty.clone(),
                        value: value.value.clone(),
                        ptr: ptr_reg.clone(),
                        offset: None,
                    });
                    return Some(value.clone());
                }
                None
            }
            AssignTarget::Dereference(inner) => self.lower_deref_assign(inner, value),
            _ => None,
        }
    }

    fn lower_field_assign(
        &mut self,
        expr: &AssignTarget,
        _field: &str,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        match expr {
            AssignTarget::Identifier(name) => {
                let base_reg = self.context.symbols.lookup(name)?.value.clone();
                if let IrValue::Register(base_reg_val) = base_reg {
                    self.push_instruction(IrInstruction::Store {
                        ty: value.ty.clone(),
                        value: value.value.clone(),
                        ptr: base_reg_val,
                        offset: Some(0),
                    });
                    return Some(value.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn lower_array_index_assign(
        &mut self,
        expr: &AssignTarget,
        index: &Expression,
        value: &LoweredValue,
    ) -> Option<LoweredValue> {
        match expr {
            AssignTarget::Identifier(name) => {
                let idx = self.lower_expression(index)?;
                let base_reg = self.context.symbols.lookup(name)?.value.clone();
                if let IrValue::Register(base_reg_val) = base_reg {
                    let offset_reg = self.new_temp();
                    self.push_instruction(IrInstruction::Offset {
                        dest: offset_reg.clone(),
                        ty: value.ty.clone(),
                        ptr: base_reg_val,
                        bytes: idx.value,
                    });
                    self.push_instruction(IrInstruction::Store {
                        ty: value.ty.clone(),
                        value: value.value.clone(),
                        ptr: offset_reg,
                        offset: None,
                    });
                    return Some(value.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn new_label(&mut self) -> IrLabel {
        let current = self.next_label;
        self.next_label = self.next_label.saturating_add(1);
        IrLabel::new(format!("label_{}", current))
    }

    fn new_temp(&mut self) -> IrRegister {
        let current = self.next_temp;
        self.next_temp = self.next_temp.saturating_add(1);
        IrRegister::Temp(current)
    }
}
