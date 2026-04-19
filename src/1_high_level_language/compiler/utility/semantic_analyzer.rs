use crate::high_level_language::ast::{
    DeclNode, Declaration, Expression, Program, ReturnType, Statement, Type, UnaryOp,
};
use crate::high_level_language::compiler::utility::diagnostics::Diagnostics;
use crate::high_level_language::compiler::utility::symbol_table::SymbolTable;
use crate::high_level_language::compiler::utility::type_context::TypeContext;
use crate::intermediate_language::IrType;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SemanticAnalyzer {
    context: TypeContext,
    symbols: SymbolTable,
    diagnostics: Diagnostics,
    /// Map of type names to their original AST type strings
    type_mapping: HashMap<String, String>,
    /// Map of function names to their return type strings
    function_signatures: HashMap<String, String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            context: TypeContext::new(),
            symbols: SymbolTable::new(),
            diagnostics: Diagnostics::new(),
            type_mapping: HashMap::new(),
            function_signatures: HashMap::new(),
        }
    }

    pub fn analyze_program(&mut self, program: &Program) -> Result<(), ()> {
        log::info!(
            "Starting semantic analysis for {} declarations",
            program.declarations.len()
        );

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
            DeclNode::Const { name, init } => {
                // Infer the type from the initializer expression
                if let Ok(ty_name) = self.infer_expression_type(init) {
                    self.type_mapping.insert(name.clone(), ty_name);
                } else {
                    // If we can't infer, default to unknown
                    self.type_mapping
                        .insert(name.clone(), "unknown".to_string());
                }
            }
            DeclNode::Function {
                name, return_type, ..
            } => {
                self.type_mapping.insert(name.clone(), "fn".to_string());
                // Store the return type for function calls
                let return_ty_str = match return_type {
                    Some(rt) => match rt {
                        ReturnType::Single(ty) => {
                            let ir_ty = self.ast_type_to_ir_type(ty);
                            self.context.get_type_name(&ir_ty)
                        }
                        ReturnType::Tuple(fields) => {
                            // Convert tuple fields to a tuple type string like "(i32, i32)"
                            let field_types: Vec<String> = fields
                                .iter()
                                .map(|f| {
                                    let ir_ty = self.ast_type_to_ir_type(&f.ty);
                                    self.context.get_type_name(&ir_ty)
                                })
                                .collect();
                            // Use parentheses format for tuples to distinguish from aggregates
                            format!("({})", field_types.join(", "))
                        }
                    },
                    None => "void".to_string(),
                };
                self.function_signatures.insert(name.clone(), return_ty_str);
            }
            _ => {}
        }
    }

    fn check_declaration(&mut self, decl: &Declaration) -> Result<(), ()> {
        match &decl.decl {
            DeclNode::Function { params, body, .. } => {
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
                    self.diagnostics
                        .error(format!("If condition must be bool, found {}", cond_ty));
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
                    self.diagnostics
                        .error(format!("While condition must be bool, found {}", cond_ty));
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
                crate::high_level_language::ast::PrimaryExpr::Literal(lit) => match lit {
                    crate::high_level_language::ast::Literal::Integer(_)
                    | crate::high_level_language::ast::Literal::HexInteger(_) => {
                        Ok("i32".to_string())
                    }
                    crate::high_level_language::ast::Literal::Float(_) => Ok("f64".to_string()),
                    crate::high_level_language::ast::Literal::Boolean(_) => Ok("i1".to_string()),
                    crate::high_level_language::ast::Literal::Null => Ok("*unknown".to_string()),
                    crate::high_level_language::ast::Literal::StringLit(_) => Ok("Str".to_string()),
                },
                crate::high_level_language::ast::PrimaryExpr::New { ty, .. } => {
                    let ir_ty = self.ast_type_to_ir_type(ty);
                    let inner_name = self.context.get_type_name(&ir_ty);
                    Ok(format!("*{}", inner_name))
                }
                crate::high_level_language::ast::PrimaryExpr::FunctionCall {
                    name,
                    arguments,
                    ..
                } => {
                    // Type check arguments
                    for arg in arguments {
                        let _ = self.infer_expression_type(arg)?;
                    }
                    // Look up the function's return type
                    if let Some(return_ty) = self.function_signatures.get(name) {
                        Ok(return_ty.clone())
                    } else {
                        // Unknown function, return unknown
                        Ok("unknown".to_string())
                    }
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
                match op {
                    UnaryOp::AddressOf => {
                        // Special case for AddressOf: we need the pointer type of the operand,
                        // not the dereferenced value type
                        if let Expression::Primary(
                            crate::high_level_language::ast::PrimaryExpr::Identifier(name),
                        ) = inner.as_ref()
                        {
                            if let Some(info) = self.symbols.lookup(name) {
                                // Get the actual stored type (which includes the pointer)
                                let ty_name = self.context.get_type_name(&info.ty);
                                // AddressOf adds another level of pointer, so we prefix with '*'
                                Ok(format!("*{}", ty_name))
                            } else {
                                self.diagnostics
                                    .error(format!("Undefined identifier: {}", name));
                                Err(())
                            }
                        } else {
                            // For non-identifiers, use normal inference
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
                    }
                    _ => {
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
                }
            }
            Expression::Assignment { target, rvalue } => {
                let rvalue_type = self.infer_expression_type(rvalue)?;

                // Handle tuple destructuring
                if let crate::high_level_language::ast::AssignTarget::Tuple(fields) =
                    target.as_ref()
                {
                    // Parse the tuple type string to extract field types
                    let field_types = match self.parse_tuple_type(&rvalue_type) {
                        Ok(types) => types,
                        Err(_) => {
                            self.diagnostics.error(format!(
                                "Expected tuple type for destructuring, got: {}",
                                rvalue_type
                            ));
                            return Err(());
                        }
                    };

                    if fields.len() != field_types.len() {
                        self.diagnostics.error(format!(
                            "Tuple destructuring mismatch: expected {} fields, got {}",
                            field_types.len(),
                            fields.len()
                        ));
                        return Err(());
                    }

                    // Register each target with its corresponding type
                    for (field, field_ty) in fields.iter().zip(field_types.iter()) {
                        // If type annotation is provided, use it; otherwise use inferred type
                        let ty_to_use = if let Some(ref annotated_ty) = field.ty {
                            let ir_ty = self.ast_type_to_ir_type(annotated_ty);
                            self.context.get_type_name(&ir_ty)
                        } else {
                            field_ty.clone()
                        };
                        
                        let target = crate::high_level_language::ast::AssignTarget::Identifier(field.name.clone());
                        self.register_assign_target(&target, &ty_to_use)?;
                    }
                } else {
                    // Regular assignment - check target exists
                    self.check_assign_target(target)?;
                }

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
            Type::Pointer(inner) => IrType::Pointer(Box::new(self.ast_type_to_ir_type(inner))),
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

    /// Parse a tuple type string like "(i32, i32)" into a vector of field types
    fn parse_tuple_type(&self, type_str: &str) -> Result<Vec<String>, ()> {
        let trimmed = type_str.trim();
        if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
            return Err(());
        }

        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.is_empty() {
            return Ok(Vec::new());
        }

        // Simple split by comma - this works for simple types
        let types: Vec<String> = inner.split(',').map(|s| s.trim().to_string()).collect();
        Ok(types)
    }

    /// Register an assignment target (for tuple destructuring)
    fn register_assign_target(
        &mut self,
        target: &crate::high_level_language::ast::AssignTarget,
        ty: &str,
    ) -> Result<(), ()> {
        match target {
            crate::high_level_language::ast::AssignTarget::Identifier(name) => {
                // Check if already defined
                if self.symbols.lookup(name).is_some() {
                    // Variable exists, just verify type compatibility
                    let info = self.symbols.lookup(name).unwrap();
                    let existing_ty = self.context.get_type_name(&info.ty);
                    if existing_ty != ty {
                        self.diagnostics.error(format!(
                            "Type mismatch in reassignment of `{}`: expected {}, found {}",
                            name, existing_ty, ty
                        ));
                        return Err(());
                    }
                } else {
                    // New variable - register it
                    let ir_ty = self.parse_type_string(ty);
                    self.symbols.insert(
                        name.clone(),
                        ir_ty,
                        crate::intermediate_language::IrValue::Null,
                    );
                }
                Ok(())
            }
            crate::high_level_language::ast::AssignTarget::Dereference(inner) => {
                // For @x, check that x is a pointer
                self.check_assign_target(inner)
            }
            crate::high_level_language::ast::AssignTarget::FieldAccess { .. }
            | crate::high_level_language::ast::AssignTarget::ArrayIndex { .. } => {
                // These should already exist
                self.check_assign_target(target)
            }
            crate::high_level_language::ast::AssignTarget::Tuple(_) => {
                self.diagnostics.error(
                    "Nested tuple destructuring not supported in semantic analysis".to_string(),
                );
                Err(())
            }
        }
    }

    /// Check that an assignment target is valid (exists and is assignable)
    fn check_assign_target(
        &mut self,
        target: &crate::high_level_language::ast::AssignTarget,
    ) -> Result<(), ()> {
        match target {
            crate::high_level_language::ast::AssignTarget::Identifier(name) => {
                if self.symbols.lookup(name).is_none() {
                    self.diagnostics
                        .error(format!("Undefined identifier: {}", name));
                    return Err(());
                }
                Ok(())
            }
            crate::high_level_language::ast::AssignTarget::Dereference(inner) => {
                self.check_assign_target(inner)
            }
            crate::high_level_language::ast::AssignTarget::FieldAccess { expr, field: _ } => {
                self.check_assign_target(expr)
            }
            crate::high_level_language::ast::AssignTarget::ArrayIndex { expr, index } => {
                self.check_assign_target(expr)?;
                let _ = self.infer_expression_type(index)?;
                Ok(())
            }
            crate::high_level_language::ast::AssignTarget::Tuple(_) => {
                self.diagnostics
                    .error("Nested tuple destructuring not supported".to_string());
                Err(())
            }
        }
    }

    /// Parse a type string back into an IrType
    fn parse_type_string(&self, ty_str: &str) -> IrType {
        match ty_str {
            "i1" => IrType::Integer(crate::intermediate_language::IntWidth::I1),
            "i8" => IrType::Integer(crate::intermediate_language::IntWidth::I8),
            "i16" => IrType::Integer(crate::intermediate_language::IntWidth::I16),
            "i32" => IrType::Integer(crate::intermediate_language::IntWidth::I32),
            "i64" => IrType::Integer(crate::intermediate_language::IntWidth::I64),
            "f32" => IrType::Float(crate::intermediate_language::FloatWidth::F32),
            "f64" => IrType::Float(crate::intermediate_language::FloatWidth::F64),
            "bool" => IrType::Integer(crate::intermediate_language::IntWidth::I1),
            other => {
                if let Some(inner) = other.strip_prefix('*') {
                    IrType::Pointer(Box::new(self.parse_type_string(inner)))
                } else {
                    IrType::Named(other.to_string())
                }
            }
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
