use crate::high_level_language::ast::{
    Program, Statement, Declaration, DeclNode, Expression, Type,
};
use crate::high_level_language::compiler::diagnostics::Diagnostics;
use crate::high_level_language::compiler::symbol_table::SymbolTable;
use crate::high_level_language::compiler::type_context::TypeContext;
use crate::intermediate_language::IrType;
use std::collections::HashMap;

/// Performs semantic analysis and type checking on an AST before lowering to IR
#[derive(Debug)]
pub struct SemanticAnalyzer {
    context: TypeContext,
    symbols: SymbolTable,
    diagnostics: Diagnostics,
    type_mapping: HashMap<String, String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            context: TypeContext::new(),
            symbols: SymbolTable::new(),
            diagnostics: Diagnostics::new(),
            type_mapping: HashMap::new(),
        }
    }

    pub fn analyze_program(&mut self, program: &Program) -> Result<(), ()> {
        log::info!("Starting semantic analysis for {} declarations", program.declarations.len());

        // First pass: register all type declarations
        for declaration in &program.declarations {
            self.register_declaration(declaration);
        }

        // Second pass: type check all declarations
        for declaration in &program.declarations {
            self.check_declaration(declaration)?;
        }

        if self.diagnostics.has_errors() {
            return Err(());
        }

        Ok(())
    }

    fn register_declaration(&mut self, decl: &Declaration) {
        match &decl.decl {
            DeclNode::Type { name, ty, .. } => {
                let ir_ty = self.ast_type_to_ir_type(ty);
                self.context.register_type(name.clone(), ir_ty);
                self.type_mapping.insert(name.clone(), format!("{:?}", ty));
            }
            DeclNode::Const { name, .. } => {
                self.type_mapping.insert(name.clone(), "const".to_string());
            }
            DeclNode::Function { name, .. } => {
                self.type_mapping.insert(name.clone(), "fn".to_string());
            }
            _ => {}
        }
    }

    fn check_declaration(&mut self, decl: &Declaration) -> Result<(), ()> {
        match &decl.decl {
            DeclNode::Function {
                params,
                body,
                ..
            } => {
                self.symbols.enter_scope();

                // Register parameters
                for param in params {
                    let ir_ty = self.ast_type_to_ir_type(&param.ty);
                    let _ty_name = self.context.get_type_name(&ir_ty);
                    self.symbols.insert(
                        param.name.clone(),
                        ir_ty,
                        crate::intermediate_language::IrValue::Null,
                    );
                }

                // Check function body
                if let Some(body_block) = body {
                    self.check_block(body_block)?;
                }

                self.symbols.exit_scope();
                Ok(())
            }
            DeclNode::Type { .. } | DeclNode::Const { .. } | DeclNode::Variable { .. } => Ok(()),
        }
    }

    fn check_block(&mut self, block: &crate::high_level_language::ast::Block) -> Result<(), ()> {
        for stmt in &block.statements {
            self.check_statement(stmt)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, stmt: &Statement) -> Result<(), ()> {
        match stmt {
            Statement::Expression(expr) => {
                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::Return(Some(expr)) => {
                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::Return(None) => Ok(()),
            Statement::VariableDecl { name, ty, init } => {
                let ir_ty = self.ast_type_to_ir_type(ty);
                let ty_name = self.context.get_type_name(&ir_ty);

                if let Some(init_expr) = init {
                    let init_ty = self.infer_expression_type(init_expr)?;
                    if init_ty != ty_name {
                        self.diagnostics.error(format!(
                            "Type mismatch in variable initialization: expected {}, found {}",
                            ty_name, init_ty
                        ));
                        return Err(());
                    }
                }

                self.symbols.insert(
                    name.clone(),
                    ir_ty,
                    crate::intermediate_language::IrValue::Null,
                );
                Ok(())
            }
            Statement::Block(block) => {
                self.symbols.enter_scope();
                self.check_block(block)?;
                self.symbols.exit_scope();
                Ok(())
            }
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                let cond_ty = self.infer_expression_type(cond)?;
                if cond_ty != "i1" && cond_ty != "bool" {
                    self.diagnostics.error(format!(
                        "If condition must be bool, found {}",
                        cond_ty
                    ));
                    return Err(());
                }

                self.symbols.enter_scope();
                self.check_block(then_block)?;
                self.symbols.exit_scope();

                if let Some(else_stmt) = else_branch {
                    self.symbols.enter_scope();
                    self.check_statement(else_stmt)?;
                    self.symbols.exit_scope();
                }
                Ok(())
            }
            Statement::While { cond, body } => {
                let cond_ty = self.infer_expression_type(cond)?;
                if cond_ty != "i1" && cond_ty != "bool" {
                    self.diagnostics.error(format!(
                        "While condition must be bool, found {}",
                        cond_ty
                    ));
                    return Err(());
                }

                self.symbols.enter_scope();
                self.check_block(body)?;
                self.symbols.exit_scope();
                Ok(())
            }
            Statement::Defer(expr) => {
                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::Break | Statement::Continue => Ok(()),
        }
    }

    fn infer_expression_type(&mut self, expr: &Expression) -> Result<String, ()> {
        match expr {
            Expression::Primary(primary) => match primary {
                crate::high_level_language::ast::PrimaryExpr::Identifier(name) => {
                    if let Some(info) = self.symbols.lookup(name) {
                        // Variables are stored as pointers in memory, so dereference them
                        let ty_name = self.context.get_type_name(&info.ty);
                        if let Some(inner_ty_str) = ty_name.strip_prefix('*') {
                            Ok(inner_ty_str.to_string())
                        } else {
                            Ok(ty_name)
                        }
                    } else if let Some(ty_name) = self.type_mapping.get(name) {
                        Ok(ty_name.clone())
                    } else {
                        self.diagnostics
                            .error(format!("Undefined identifier: {}", name));
                        Err(())
                    }
                }
                crate::high_level_language::ast::PrimaryExpr::Literal(lit) => {
                    match lit {
                        crate::high_level_language::ast::Literal::Integer(_)
                        | crate::high_level_language::ast::Literal::HexInteger(_) => {
                            Ok("i32".to_string())
                        }
                        crate::high_level_language::ast::Literal::Float(_) => Ok("f64".to_string()),
                        crate::high_level_language::ast::Literal::Boolean(_) => Ok("i1".to_string()),
                        crate::high_level_language::ast::Literal::Null => {
                            Ok("*unknown".to_string())
                        }
                        crate::high_level_language::ast::Literal::StringLit(_) => {
                            Ok("Str".to_string())
                        }
                    }
                }
                crate::high_level_language::ast::PrimaryExpr::New { ty, .. } => {
                    let ir_ty = self.ast_type_to_ir_type(ty);
                    let inner_name = self.context.get_type_name(&ir_ty);
                    Ok(format!("*{}", inner_name))
                }
                crate::high_level_language::ast::PrimaryExpr::FunctionCall { arguments, .. } => {
                    // Type check arguments
                    for arg in arguments {
                        let _ = self.infer_expression_type(arg)?;
                    }
                    // Return unknown type for now
                    Ok("unknown".to_string())
                }
                _ => Ok("unknown".to_string()),
            },
            Expression::Binary { op, left, right } => {
                let lhs_type = self.infer_expression_type(left)?;
                let rhs_type = self.infer_expression_type(right)?;

                match self.context.check_binary_op(op, &lhs_type, &rhs_type) {
                    Ok(result_type) => Ok(result_type),
                    Err(err) => {
                        self.diagnostics
                            .error(format!("Type error in binary operation: {:?}", err));
                        Err(())
                    }
                }
            }
            Expression::Unary { op, expr: inner } => {
                let inner_type = self.infer_expression_type(inner)?;

                match self.context.check_unary_op(op, &inner_type) {
                    Ok(result_type) => Ok(result_type),
                    Err(err) => {
                        self.diagnostics
                            .error(format!("Type error in unary operation: {:?}", err));
                        Err(())
                    }
                }
            }
            Expression::Assignment { target: _, rvalue } => {
                let rvalue_type = self.infer_expression_type(rvalue)?;
                // Assignment returns the type of the right side
                Ok(rvalue_type)
            }
            Expression::Tuple(elements) => {
                let mut field_types = Vec::new();
                for elem in elements {
                    let elem_type = self.infer_expression_type(elem)?;
                    field_types.push(elem_type);
                }
                Ok(format!("({})", field_types.join(", ")))
            }
        }
    }

    fn ast_type_to_ir_type(&self, ty: &Type) -> IrType {
        match ty {
            Type::Primitive(name) => self.primitive_to_ir(name),
            Type::Pointer(inner) => {
                IrType::Pointer(Box::new(self.ast_type_to_ir_type(inner)))
            }
            Type::Array(len, inner) => IrType::Array {
                len: *len,
                element: Box::new(self.ast_type_to_ir_type(inner)),
            },
            Type::Struct(fields) => {
                let field_types: Vec<(String, IrType)> = fields
                    .iter()
                    .map(|f| (f.name.clone(), self.ast_type_to_ir_type(&f.ty)))
                    .collect();
                IrType::Aggregate(field_types)
            }
            Type::Named { name, .. } => IrType::Named(name.clone()),
        }
    }

    fn primitive_to_ir(&self, name: &str) -> IrType {
        match name {
            "i8" => IrType::Integer(crate::intermediate_language::IntWidth::I8),
            "i16" => IrType::Integer(crate::intermediate_language::IntWidth::I16),
            "i32" => IrType::Integer(crate::intermediate_language::IntWidth::I32),
            "i64" => IrType::Integer(crate::intermediate_language::IntWidth::I64),
            "u8" => IrType::Integer(crate::intermediate_language::IntWidth::I8),
            "u16" => IrType::Integer(crate::intermediate_language::IntWidth::I16),
            "u32" => IrType::Integer(crate::intermediate_language::IntWidth::I32),
            "u64" => IrType::Integer(crate::intermediate_language::IntWidth::I64),
            "f32" => IrType::Float(crate::intermediate_language::FloatWidth::F32),
            "f64" => IrType::Float(crate::intermediate_language::FloatWidth::F64),
            "bool" => IrType::Integer(crate::intermediate_language::IntWidth::I1),
            other => IrType::Named(other.to_string()),
        }
    }

    pub fn diagnostics(&self) -> &[crate::high_level_language::compiler::Diagnostic] {
        self.diagnostics.entries()
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}






