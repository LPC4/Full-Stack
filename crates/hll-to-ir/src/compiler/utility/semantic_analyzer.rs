use crate::LanguageVersion;
use crate::ast::*;
use crate::compiler::*;
use crate::ir::IrType;
use crate::monomorphize::substitute_type;
use std::collections::{HashMap, HashSet};
use utility::diagnostics::Diagnostics;
use utility::symbol_table::SymbolTable;
use utility::type_context::TypeContext;
use {DeclNode, Declaration, Expression, Program, ReturnType, Statement, Type, UnaryOp};

// A V2 enum variant constructor: which enum it belongs to and its payload types.
#[derive(Debug, Clone)]
struct EnumVariantSig {
    enum_name: String,
    payloads: Vec<IrType>,
}

#[derive(Debug)]
pub struct SemanticAnalyzer {
    context: TypeContext,
    symbols: SymbolTable,
    diagnostics: Diagnostics,
    type_mapping: HashMap<String, String>,
    function_signatures: HashMap<String, String>,
    function_parameters: HashMap<String, Vec<IrType>>,
    generic_type_defs: HashMap<String, (Vec<String>, Type)>,
    // V2 enums: variant constructor -> signature, and enum name -> its variant list.
    enum_variants: HashMap<String, EnumVariantSig>,
    enum_variant_names: HashMap<String, Vec<String>>,
    current_function: Option<String>,
    language_version: LanguageVersion,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self::with_language_version(LanguageVersion::V1)
    }

    pub fn with_language_version(language_version: LanguageVersion) -> Self {
        Self {
            context: TypeContext::new(),
            symbols: SymbolTable::new(),
            diagnostics: Diagnostics::new(),
            type_mapping: HashMap::new(),
            function_signatures: HashMap::new(),
            function_parameters: HashMap::new(),
            generic_type_defs: HashMap::new(),
            enum_variants: HashMap::new(),
            enum_variant_names: HashMap::new(),
            current_function: None,
            language_version,
        }
    }

    pub fn seed_types(&mut self, types: &[(String, IrType)]) {
        self.context.register_types(types);
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
            DeclNode::Type { name, generics, ty } => {
                if !generics.is_empty() {
                    self.generic_type_defs
                        .insert(name.clone(), (generics.clone(), ty.clone()));
                }
                let ir_ty = self.ast_type_to_ir_type(ty);
                self.context.register_type(name.clone(), ir_ty);
                self.type_mapping.insert(name.clone(), format!("{ty:?}"));
            }
            DeclNode::Struct {
                name,
                generics,
                fields,
            } => {
                let ty = Type::Struct(fields.clone());
                if !generics.is_empty() {
                    self.generic_type_defs
                        .insert(name.clone(), (generics.clone(), ty.clone()));
                }
                let ir_ty = self.ast_type_to_ir_type(&ty);
                self.context.register_type(name.clone(), ir_ty);
                self.type_mapping.insert(name.clone(), format!("{ty:?}"));
            }
            DeclNode::Enum {
                name,
                generics,
                variants,
            } => {
                // Register the enum name as an aggregate so `s: EnumName` resolves;
                // the tag is enough for type identity (payload layout is the lowerer's).
                if generics.is_empty() {
                    let ir_ty = IrType::Aggregate(vec![(
                        "tag".to_owned(),
                        IrType::Integer(crate::ir::IntWidth::I64),
                    )]);
                    self.context.register_type(name.clone(), ir_ty);
                    self.type_mapping.insert(name.clone(), name.clone());

                    let mut variant_names = Vec::with_capacity(variants.len());
                    for variant in variants {
                        variant_names.push(variant.name.clone());
                        let payloads = variant
                            .payload
                            .iter()
                            .map(|ty| self.ast_type_to_ir_type(ty))
                            .collect();
                        self.enum_variants.insert(
                            variant.name.clone(),
                            EnumVariantSig {
                                enum_name: name.clone(),
                                payloads,
                            },
                        );
                    }
                    self.enum_variant_names.insert(name.clone(), variant_names);
                }
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
                name,
                params,
                return_type,
                ..
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
                self.function_parameters.insert(
                    name.clone(),
                    params
                        .iter()
                        .map(|param| self.ast_type_to_ir_type(&param.ty))
                        .collect(),
                );
            }
            _ => {}
        }
    }

    fn check_declaration(&mut self, decl: &Declaration) -> Result<(), ()> {
        match &decl.decl {
            DeclNode::Import { .. } => Ok(()),
            DeclNode::Function {
                name, params, body, ..
            } => {
                self.current_function = Some(name.clone());
                self.symbols.enter_scope();

                // Register parameters
                for param in params {
                    let ir_ty = self.ast_type_to_ir_type(&param.ty);
                    let _ty_name = self.context.get_type_name(&ir_ty);
                    self.symbols
                        .insert(param.name.clone(), ir_ty, crate::ir::IrValue::Null);
                }

                // Check function body
                if let Some(body_block) = body {
                    self.check_block(body_block)?;
                }

                self.symbols.exit_scope();
                self.current_function = None;
                Ok(())
            }
            DeclNode::Type { .. }
            | DeclNode::Struct { .. }
            | DeclNode::Const { .. }
            | DeclNode::Enum { .. } => Ok(()),
            DeclNode::Variable { name, ty, .. } => {
                let ir_ty = self.ast_type_to_ir_type(ty);
                self.symbols
                    .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
                Ok(())
            }
            DeclNode::InferredVariable { name, init } => self.check_inferred_binding(name, init),
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

                if self.language_version == LanguageVersion::V2 {
                    if let Expression::Primary(PrimaryExpr::StructLiteral(fields)) = expr {
                        let expected = self
                            .current_function
                            .as_ref()
                            .and_then(|name| self.function_signatures.get(name))
                            .map(|name| self.resolve_type_string(name));
                        if let Some(expected) = expected {
                            self.check_contextual_struct_literal(fields, &expected)?;
                            return Ok(());
                        }
                    }
                }
                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::Return(None) => Ok(()),
            Statement::VariableDecl { name, ty, init } => {
                if self.symbols.contains_in_current_scope(name) {
                    self.error(format!("duplicate binding `{name}` in the same scope"));
                    return Err(());
                }
                let ir_ty = self.ast_type_to_ir_type(ty);

                if let Some(init_expr) = init {
                    if self.language_version == LanguageVersion::V2 {
                        if let Expression::Primary(PrimaryExpr::StructLiteral(fields)) = init_expr {
                            self.check_contextual_struct_literal(fields, &ir_ty)?;
                            self.symbols
                                .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
                            return Ok(());
                        }
                        if let Expression::Primary(PrimaryExpr::ArrayLiteral(elements)) = init_expr
                        {
                            if let IrType::Array { element, len } =
                                self.resolve_named_ir_type(&ir_ty)
                            {
                                self.check_contextual_array_literal(elements, &element, len)?;
                                self.symbols
                                    .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
                                return Ok(());
                            }
                        }
                    }
                    let init_ty = self.infer_expression_type(init_expr)?;
                    let resolved_decl_ty =
                        self.resolve_type_string(&self.context.get_type_name(&ir_ty));
                    let resolved_init_ty = self.resolve_type_string(&init_ty);

                    log::debug!(
                        "Variable '{}': declared as {:?}, inferred init as {}",
                        name,
                        ir_ty,
                        init_ty
                    );
                    log::debug!(
                        "  resolved_decl_ty={:?}, resolved_init_ty={:?}",
                        resolved_decl_ty,
                        resolved_init_ty
                    );

                    // A constant integer expression adopts the declared integer
                    // width; lowering folds it to the target type. Float literals
                    // are width-flexible the same way: a bare float literal adopts
                    // the declared f32/f64 (lowering materializes the right bits).
                    let is_literal_widening_allowed = (Self::is_integer_type(&resolved_decl_ty)
                        && Self::is_integer_type(&resolved_init_ty)
                        && Self::is_const_int_expr(init_expr))
                        || (Self::is_float_type(&resolved_decl_ty)
                            && Self::is_float_type(&resolved_init_ty)
                            && Self::is_float_literal_expr(init_expr));

                    // A fixed array coerces to a slice of the same element type.
                    let is_array_to_slice = self.language_version == LanguageVersion::V2
                        && Self::is_array_to_slice_coercion(
                            &self.context.get_type_name(&ir_ty),
                            &init_ty,
                        );

                    if resolved_decl_ty != resolved_init_ty
                        && !is_literal_widening_allowed
                        && !is_array_to_slice
                    {
                        self.error(format!(
                            "Type mismatch in variable initialization '{}': declared as {}, but expression yields {} (inferred_str: {}, resolved_decl: {:?}, resolved_init: {:?}, is_widening_allowed: {})",
                            name,
                            self.context.get_type_name(&ir_ty),
                            init_ty,
                            init_ty,
                            resolved_decl_ty,
                            resolved_init_ty,
                            is_literal_widening_allowed
                        ));
                        return Err(());
                    }
                }

                self.symbols
                    .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
                Ok(())
            }
            Statement::InferredVariableDecl { name, init } => {
                self.check_inferred_binding(name, init)
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
            Statement::For { var, iter, body } => {
                // The loop variable's type depends on the iterator kind.
                let var_ty = match iter {
                    crate::ast::ForIter::Range {
                        start,
                        end,
                        inclusive: _,
                    } => {
                        let start_ty = self.infer_expression_type(start)?;
                        let end_ty = self.infer_expression_type(end)?;
                        if !Self::is_integer_type(&self.resolve_type_string(&start_ty))
                            || !Self::is_integer_type(&self.resolve_type_string(&end_ty))
                        {
                            self.error(format!(
                                "for-range endpoints must be integers, found `{start_ty}..{end_ty}`"
                            ));
                            return Err(());
                        }
                        // The loop variable takes the start endpoint's type.
                        self.parse_type_string(&start_ty)
                    }
                    crate::ast::ForIter::Each(seq) => {
                        let seq_ty = self.infer_expression_type(seq)?;
                        let elem_ty = self.infer_index_element_type(&seq_ty)?;
                        self.parse_type_string(&elem_ty)
                    }
                };

                self.symbols.enter_scope();
                self.symbols
                    .insert(var.clone(), var_ty, crate::ir::IrValue::Null);
                self.check_block(body)?;
                self.symbols.exit_scope();
                Ok(())
            }
            Statement::Defer(expr) => {
                let _ = self.infer_expression_type(expr)?;
                Ok(())
            }
            Statement::AsmBlock { lines } => {
                for line in lines {
                    let instr = line.split_whitespace().next().unwrap_or("");
                    // Allow local labels (.Lname:) but reject data directives (.section, .word, etc.)
                    if instr.starts_with('.') && !instr.ends_with(':') {
                        self.error(format!(
                            "data directives are not allowed in asm blocks: `{instr}`"
                        ));
                        return Err(());
                    }
                }
                Ok(())
            }
            Statement::Break | Statement::Continue => Ok(()),
        }
    }

    fn check_inferred_binding(&mut self, name: &str, init: &Expression) -> Result<(), ()> {
        if self.symbols.contains_in_current_scope(name) {
            self.error(format!(
                "duplicate inferred binding `{name}` in the same scope"
            ));
            return Err(());
        }
        if self.language_version == LanguageVersion::V2
            && matches!(init, Expression::Primary(PrimaryExpr::StructLiteral(_)))
        {
            self.error(format!(
                "cannot infer `{name}` from an anonymous struct literal; use `Type {{ ... }}` or add an explicit type"
            ));
            return Err(());
        }

        // Infer before registering the name: `x := x` must not resolve to the
        // binding currently being declared.
        let inferred = self.infer_expression_type(init)?;
        let ir_ty = self.resolve_type_string(&inferred);
        if matches!(ir_ty, IrType::Void) || inferred == "unknown" || inferred == "*unknown" {
            self.error(format!(
                "cannot infer a concrete type for binding `{name}` from `{inferred}`"
            ));
            return Err(());
        }

        self.symbols
            .insert(name.to_owned(), ir_ty, crate::ir::IrValue::Null);
        Ok(())
    }

    fn infer_expression_type(&mut self, expr: &Expression) -> Result<String, ()> {
        match expr {
            Expression::Primary(primary) => match primary {
                PrimaryExpr::Identifier(name) => {
                    if let Some(info) = self.symbols.lookup(name) {
                        Ok(self.context.get_type_name(&info.ty))
                    } else if let Some(sig) = self.enum_variants.get(name) {
                        // A bare unit-variant constructor has its enum's type.
                        if !sig.payloads.is_empty() {
                            let enum_name = sig.enum_name.clone();
                            self.error(format!(
                                "enum variant `{name}` needs payload value(s); call it like `{name}(...)`"
                            ));
                            return Ok(enum_name);
                        }
                        Ok(sig.enum_name.clone())
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
                    Literal::String(_) => {
                        // V2 strings are `u8[]` slices; V1 keeps the record shape.
                        if self.language_version == LanguageVersion::V2 {
                            Ok("u8[]".to_owned())
                        } else {
                            Ok("{ data: u8*, length: u64 }".to_owned())
                        }
                    }
                },
                PrimaryExpr::Grouped(expr) => self.infer_expression_type(expr),
                PrimaryExpr::New { ty, .. } => {
                    let ir_ty = self.ast_type_to_ir_type(ty);
                    let inner_name = self.context.get_type_name(&ir_ty);
                    Ok(format!("*{inner_name}"))
                }
                PrimaryExpr::AsmReg { reg } => {
                    const ALLOWED: &[&str] = &[
                        "sp", "fp", "ra", "gp", "tp", "a0", "a1", "a2", "a3", "a4", "a5", "a6",
                        "a7", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11",
                    ];
                    if !ALLOWED.contains(&reg.as_str()) {
                        self.error(format!(
                            "asm_reg: `{reg}` is not an allowed ABI register (temp registers t0-t6 and s0/fp are excluded by name)"
                        ));
                        return Err(());
                    }
                    Ok("i64".to_owned())
                }
                PrimaryExpr::FunctionCall {
                    name, arguments, ..
                } => {
                    // A call whose name is an enum variant constructs that variant.
                    if let Some(sig) = self.enum_variants.get(name).cloned() {
                        if arguments.len() != sig.payloads.len() {
                            self.error(format!(
                                "enum variant `{name}` expects {} payload value(s), got {}",
                                sig.payloads.len(),
                                arguments.len()
                            ));
                            return Err(());
                        }
                        for arg in arguments {
                            let _ = self.infer_expression_type(arg)?;
                        }
                        return Ok(sig.enum_name);
                    }

                    // Type check arguments
                    let parameter_types = self.function_parameters.get(name).cloned();
                    for (index, arg) in arguments.iter().enumerate() {
                        if self.language_version == LanguageVersion::V2 {
                            if let (
                                Some(expected),
                                Expression::Primary(PrimaryExpr::StructLiteral(fields)),
                            ) = (parameter_types.as_ref().and_then(|p| p.get(index)), arg)
                            {
                                self.check_contextual_struct_literal(fields, expected)?;
                                continue;
                            }
                        }
                        let _ = self.infer_expression_type(arg)?;
                    }

                    // Check if this is a type cast (e.g., u64(...), i32(...), etc.)
                    // Type casts are recognized by checking if the name is a known type
                    let cast_return_type = match name.as_str() {
                        "i32" => Some("i32".to_owned()),
                        "i64" => Some("i64".to_owned()),
                        "u32" => Some("u32".to_owned()),
                        "u64" => Some("u64".to_owned()),
                        "i16" => Some("i16".to_owned()),
                        "u16" => Some("u16".to_owned()),
                        "i8" => Some("i8".to_owned()),
                        "u8" => Some("u8".to_owned()),
                        "f32" => Some("f32".to_owned()),
                        "f64" => Some("f64".to_owned()),
                        "bool" => Some("i1".to_owned()),
                        _ => None,
                    };

                    if let Some(cast_type) = cast_return_type {
                        return Ok(cast_type);
                    }

                    // Look up the function's return type
                    if let Some(return_ty) = self.function_signatures.get(name) {
                        Ok(return_ty.clone())
                    } else {
                        // Unknown function, return unknown
                        log::warn!("Unknown function/type cast: {}", name);
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

                    let element_ty =
                        inferred_element_ty.expect("array must have at least one typed element");
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
                PrimaryExpr::Slice {
                    expr, start, end, ..
                } => {
                    if let Some(start) = start {
                        let _ = self.infer_expression_type(start)?;
                    }
                    if let Some(end) = end {
                        let _ = self.infer_expression_type(end)?;
                    }
                    let base_ty = self.infer_expression_type(expr)?;
                    let elem_ty = self.infer_index_element_type(&base_ty)?;
                    Ok(format!("{elem_ty}[]"))
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
                PrimaryExpr::NamedStructLiteral { name, fields } => {
                    self.infer_named_struct_literal_type(name, fields)
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
                    if self.language_version == LanguageVersion::V2 {
                        if !self.is_place_expression(inner) {
                            self.error(
                                "address-of requires a place (identifier, dereference, field, or indexed element)"
                                    .to_owned(),
                            );
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
                    }

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
            Expression::Match { scrutinee, arms } => self.check_match(scrutinee, arms),
            Expression::Try(expr) => self.check_try(expr),
        }
    }

    fn check_try(&mut self, expr: &Expression) -> Result<String, ()> {
        let operand_ty = self.infer_expression_type(expr)?;
        let family = if operand_ty.starts_with("Result__") {
            "Result"
        } else if operand_ty.starts_with("Option__") {
            "Option"
        } else {
            self.error(format!(
                "`?` requires `Result<T, E>` or `Option<T>`, found `{operand_ty}`"
            ));
            return Err(());
        };

        let return_ty = self
            .current_function
            .as_ref()
            .and_then(|name| self.function_signatures.get(name))
            .cloned()
            .unwrap_or_else(|| "void".to_owned());
        if !return_ty.starts_with(&format!("{family}__")) {
            self.error(format!(
                "`?` on `{operand_ty}` requires the enclosing function to return `{family}`"
            ));
            return Err(());
        }

        let success_prefix = if family == "Result" { "Ok" } else { "Some" };
        let success = self
            .enum_variants
            .iter()
            .find(|(name, sig)| {
                sig.enum_name == operand_ty
                    && (name.as_str() == success_prefix
                        || name.starts_with(&format!("{success_prefix}__")))
            })
            .map(|(_, sig)| sig.clone());
        let Some(success) = success else {
            self.error(format!("invalid `{family}` specialization `{operand_ty}`"));
            return Err(());
        };
        if success.payloads.len() != 1 {
            self.error(format!("invalid `{family}` success variant layout"));
            return Err(());
        }

        if family == "Result" {
            let failure_payload = |enum_name: &str| {
                self.enum_variants
                    .iter()
                    .find(|(name, sig)| {
                        sig.enum_name == enum_name
                            && (name.as_str() == "Err" || name.starts_with("Err__"))
                    })
                    .and_then(|(_, sig)| sig.payloads.first())
                    .cloned()
            };
            if failure_payload(&operand_ty) != failure_payload(&return_ty) {
                self.error(format!(
                    "`?` cannot propagate `{operand_ty}` from a function returning `{return_ty}`"
                ));
                return Err(());
            }
        }

        Ok(self.context.get_type_name(&success.payloads[0]))
    }

    // Type-check a `match`: the scrutinee must be an enum, every arm pattern must
    // be a valid variant of it (payload arity respected), and the arms must be
    // exhaustive (cover all variants or include a catch-all). Returns "void" --
    // value-producing match is deferred.
    fn check_match(&mut self, scrutinee: &Expression, arms: &[MatchArm]) -> Result<String, ()> {
        let enum_name = self.infer_expression_type(scrutinee)?;
        let Some(all_variants) = self.enum_variant_names.get(&enum_name).cloned() else {
            self.error(format!("`match` scrutinee has non-enum type `{enum_name}`"));
            return Err(());
        };

        // A `-> expr` value arm makes the match value-producing; either every arm
        // yields a value (types must unify) or none do (a "void" statement match).
        let value_arms = arms.iter().filter(|a| a.value.is_some()).count();
        if value_arms != 0 && value_arms != arms.len() {
            self.error(
                "all `match` arms must produce a value or none may; mix not allowed".to_owned(),
            );
            return Err(());
        }
        let mut result_ty: Option<String> = None;

        let mut covered: HashSet<String> = HashSet::new();
        let mut has_catch_all = false;
        for arm in arms {
            if has_catch_all {
                self.error("unreachable match arm after a catch-all pattern".to_owned());
                return Err(());
            }
            self.symbols.enter_scope();
            let arm_ok = match &arm.pattern {
                Pattern::Wildcard => {
                    has_catch_all = true;
                    Ok(())
                }
                Pattern::Binding(name) => {
                    has_catch_all = true;
                    let ir_ty = self.ast_type_to_ir_type(&Type::Named {
                        name: enum_name.clone(),
                        args: Vec::new(),
                    });
                    self.symbols
                        .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
                    Ok(())
                }
                Pattern::Variant {
                    variant, bindings, ..
                } => self.check_variant_pattern(&enum_name, variant, bindings, &mut covered),
            };
            let body_result = arm_ok.and_then(|()| self.check_block(&arm.body));
            // Infer the arm value while its payload bindings are still in scope.
            let arm_value_ty = body_result.and_then(|()| match &arm.value {
                Some(value) => self.infer_expression_type(value).map(Some),
                None => Ok(None),
            });
            self.symbols.exit_scope();
            if let Some(arm_ty) = arm_value_ty? {
                match &result_ty {
                    None => result_ty = Some(arm_ty),
                    Some(existing) if *existing != arm_ty => {
                        self.error(format!(
                            "`match` arms produce different types: `{existing}` and `{arm_ty}`"
                        ));
                        return Err(());
                    }
                    Some(_) => {}
                }
            }
        }

        if !has_catch_all {
            let missing: Vec<String> = all_variants
                .iter()
                .filter(|v| !covered.contains(*v))
                .cloned()
                .collect();
            if !missing.is_empty() {
                self.error(format!(
                    "non-exhaustive `match` on `{enum_name}`: missing {}",
                    missing.join(", ")
                ));
                return Err(());
            }
        }
        Ok(result_ty.unwrap_or_else(|| "void".to_owned()))
    }

    // Validate one `Variant(bindings)` arm and bind its payload slots as locals.
    fn check_variant_pattern(
        &mut self,
        enum_name: &str,
        variant: &str,
        bindings: &[String],
        covered: &mut HashSet<String>,
    ) -> Result<(), ()> {
        let Some(sig) = self.enum_variants.get(variant).cloned() else {
            self.error(format!("unknown enum variant `{variant}` in match arm"));
            return Err(());
        };
        if sig.enum_name != enum_name {
            self.error(format!(
                "variant `{variant}` is not a variant of `{enum_name}`"
            ));
            return Err(());
        }
        if bindings.len() != sig.payloads.len() {
            self.error(format!(
                "variant `{variant}` binds {} value(s) but has {}",
                bindings.len(),
                sig.payloads.len()
            ));
            return Err(());
        }
        if !covered.insert(variant.to_owned()) {
            self.error(format!("duplicate match arm for variant `{variant}`"));
            return Err(());
        }
        for (binding, ty) in bindings.iter().zip(&sig.payloads) {
            if binding != "_" {
                self.symbols
                    .insert(binding.clone(), ty.clone(), crate::ir::IrValue::Null);
            }
        }
        Ok(())
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
            Expression::Primary(PrimaryExpr::Slice {
                expr, start, end, ..
            }) => {
                self.contains_dereference(expr)
                    || start
                        .as_deref()
                        .is_some_and(|s| self.contains_dereference(s))
                    || end.as_deref().is_some_and(|e| self.contains_dereference(e))
            }
            Expression::Primary(PrimaryExpr::FunctionCall { arguments, .. }) => {
                arguments.iter().any(|arg| self.contains_dereference(arg))
            }
            Expression::Primary(PrimaryExpr::StructLiteral(fields)) => fields
                .iter()
                .any(|field| self.contains_dereference(&field.expr)),
            Expression::Primary(PrimaryExpr::NamedStructLiteral { fields, .. }) => fields
                .iter()
                .any(|field| self.contains_dereference(&field.expr)),
            _ => false,
        }
    }

    fn is_place_expression(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Primary(PrimaryExpr::Identifier(_))
            | Expression::Unary {
                op: UnaryOp::Dereference,
                ..
            }
            | Expression::Primary(PrimaryExpr::FieldAccess { .. })
            | Expression::Primary(PrimaryExpr::ArrayIndex { .. }) => true,
            Expression::Primary(PrimaryExpr::Grouped(inner)) => self.is_place_expression(inner),
            _ => false,
        }
    }

    fn infer_named_struct_literal_type(
        &mut self,
        name: &str,
        fields: &[FieldInit],
    ) -> Result<String, ()> {
        let declared = self.resolve_type_string(name);
        let IrType::Aggregate(declared_fields) = declared else {
            self.error(format!(
                "named struct literal references non-struct type `{name}`"
            ));
            return Err(());
        };

        let mut seen = HashSet::new();
        for field in fields {
            if !seen.insert(field.name.clone()) {
                self.error(format!(
                    "duplicate field `{}` in `{name}` literal",
                    field.name
                ));
                return Err(());
            }
            let Some((_, expected_ty)) = declared_fields
                .iter()
                .find(|(field_name, _)| field_name == &field.name)
            else {
                self.error(format!(
                    "unknown field `{}` in `{name}` literal",
                    field.name
                ));
                return Err(());
            };
            let actual_name = self.infer_expression_type(&field.expr)?;
            let actual_ty = self.resolve_type_string(&actual_name);
            let expected_ty = self.resolve_named_ir_type(expected_ty);
            let integer_literal_compatible = Self::is_integer_type(&expected_ty)
                && Self::is_integer_type(&actual_ty)
                && Self::is_const_int_expr(&field.expr);
            let float_literal_compatible = Self::is_float_type(&expected_ty)
                && Self::is_float_type(&actual_ty)
                && Self::is_float_literal_expr(&field.expr);
            if actual_ty != expected_ty && !integer_literal_compatible && !float_literal_compatible
            {
                self.error(format!(
                    "field `{}` in `{name}` literal expects `{}`, found `{actual_name}`",
                    field.name,
                    self.context.get_type_name(&expected_ty)
                ));
                return Err(());
            }
        }

        if let Some((missing, _)) = declared_fields
            .iter()
            .find(|(field_name, _)| !seen.contains(field_name))
        {
            self.error(format!("missing field `{missing}` in `{name}` literal"));
            return Err(());
        }
        Ok(name.to_owned())
    }

    fn check_contextual_struct_literal(
        &mut self,
        fields: &[FieldInit],
        expected: &IrType,
    ) -> Result<(), ()> {
        let resolved = self.resolve_named_ir_type(expected);
        let IrType::Aggregate(declared_fields) = resolved else {
            self.error(format!(
                "contextual struct literal requires a struct target, found `{}`",
                self.context.get_type_name(expected)
            ));
            return Err(());
        };

        let mut seen = HashSet::new();
        for field in fields {
            if !seen.insert(field.name.clone()) {
                self.error(format!(
                    "duplicate field `{}` in struct literal",
                    field.name
                ));
                return Err(());
            }
            let Some((_, expected_ty)) = declared_fields
                .iter()
                .find(|(field_name, _)| field_name == &field.name)
            else {
                self.error(format!(
                    "unknown field `{}` in contextual struct literal",
                    field.name
                ));
                return Err(());
            };
            let actual_name = self.infer_expression_type(&field.expr)?;
            let actual_ty = self.resolve_type_string(&actual_name);
            let expected_ty = self.resolve_named_ir_type(expected_ty);
            let compatible_literal = (Self::is_integer_type(&expected_ty)
                && Self::is_integer_type(&actual_ty)
                && Self::is_const_int_expr(&field.expr))
                || (Self::is_float_type(&expected_ty)
                    && Self::is_float_type(&actual_ty)
                    && Self::is_float_literal_expr(&field.expr));
            if actual_ty != expected_ty && !compatible_literal {
                self.error(format!(
                    "field `{}` expects `{}`, found `{actual_name}`",
                    field.name,
                    self.context.get_type_name(&expected_ty)
                ));
                return Err(());
            }
        }
        Ok(())
    }

    // Check an array literal against a declared array type (V2). Bare struct
    // literal elements are checked contextually; other elements must match the
    // element type (with the usual scalar-literal width flexibility).
    fn check_contextual_array_literal(
        &mut self,
        elements: &[Expression],
        element_ty: &IrType,
        len: usize,
    ) -> Result<(), ()> {
        // `arr: T[N] = []` is the explicit zero-fill form.
        if elements.is_empty() {
            return Ok(());
        }
        if elements.len() != len {
            self.error(format!(
                "array literal has {} elements, but the declared type expects {len}",
                elements.len()
            ));
            return Err(());
        }
        let expected_ty = self.resolve_named_ir_type(element_ty);
        for element in elements {
            if let Expression::Primary(PrimaryExpr::StructLiteral(fields)) = element {
                self.check_contextual_struct_literal(fields, element_ty)?;
                continue;
            }
            let actual_name = self.infer_expression_type(element)?;
            let actual_ty = self.resolve_type_string(&actual_name);
            let compatible_literal = (Self::is_integer_type(&expected_ty)
                && Self::is_integer_type(&actual_ty)
                && Self::is_const_int_expr(element))
                || (Self::is_float_type(&expected_ty)
                    && Self::is_float_type(&actual_ty)
                    && Self::is_float_literal_expr(element));
            if actual_ty != expected_ty && !compatible_literal {
                self.error(format!(
                    "array element expects `{}`, found `{actual_name}`",
                    self.context.get_type_name(&expected_ty)
                ));
                return Err(());
            }
        }
        Ok(())
    }

    fn ast_type_to_ir_type(&self, ty: &Type) -> IrType {
        self.ast_type_to_ir_type_inner(ty, &mut HashSet::new())
    }

    fn ast_type_to_ir_type_inner(&self, ty: &Type, active: &mut HashSet<String>) -> IrType {
        match ty {
            Type::Primitive(name) => self.primitive_to_ir(name),
            Type::Pointer(inner) => {
                IrType::Pointer(Box::new(self.ast_type_to_ir_type_inner(inner, active)))
            }
            Type::Array(len, inner) => IrType::Array {
                len: *len,
                element: Box::new(self.ast_type_to_ir_type_inner(inner, active)),
            },
            Type::Slice(inner) => {
                IrType::Slice(Box::new(self.ast_type_to_ir_type_inner(inner, active)))
            }
            Type::Struct(fields) => {
                let field_types: Vec<(String, IrType)> = fields
                    .iter()
                    .map(|f| {
                        (
                            f.name.clone(),
                            self.ast_type_to_ir_type_inner(&f.ty, active),
                        )
                    })
                    .collect();
                IrType::Aggregate(field_types)
            }
            Type::Named { name, args } => {
                if args.is_empty() {
                    IrType::Named(name.clone())
                } else if let Some((params, definition)) = self.generic_type_defs.get(name) {
                    if params.len() != args.len() {
                        return IrType::Named(name.clone());
                    }
                    let specialized_name = format!(
                        "{name}<{}>",
                        args.iter()
                            .map(|arg| {
                                self.context
                                    .get_type_name(&self.ast_type_to_ir_type_inner(arg, active))
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    if !active.insert(specialized_name.clone()) {
                        return IrType::Named(specialized_name);
                    }
                    let substitutions = params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    let resolved = self.ast_type_to_ir_type_inner(
                        &substitute_type(definition, &substitutions),
                        active,
                    );
                    active.remove(&specialized_name);
                    resolved
                } else {
                    let args = args
                        .iter()
                        .map(|arg| {
                            self.context
                                .get_type_name(&self.ast_type_to_ir_type_inner(arg, active))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    IrType::Named(format!("{name}<{args}>"))
                }
            }
        }
    }

    fn primitive_to_ir(&self, name: &str) -> IrType {
        match name {
            "i8" => IrType::Integer(crate::ir::IntWidth::I8),
            "i16" => IrType::Integer(crate::ir::IntWidth::I16),
            "i32" => IrType::Integer(crate::ir::IntWidth::I32),
            "i64" => IrType::Integer(crate::ir::IntWidth::I64),
            "u8" => IrType::Integer(crate::ir::IntWidth::I8),
            "u16" => IrType::Integer(crate::ir::IntWidth::I16),
            "u32" => IrType::Integer(crate::ir::IntWidth::I32),
            "u64" => IrType::Integer(crate::ir::IntWidth::I64),
            "f32" => IrType::Float(crate::ir::FloatWidth::F32),
            "f64" => IrType::Float(crate::ir::FloatWidth::F64),
            "bool" => IrType::Integer(crate::ir::IntWidth::I1),
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

        // A slice `T[]` exposes `ptr` (*T) and `len` (i64).
        if let Some(elem) = base_type.strip_suffix("[]") {
            return match field {
                "len" => Ok("i64".to_owned()),
                "ptr" => Ok(format!("*{elem}")),
                _ => {
                    self.diagnostics
                        .error(format!("unknown field `{field}` for slice `{base_type}`"));
                    Err(())
                }
            };
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
            if through_pointer && self.language_version == LanguageVersion::V1 {
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
                return Ok(if self.language_version == LanguageVersion::V2 {
                    element.to_owned()
                } else {
                    format!("*{element}")
                });
            }
            // Pointer to non-array: *T -> *T (indexing through pointer returns pointer to element)
            return Ok(if self.language_version == LanguageVersion::V2 {
                inner.to_owned()
            } else {
                format!("*{inner}")
            });
        }

        // Handle direct array: T[N] -> *T (stack arrays follow the same pointer-element rule)
        if let Some((element, _rest)) = base_type.split_once('[') {
            return Ok(if self.language_version == LanguageVersion::V2 {
                element.to_owned()
            } else {
                format!("*{element}")
            });
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
                Ok(if self.language_version == LanguageVersion::V2 {
                    inner_name
                } else {
                    format!("*{inner_name}")
                })
            }
            IrType::Array { element, .. } => {
                let element_name = self.context.get_type_name(element);
                Ok(if self.language_version == LanguageVersion::V2 {
                    element_name
                } else {
                    format!("*{element_name}")
                })
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
                    let info = self
                        .symbols
                        .lookup(name)
                        .expect("symbol exists: checked above");
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
                    self.symbols
                        .insert(name.clone(), ir_ty, crate::ir::IrValue::Null);
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
                    if self.language_version == LanguageVersion::V2 {
                        self.error(format!(
                            "assignment target `{name}` is not declared; use `{name} := ...` to create a binding"
                        ));
                    } else {
                        self.diagnostics
                            .error(format!("Undefined identifier: {name}"));
                    }
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

        if let Some(elem) = trimmed.strip_suffix("[]") {
            if !elem.is_empty() {
                return IrType::Slice(Box::new(self.parse_type_string(elem)));
            }
        }

        if let Some(inner) = trimmed.strip_suffix('*') {
            if !inner.is_empty() {
                return IrType::Pointer(Box::new(self.parse_type_string(inner)));
            }
        }

        match trimmed {
            "i1" => IrType::Integer(crate::ir::IntWidth::I1),
            "i8" => IrType::Integer(crate::ir::IntWidth::I8),
            "i16" => IrType::Integer(crate::ir::IntWidth::I16),
            "i32" => IrType::Integer(crate::ir::IntWidth::I32),
            "i64" => IrType::Integer(crate::ir::IntWidth::I64),
            "u8" => IrType::Integer(crate::ir::IntWidth::I8),
            "u16" => IrType::Integer(crate::ir::IntWidth::I16),
            "u32" => IrType::Integer(crate::ir::IntWidth::I32),
            "u64" => IrType::Integer(crate::ir::IntWidth::I64),
            "f32" => IrType::Float(crate::ir::FloatWidth::F32),
            "f64" => IrType::Float(crate::ir::FloatWidth::F64),
            "bool" => IrType::Integer(crate::ir::IntWidth::I1),
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
                .or_else(|| {
                    // Generic instantiation `Base<...>`: the semantic context
                    // registers only the base definition, so fall back to it.
                    // Type arguments are not yet substituted here (V1-loose
                    // generics); monomorphization happens during lowering.
                    name.split_once('<')
                        .and_then(|(base, _)| self.context.resolve(base).cloned())
                })
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

        // Allow pointer to integer casts
        let source_is_pointer = matches!(source, IrType::Pointer(_));
        let target_is_pointer = matches!(target, IrType::Pointer(_));
        if (source_is_pointer && target_is_numeric) || (source_is_numeric && target_is_pointer) {
            return true;
        }

        // Allow same-type casts (identity)
        if source == target {
            return true;
        }

        // No other cast forms are permitted.
        false
    }

    fn is_integer_type(ty: &IrType) -> bool {
        matches!(ty, IrType::Integer(_))
    }

    fn is_float_type(ty: &IrType) -> bool {
        matches!(ty, IrType::Float(_))
    }

    // True when `decl` is a slice `T[]` and `init` is an array `T[N]` of the same
    // element type, e.g. decl "i32[]" against init "i32[3]".
    fn is_array_to_slice_coercion(decl: &str, init: &str) -> bool {
        let Some(elem) = decl.strip_suffix("[]") else {
            return false;
        };
        let Some(open) = init.rfind('[') else {
            return false;
        };
        let (init_elem, count) = init.split_at(open);
        init_elem == elem
            && count.starts_with('[')
            && count.ends_with(']')
            && count[1..count.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit())
            && count.len() > 2
    }

    // A bare float literal (optionally grouped or negated). Such a literal is
    // width-flexible and adopts the declared f32/f64 type.
    fn is_float_literal_expr(expr: &Expression) -> bool {
        match expr {
            Expression::Primary(PrimaryExpr::Literal(Literal::Float(_))) => true,
            Expression::Primary(PrimaryExpr::Grouped(inner)) => Self::is_float_literal_expr(inner),
            Expression::Unary {
                op: UnaryOp::Negate,
                expr,
            } => Self::is_float_literal_expr(expr),
            _ => false,
        }
    }

    // An integer literal, a negation, or arithmetic/bitwise ops over such (grouped).
    fn is_const_int_expr(expr: &Expression) -> bool {
        match expr {
            Expression::Primary(PrimaryExpr::Literal(
                Literal::Integer(_) | Literal::HexInteger(_),
            )) => true,
            Expression::Primary(PrimaryExpr::Grouped(inner)) => Self::is_const_int_expr(inner),
            Expression::Unary {
                op: UnaryOp::Negate,
                expr,
            } => Self::is_const_int_expr(expr),
            Expression::Binary { op, left, right } => {
                matches!(
                    op,
                    BinaryOp::Add
                        | BinaryOp::Sub
                        | BinaryOp::Mul
                        | BinaryOp::Div
                        | BinaryOp::Mod
                        | BinaryOp::Shl
                        | BinaryOp::Shr
                        | BinaryOp::BitwiseAnd
                        | BinaryOp::BitwiseOr
                        | BinaryOp::BitwiseXor
                ) && Self::is_const_int_expr(left)
                    && Self::is_const_int_expr(right)
            }
            _ => false,
        }
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
