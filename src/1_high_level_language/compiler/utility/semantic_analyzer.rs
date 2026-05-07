use crate::high_level_language::ast::*;
use crate::high_level_language::compiler::*;
use crate::intermediate_language::IrType;
use std::collections::{HashMap, HashSet};
use utility::diagnostics::Diagnostics;
use utility::symbol_table::SymbolTable;
use utility::type_context::TypeContext;
use {DeclNode, Declaration, Expression, Program, ReturnType, Statement, Type, UnaryOp};

#[derive(Debug)]
pub struct SemanticAnalyzer {
    context: TypeContext,
    symbols: SymbolTable,
    diagnostics: Diagnostics,
    type_mapping: HashMap<String, String>,
    function_signatures: HashMap<String, String>,
    current_function: Option<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            context: TypeContext::new(),
            symbols: SymbolTable::new(),
            diagnostics: Diagnostics::new(),
            type_mapping: HashMap::new(),
            function_signatures: HashMap::new(),
            current_function: None,
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let msg = message.into();
        let full = match &self.current_function {
            Some(fn_name) => format!("in function '{fn_name}': {msg}"),
            None => msg,
        };
        self.diagnostics.error(full);
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

        // Post-resolution check for unresolved types (optional strict mode)
        // self.check_for_unresolved_unknowns(program)?;

        Ok(())
    }

    fn register_declaration(&mut self, decl: &Declaration) {
        match &decl.decl {
            DeclNode::Type { name, ty, .. } => {
                let ir_ty = self.ast_type_to_ir_type(ty);
                self.context.register_type(name.clone(), ir_ty);
                self.type_mapping.insert(name.clone(), format!("{ty:?}"));
            }
            DeclNode::Const { name, init } => {
                // Infer the type from the initializer expression
                if let Ok(ty_name) = self.infer_expression_type(init) {
                    self.type_mapping.insert(name.clone(), ty_name);
                } else {
                    // If we can't infer, default to unknown
                    self.type_mapping.insert(name.clone(), "unknown".to_owned());
                }
            }
            DeclNode::Function {
                name, return_type, ..
            } => {
                self.type_mapping.insert(name.clone(), "fn".to_owned());
                // Store the return type for function calls
                let return_ty_str = match return_type {
                    Some(rt) => match rt {
                        ReturnType::Single(ty) => {
                            let ir_ty = self.ast_type_to_ir_type(ty);
                            self.context.get_type_name(&ir_ty)
                        }
                    },
                    None => "void".to_owned(),
                };
                self.function_signatures.insert(name.clone(), return_ty_str);
            }
            _ => {}
        }
    }

    fn check_declaration(&mut self, decl: &Declaration) -> Result<(), ()> {
        match &decl.decl {
            DeclNode::Function {
                name, params, body, ..
            } => {
                self.current_function = Some(name.clone());
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
                self.current_function = None;
                Ok(())
            }
            DeclNode::Type { .. } | DeclNode::Const { .. } => Ok(()),
            DeclNode::Variable { name, ty, init: _ } => {
                let ir_ty = self.ast_type_to_ir_type(ty);
                self.symbols.insert(
                    name.clone(),
                    ir_ty,
                    crate::intermediate_language::IrValue::Null,
                );
                Ok(())
            }
        }
    }

    fn check_block(&mut self, block: &Block) -> Result<(), ()> {
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
                if let Some(name) = self.returning_local_address_name(expr) {
                    self.error(format!(
                        "Returning address of local `{name}` is not allowed"
                    ));
                    return Err(());
                }

                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::Return(None) => Ok(()),
            Statement::VariableDecl { name, ty, init } => {
                let ir_ty = self.ast_type_to_ir_type(ty);

                if let Some(init_expr) = init {
                    let init_ty = self.infer_expression_type(init_expr)?;
                    let resolved_decl_ty =
                        self.resolve_type_string(&self.context.get_type_name(&ir_ty));
                    let resolved_init_ty = self.resolve_type_string(&init_ty);

                    // Allow integer literal widening
                    let is_literal_widening_allowed = Self::is_integer_type(&resolved_decl_ty)
                        && Self::is_i32_type(&resolved_init_ty)
                        && Self::is_literal_source(init_expr);

                    if resolved_decl_ty != resolved_init_ty && !is_literal_widening_allowed {
                        self.error(format!(
                            "Type mismatch in variable initialization: expected {}, found {}",
                            self.context.get_type_name(&ir_ty),
                            init_ty
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
                        .error(format!("If condition must be bool, found {cond_ty}"));
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
                        .error(format!("While condition must be bool, found {cond_ty}"));
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
                PrimaryExpr::Identifier(name) => {
                    if let Some(info) = self.symbols.lookup(name) {
                        Ok(self.context.get_type_name(&info.ty))
                    } else if let Some(ty_name) = self.type_mapping.get(name) {
                        Ok(ty_name.clone())
                    } else {
                        self.diagnostics
                            .error(format!("Undefined identifier: {name}"));
                        Err(())
                    }
                }
                PrimaryExpr::Literal(lit) => match lit {
                    Literal::Integer(_) | Literal::HexInteger(_) => Ok("i32".to_owned()),
                    Literal::Float(_) => Ok("f32".to_owned()),
                    Literal::Boolean(_) => Ok("i1".to_owned()),
                    Literal::Null => Ok("*unknown".to_owned()),
                    Literal::String(_) => Ok("{ data: u8*, length: u64 }".to_owned()),
                },
                PrimaryExpr::Grouped(expr) => self.infer_expression_type(expr),
                PrimaryExpr::New { ty, .. } => {
                    let ir_ty = self.ast_type_to_ir_type(ty);
                    let inner_name = self.context.get_type_name(&ir_ty);
                    Ok(format!("*{inner_name}"))
                }
                PrimaryExpr::FunctionCall {
                    name, arguments, ..
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
                        Ok("unknown".to_owned())
                    }
                }
                PrimaryExpr::ArrayLiteral(elements) => {
                    if elements.is_empty() {
                        self.diagnostics
                            .error("empty array literals are not supported yet".to_owned());
                        return Err(());
                    }

                    let mut inferred_element_ty: Option<String> = None;
                    for element in elements {
                        let element_ty = self.infer_expression_type(element)?;
                        if let Some(expected_ty) = &inferred_element_ty {
                            let resolved_expected = self.resolve_type_string(expected_ty);
                            let resolved_actual = self.resolve_type_string(&element_ty);
                            if resolved_expected != resolved_actual {
                                self.error(format!(
                                    "array literal element type mismatch: expected {expected_ty}, found {element_ty}"
                                ));
                                return Err(());
                            }
                        } else {
                            inferred_element_ty = Some(element_ty);
                        }
                    }

                    let element_ty = inferred_element_ty.expect("array must have at least one typed element");
                    Ok(format!("{}[{}]", element_ty, elements.len()))
                }
                PrimaryExpr::FieldAccess { expr, field } => {
                    let base_ty = self.infer_expression_type(expr)?;
                    self.infer_field_access_type(&base_ty, field)
                }
                PrimaryExpr::ArrayIndex { expr, index } => {
                    let _ = self.infer_expression_type(index)?;
                    let base_ty = match expr.as_ref() {
                        Expression::Unary {
                            op: UnaryOp::Dereference,
                            expr: inner,
                        } => self.infer_expression_type(inner)?,
                        _ => self.infer_expression_type(expr)?,
                    };
                    if matches!(
                        expr.as_ref(),
                        Expression::Unary {
                            op: UnaryOp::Dereference,
                            ..
                        }
                    ) {
                        let indexed_ty = self.infer_index_element_type(&base_ty)?;
                        Ok(indexed_ty
                            .strip_prefix('*')
                            .map_or(indexed_ty.clone(), ToString::to_string))
                    } else {
                        self.infer_index_element_type(&base_ty)
                    }
                }
                PrimaryExpr::StructLiteral(fields) => {
                    let mut field_types = Vec::new();
                    for field in fields {
                        let expr_ty = self.infer_expression_type(&field.expr)?;
                        let inferred_ir = if let Some(annotated_ty) = &field.ty {
                            let annotated_ir = self.ast_type_to_ir_type(annotated_ty);
                            let annotated_name = self.context.get_type_name(&annotated_ir);
                            if expr_ty != annotated_name
                                && expr_ty != "unknown"
                                && expr_ty != "*unknown"
                            {
                                self.error(format!(
                                    "Type mismatch in struct literal field `{}`: expected {}, found {}",
                                    field.name, annotated_name, expr_ty
                                ));
                                return Err(());
                            }
                            annotated_ir
                        } else {
                            self.parse_type_string(&expr_ty)
                        };
                        field_types.push((field.name.clone(), inferred_ir));
                    }
                    Ok(self.context.get_type_name(&IrType::Aggregate(field_types)))
                }
            },
            Expression::Binary { op, left, right } => {
                let lhs_type = self.infer_expression_type(left)?;
                let rhs_type = self.infer_expression_type(right)?;

                match self.context.check_binary_op(op, &lhs_type, &rhs_type) {
                    Ok(result_type) => Ok(result_type),
                    Err(err) => {
                        self.diagnostics
                            .error(format!("Type error in binary operation: {err}"));
                        Err(())
                    }
                }
            }
            Expression::Unary { op, expr: inner } => {
                if op == &UnaryOp::AddressOf {
                    if self.contains_dereference(inner) {
                        self.error(
                            "cannot take address of a dereference expression (`&@...` is invalid)"
                                .to_owned(),
                        );
                        return Err(());
                    }

                    if self.stack_address_root_name(inner).is_some() {
                        // Special case for AddressOf: we need the pointer type of the operand,
                        // not the dereferenced value type.
                        if let Expression::Primary(PrimaryExpr::Identifier(name)) = inner.as_ref() {
                            if let Some(info) = self.symbols.lookup(name) {
                                let ty_name = self.context.get_type_name(&info.ty);
                                return Ok(format!("*{ty_name}"));
                            }

                            self.diagnostics
                                .error(format!("Undefined identifier: {name}"));
                            return Err(());
                        }

                        let inner_type = self.infer_expression_type(inner)?;
                        return match self.context.check_unary_op(op, &inner_type) {
                            Ok(result_type) => Ok(result_type),
                            Err(err) => {
                                self.diagnostics
                                    .error(format!("Type error in unary operation: {err}"));
                                Err(())
                            }
                        };
                    } else {
                        self.error(
                            "address-of requires an assignable l-value (identifier, field access, or array element)".to_owned(),
                        );
                        Err(())
                    }
                } else {
                    let inner_type = self.infer_expression_type(inner)?;
                    match self.context.check_unary_op(op, &inner_type) {
                        Ok(result_type) => Ok(result_type),
                        Err(err) => {
                            self.diagnostics
                                .error(format!("Type error in unary operation: {err}"));
                            Err(())
                        }
                    }
                }
            }
            Expression::Assignment { target, rvalue } => {
                let rvalue_type = self.infer_expression_type(rvalue)?;

                // Handle brace-based struct destructuring.
                if let AssignTarget::StructDestructure(fields) = target.as_ref() {
                    let resolved_rvalue = self.resolve_type_string(&rvalue_type);
                    let field_types = match resolved_rvalue {
                        IrType::Aggregate(types) => types,
                        IrType::Pointer(inner) => {
                            if let IrType::Aggregate(types) = *inner {
                                types
                            } else {
                                self.error(format!(
                                    "Expected struct type for destructuring, got: {rvalue_type}"
                                ));
                                return Err(());
                            }
                        }
                        _ => {
                            self.error(format!(
                                "Expected struct type for destructuring, got: {rvalue_type}"
                            ));
                            return Err(());
                        }
                    };

                    // Per spec v1.4.1: all fields must have explicit type annotations
                    for field in fields {
                        let Some(name) = field.name.as_ref() else {
                            self.error(
                                "Struct destructuring requires explicit variable names".to_owned(),
                            );
                            return Err(());
                        };

                        // Type annotation is required per spec
                        let Some(annotated_ty) = &field.ty else {
                            self.error(format!(
                                "Struct destructuring field `{name}` requires explicit type annotation"
                            ));
                            return Err(());
                        };

                        let ty_to_use = self
                            .context
                            .get_type_name(&self.ast_type_to_ir_type(annotated_ty));

                        // Verify the annotated type matches the inferred type from the struct
                        let inferred_ty = field_types
                            .iter()
                            .find(|(field_name, _)| field_name == name)
                            .map(|(_, ty)| self.context.get_type_name(ty))
                            .unwrap_or_else(|| "unknown".to_owned());

                        if inferred_ty == "unknown" {
                            self.error(format!(
                                "Field `{}` not found in struct type `{}`. Available fields: {}",
                                name,
                                rvalue_type,
                                field_types
                                    .iter()
                                    .map(|(n, _)| n.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            return Err(());
                        }

                        let resolved_ty_to_use = self.resolve_type_string(&ty_to_use);
                        let resolved_inferred_ty = self.resolve_type_string(&inferred_ty);

                        if resolved_ty_to_use != resolved_inferred_ty && inferred_ty != "unknown" {
                            self.error(format!(
                                "Type mismatch in struct destructuring field `{name}`: expected {inferred_ty}, found {ty_to_use}"
                            ));
                            return Err(());
                        }

                        let target = AssignTarget::Identifier(name.clone());
                        self.register_assign_target(&target, &ty_to_use)?;
                    }
                } else {
                    // Regular assignment, check target exists
                    self.check_assign_target(target)?;
                }

                // Assignment returns the type of the right side
                Ok(rvalue_type)
            }
            Expression::Cast { target_ty, expr } => {
                // Infer the type of the expression being cast
                let source_type = self.infer_expression_type(expr)?;

                // Convert target type to IR type
                let target_ir = self.ast_type_to_ir_type(target_ty);
                let target_name = self.context.get_type_name(&target_ir);

                // Validate that the cast is legal
                let source_ir = self.parse_type_string(&source_type);
                if !self.is_valid_cast(&source_ir, &target_ir) {
                    self.error(format!(
                        "Invalid cast from `{source_type}` to `{target_name}`"
                    ));
                    return Err(());
                }

                Ok(target_name)
            }
        }
    }

    fn returning_local_address_name(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Unary {
                op: UnaryOp::AddressOf,
                expr: inner,
            } => self.stack_address_root_name(inner),
            Expression::Primary(PrimaryExpr::Grouped(inner)) => {
                self.returning_local_address_name(inner)
            }
            _ => None,
        }
    }

    fn stack_address_root_name(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Primary(PrimaryExpr::Identifier(name)) => {
                self.symbols.lookup(name).map(|_| name.clone())
            }
            Expression::Primary(PrimaryExpr::Grouped(inner)) => self.stack_address_root_name(inner),
            Expression::Primary(PrimaryExpr::FieldAccess { expr, .. }) => {
                self.stack_address_root_name(expr)
            }
            Expression::Primary(PrimaryExpr::ArrayIndex { expr, .. }) => {
                self.stack_address_root_name(expr)
            }
            _ => None,
        }
    }

    fn contains_dereference(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Unary {
                op: UnaryOp::Dereference,
                ..
            } => true,
            Expression::Unary { expr, .. } => self.contains_dereference(expr),
            Expression::Binary { left, right, .. } => {
                self.contains_dereference(left) || self.contains_dereference(right)
            }
            Expression::Primary(PrimaryExpr::Grouped(inner)) => self.contains_dereference(inner),
            Expression::Primary(PrimaryExpr::FieldAccess { expr, .. }) => {
                self.contains_dereference(expr)
            }
            Expression::Primary(PrimaryExpr::ArrayIndex { expr, index }) => {
                self.contains_dereference(expr) || self.contains_dereference(index)
            }
            Expression::Primary(PrimaryExpr::FunctionCall { arguments, .. }) => {
                arguments.iter().any(|arg| self.contains_dereference(arg))
            }
            Expression::Primary(PrimaryExpr::StructLiteral(fields)) => fields
                .iter()
                .any(|field| self.contains_dereference(&field.expr)),
            _ => false,
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
            other => IrType::Named(other.to_owned()),
        }
    }

    /// Parse an aggregate type string like `{ a: i32, b: i32 }` into field names and types.
    fn parse_aggregate_type(&self, type_str: &str) -> Result<Vec<(Option<String>, String)>, ()> {
        let trimmed = type_str.trim();
        let (inner, named) = if trimmed.starts_with('{') && trimmed.ends_with('}') {
            (&trimmed[1..trimmed.len() - 1], true)
        } else {
            return Err(());
        };

        if inner.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut parts = Vec::new();
        let mut depth = 0i32;
        let mut start = 0usize;

        for (idx, ch) in inner.char_indices() {
            match ch {
                '{' | '(' | '[' | '<' => depth += 1,
                '}' | ')' | ']' | '>' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(inner[start..idx].trim().to_owned());
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        parts.push(inner[start..].trim().to_owned());

        let mut fields = Vec::new();
        for part in parts {
            if named {
                if let Some((name, ty)) = part.split_once(':') {
                    fields.push((Some(name.trim().to_owned()), ty.trim().to_owned()));
                } else {
                    fields.push((None, part));
                }
            } else {
                fields.push((None, part));
            }
        }

        Ok(fields)
    }

    fn infer_field_access_type(&mut self, base_type: &str, field: &str) -> Result<String, ()> {
        if base_type == "unknown" || base_type == "*unknown" {
            return Ok("unknown".to_owned());
        }

        let resolved = self.resolve_type_string(base_type);

        let (fields, through_pointer) = match resolved {
            IrType::Aggregate(fields) => (fields, false),
            IrType::Pointer(inner) => {
                if let IrType::Aggregate(fields) = *inner {
                    (fields, true)
                } else {
                    self.diagnostics
                        .error(format!("field access on non-aggregate type `{base_type}`"));
                    return Err(());
                }
            }
            _ => {
                self.diagnostics
                    .error(format!("field access on non-aggregate type `{base_type}`"));
                return Err(());
            }
        };

        if let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) {
            let field_type_name = self.context.get_type_name(field_ty);
            if through_pointer {
                Ok(format!("*{field_type_name}"))
            } else {
                Ok(field_type_name)
            }
        } else {
            self.diagnostics
                .error(format!("unknown field `{field}` for type `{base_type}`"));
            Err(())
        }
    }

    fn infer_index_element_type(&mut self, base_type: &str) -> Result<String, ()> {
        // Allow unknown types to propagate only if they're genuinely unresolved
        if base_type == "unknown" || base_type == "*unknown" {
            self.diagnostics.warn(format!(
                "indexing operation on unresolved type `{base_type}`; ensure type is resolved before codegen"
            ));
            return Ok("*unknown".to_owned());
        }

        // Handle pointer-to-array: *T[N] -> *T
        if let Some(inner) = base_type.strip_prefix('*') {
            if let Some((element, _len)) = inner.split_once('[') {
                return Ok(format!("*{element}"));
            }
            // Pointer to non-array: *T -> *T (indexing through pointer returns pointer to element)
            return Ok(format!("*{inner}"));
        }

        // Handle direct array: T[N] -> *T (stack arrays follow the same pointer-element rule)
        if let Some((element, _rest)) = base_type.split_once('[') {
            return Ok(format!("*{element}"));
        }

        // Resolve named types and check the underlying structure
        let resolved = self.resolve_type_string(base_type);
        match &resolved {
            IrType::Pointer(inner) => {
                let inner_name = self.context.get_type_name(inner);
                // Check if the pointed-to type is itself indexable
                if inner_name == "unknown" || inner_name == "*unknown" {
                    self.diagnostics.warn(format!(
                        "indexing through pointer to unresolved type `{inner_name}`"
                    ));
                    return Ok("*unknown".to_owned());
                }
                Ok(format!("*{inner_name}"))
            }
            IrType::Array { element, .. } => {
                let element_name = self.context.get_type_name(element);
                Ok(format!("*{element_name}"))
            }
            IrType::Named(name) if name == "unknown" => {
                self.diagnostics.warn(format!(
                    "indexing operation on unresolved named type `{name}`"
                ));
                Ok("*unknown".to_owned())
            }
            _ => {
                self.error(format!(
                    "indexing non-indexable type `{base_type}` (resolved: `{resolved:?}`)"
                ));
                Err(())
            }
        }
    }

    /// Register an assignment target (for struct destructuring)
    fn register_assign_target(&mut self, target: &AssignTarget, ty: &str) -> Result<(), ()> {
        match target {
            AssignTarget::Identifier(name) => {
                // Check if already defined
                if self.symbols.lookup(name).is_some() {
                    // Variable exists, just verify type compatibility
                    let info = self.symbols.lookup(name).expect("symbol exists: checked above");
                    let existing_ty = self.context.get_type_name(&info.ty);
                    if existing_ty != ty {
                        self.error(format!(
                            "Type mismatch in reassignment of `{name}`: expected {existing_ty}, found {ty}"
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
            AssignTarget::Dereference(inner) => {
                // For @x, check that x is a pointer
                self.check_assign_target(inner)
            }
            AssignTarget::FieldAccess { .. } | AssignTarget::ArrayIndex { .. } => {
                // These should already exist
                self.check_assign_target(target)
            }
            AssignTarget::StructDestructure(_) => {
                self.error(
                    "Nested struct destructuring not supported in semantic analysis".to_owned(),
                );
                Err(())
            }
        }
    }

    /// Check that an assignment target is valid (exists and is assignable)
    fn check_assign_target(&mut self, target: &AssignTarget) -> Result<(), ()> {
        match target {
            AssignTarget::Identifier(name) => {
                if self.symbols.lookup(name).is_none() {
                    self.diagnostics
                        .error(format!("Undefined identifier: {name}"));
                    return Err(());
                }
                Ok(())
            }
            AssignTarget::Dereference(inner) => self.check_assign_target(inner),
            AssignTarget::FieldAccess { expr, field: _ } => self.check_assign_target(expr),
            AssignTarget::ArrayIndex { expr, index } => {
                self.check_assign_target(expr)?;
                let _ = self.infer_expression_type(index)?;
                Ok(())
            }
            AssignTarget::StructDestructure(_) => {
                self.diagnostics
                    .error("Nested struct destructuring not supported".to_owned());
                Err(())
            }
        }
    }

    /// Parse a type string back into an `IrType`
    fn parse_type_string(&self, ty_str: &str) -> IrType {
        let trimmed = ty_str.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(fields) = self.parse_aggregate_type(trimmed) {
                return IrType::Aggregate(
                    fields
                        .into_iter()
                        .map(|(name, field_ty)| {
                            (name.unwrap_or_default(), self.parse_type_string(&field_ty))
                        })
                        .collect(),
                );
            }
        }

        if let Some(inner) = trimmed.strip_suffix('*') {
            if !inner.is_empty() {
                return IrType::Pointer(Box::new(self.parse_type_string(inner)));
            }
        }

        match trimmed {
            "i1" => IrType::Integer(crate::intermediate_language::IntWidth::I1),
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
            other => {
                if let Some(inner) = other.strip_prefix('*') {
                    IrType::Pointer(Box::new(self.parse_type_string(inner)))
                } else {
                    IrType::Named(other.to_owned())
                }
            }
        }
    }

    fn resolve_named_ir_type(&self, ty: &IrType) -> IrType {
        self.resolve_named_ir_type_inner(ty, &mut HashSet::new())
    }

    fn resolve_named_ir_type_inner(&self, ty: &IrType, seen: &mut HashSet<String>) -> IrType {
        match ty {
            IrType::Named(name) => self
                .context
                .resolve(name)
                .cloned()
                .map(|resolved| {
                    if !seen.insert(name.clone()) {
                        IrType::Named(name.clone())
                    } else {
                        let resolved_ty = self.resolve_named_ir_type_inner(&resolved, seen);
                        seen.remove(name);
                        resolved_ty
                    }
                })
                .unwrap_or_else(|| IrType::Named(name.clone())),
            IrType::Pointer(inner) => {
                IrType::Pointer(Box::new(self.resolve_named_ir_type_inner(inner, seen)))
            }
            IrType::Array { len, element } => IrType::Array {
                len: *len,
                element: Box::new(self.resolve_named_ir_type_inner(element, seen)),
            },
            IrType::Aggregate(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|(name, field_ty)| {
                        (
                            name.clone(),
                            self.resolve_named_ir_type_inner(field_ty, seen),
                        )
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    fn resolve_type_string(&self, ty_str: &str) -> IrType {
        self.resolve_named_ir_type(&self.parse_type_string(ty_str))
    }

    /// Check if a cast from source to target type is valid
    fn is_valid_cast(&self, source: &IrType, target: &IrType) -> bool {
        // Allow casts between numeric types (integers and floats)
        let source_is_numeric = matches!(source, IrType::Integer(_) | IrType::Float(_));
        let target_is_numeric = matches!(target, IrType::Integer(_) | IrType::Float(_));

        if source_is_numeric && target_is_numeric {
            return true;
        }

        // Allow pointer to pointer casts
        if matches!(source, IrType::Pointer(_)) && matches!(target, IrType::Pointer(_)) {
            return true;
        }

        // Allow same-type casts (identity)
        if source == target {
            return true;
        }

        // TODO: can still be improved
        false
    }

    fn is_integer_type(ty: &IrType) -> bool {
        matches!(ty, IrType::Integer(_))
    }

    fn is_i32_type(ty: &IrType) -> bool {
        matches!(
            ty,
            IrType::Integer(crate::intermediate_language::IntWidth::I32)
        )
    }

    fn is_literal_source(expr: &Expression) -> bool {
        matches!(
            expr,
            Expression::Primary(PrimaryExpr::Literal(
                Literal::Integer(_) | Literal::HexInteger(_)
            ))
        )
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.entries()
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
