// AST-only lowering of `dyn* I` (spec 6.6): types become the synthesized wide
// reference `I__dyn`; `expr as dyn* I` packs the erased data pointer with thunks.

use crate::ast::{
    Block, DeclNode, Declaration, Expression, FieldDecl, FieldInit, ForIter, InterfaceMethod,
    Parameter, PrimaryExpr, Program, ReturnType, Statement, Type,
};
use crate::monomorphize::{
    GenericSignature, Substitutions, check_type_implements, infer_expression_type, substitute_type,
};
use std::collections::{BTreeSet, HashMap};

pub(crate) fn lower_dyn_interfaces(
    program: &mut Program,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
) -> Result<(), String> {
    // Blanket-rewrite the parser's `dyn I` marker types to `I__dyn`. A marker
    // naming an unknown interface survives and fails in semantic analysis.
    let marker_substitutions = interfaces
        .keys()
        .map(|name| {
            (
                format!("dyn* {name}"),
                Type::Named {
                    name: dyn_struct_name(name),
                    args: Vec::new(),
                },
            )
        })
        .collect::<Substitutions>();
    for declaration in &mut program.declarations {
        substitute_declaration_types(declaration, &marker_substitutions);
    }
    for statement in &mut program.statements {
        crate::monomorphize::substitute_statement_types(statement, &marker_substitutions);
    }

    let mut context = DynContext {
        interfaces,
        concrete_signatures: collect_concrete_signatures(program),
        function_returns: collect_function_returns(program),
        used_interfaces: BTreeSet::new(),
        used_pairs: BTreeSet::new(),
    };

    // Rewrite `expr as dyn I` conversion sites, tracking every (interface, type)
    // pairing so the matching thunks can be synthesized afterwards.
    let globals = collect_global_types(program);
    for declaration in &mut program.declarations {
        if let DeclNode::Function {
            params,
            body: Some(body),
            ..
        } = &mut declaration.decl
        {
            let mut environment = globals.clone();
            environment.extend(
                params
                    .iter()
                    .map(|param| (param.name.clone(), param.ty.clone())),
            );
            rewrite_block(body, &mut environment, &mut context)?;
        }
    }
    let mut environment = globals;
    for statement in &mut program.statements {
        rewrite_statement(statement, &mut environment, &mut context)?;
    }

    // A `dyn I` type used anywhere (parameter, field, local) needs the struct
    // and dispatch wrappers even when this unit performs no conversion.
    collect_mentioned_dyn_structs(program, interfaces, &mut context.used_interfaces);

    let mut synthesized = Vec::new();
    for interface_name in &context.used_interfaces {
        let methods = &interfaces[interface_name];
        validate_dyn_compatible(interface_name, methods)?;
        synthesized.push(dyn_struct_declaration(interface_name, methods));
        for method in methods {
            synthesized.push(dispatch_wrapper(interface_name, method));
        }
    }
    for (interface_name, type_name) in &context.used_pairs {
        for method in &interfaces[interface_name] {
            synthesized.push(thunk_declaration(interface_name, type_name, method));
        }
    }
    program.declarations.extend(synthesized);
    Ok(())
}

struct DynContext<'a> {
    interfaces: &'a HashMap<String, Vec<InterfaceMethod>>,
    concrete_signatures: HashMap<String, (Vec<Type>, Option<Type>)>,
    function_returns: HashMap<String, Option<Type>>,
    // BTreeSets keep synthesized declaration order deterministic.
    used_interfaces: BTreeSet<String>,
    used_pairs: BTreeSet<(String, String)>,
}

// --- Names and shared type builders ---

fn dyn_struct_name(interface: &str) -> String {
    format!("{interface}__dyn")
}

fn data_field() -> String {
    "__data".to_owned()
}

fn fn_field(method: &str) -> String {
    format!("__fn_{method}")
}

fn thunk_name(interface: &str, type_name: &str, method: &str) -> String {
    format!("{interface}__dyn__{type_name}__{method}")
}

fn u8_pointer() -> Type {
    Type::Pointer(Box::new(Type::Primitive("u8".to_owned())))
}

fn named(name: &str) -> Type {
    Type::Named {
        name: name.to_owned(),
        args: Vec::new(),
    }
}

fn ident(name: &str) -> Expression {
    Expression::Primary(PrimaryExpr::Identifier(name.to_owned()))
}

fn field_access(expr: Expression, field: &str) -> Expression {
    Expression::Primary(PrimaryExpr::FieldAccess {
        expr: Box::new(expr),
        field: field.to_owned(),
    })
}

fn return_block(expr: Expression) -> Block {
    Block {
        statements: vec![Statement::Return(Some(expr))],
        stmt_lines: Vec::new(),
    }
}

// `Self` erases to `u8` so the receiver `Self*` becomes the wide pointer's raw
// `u8*` data slot; the thunk casts it back to the concrete type.
fn erase_self(ty: &Type) -> Type {
    let erasure = Substitutions::from([("Self".to_owned(), Type::Primitive("u8".to_owned()))]);
    substitute_type(ty, &erasure)
}

fn erased_return(method: &InterfaceMethod) -> Option<ReturnType> {
    method
        .return_type
        .as_ref()
        .map(|ReturnType::Single(ty)| ReturnType::Single(erase_self(ty)))
}

// The non-receiver parameters of an interface method, types erased.
fn erased_tail_params(method: &InterfaceMethod) -> Vec<Parameter> {
    method
        .params
        .iter()
        .skip(1)
        .map(|param| Parameter {
            name: param.name.clone(),
            ty: erase_self(&param.ty),
        })
        .collect()
}

fn type_mentions_self(ty: &Type) -> bool {
    match ty {
        Type::Named { name, args } => name == "Self" || args.iter().any(type_mentions_self),
        Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
            type_mentions_self(inner)
        }
        Type::Function {
            params,
            return_type,
        } => {
            params.iter().any(type_mentions_self)
                || return_type
                    .as_ref()
                    .is_some_and(|ty| type_mentions_self(ty))
        }
        Type::Struct(fields) => fields.iter().any(|field| type_mentions_self(&field.ty)),
        Type::Primitive(_) => false,
    }
}

// v1 erasure only handles the receiver; a `Self` anywhere else has no single
// erased representation, so reject it for `dyn` use.
fn validate_dyn_compatible(interface: &str, methods: &[InterfaceMethod]) -> Result<(), String> {
    for method in methods {
        let tail_mentions_self = method
            .params
            .iter()
            .skip(1)
            .any(|param| type_mentions_self(&param.ty));
        let return_mentions_self = method
            .return_type
            .as_ref()
            .is_some_and(|ReturnType::Single(ty)| type_mentions_self(ty));
        if tail_mentions_self || return_mentions_self {
            return Err(format!(
                "interface `{interface}` cannot be used as `dyn {interface}`: method `{}` \
                 mentions `Self` outside the receiver",
                method.name
            ));
        }
    }
    Ok(())
}

// --- Synthesized declarations ---

// struct I__dyn { __data: u8*, __fn_m: fn(u8*, ...) -> R, ... }
fn dyn_struct_declaration(interface: &str, methods: &[InterfaceMethod]) -> Declaration {
    let mut fields = vec![FieldDecl {
        name: data_field(),
        ty: u8_pointer(),
        init: None,
    }];
    for method in methods {
        let mut params = vec![u8_pointer()];
        params.extend(erased_tail_params(method).into_iter().map(|p| p.ty));
        fields.push(FieldDecl {
            name: fn_field(&method.name),
            ty: Type::Function {
                params,
                return_type: method
                    .return_type
                    .as_ref()
                    .map(|ReturnType::Single(ty)| Box::new(erase_self(ty))),
            },
            init: None,
        });
    }
    Declaration {
        decl: DeclNode::Struct {
            name: dyn_struct_name(interface),
            generics: Vec::new(),
            bounds: Vec::new(),
            fields,
        },
        exported: false,
    }
}

// I__dyn_m: (self: I__dyn*, ...) -> R { return self.__fn_m(self.__data, ...) }
// Named by the `Type_method` convention so `d.m(args)` resolves like any method.
fn dispatch_wrapper(interface: &str, method: &InterfaceMethod) -> Declaration {
    let struct_name = dyn_struct_name(interface);
    let tail = erased_tail_params(method);
    let mut params = vec![Parameter {
        name: "self".to_owned(),
        ty: Type::Pointer(Box::new(named(&struct_name))),
    }];
    params.extend(tail.iter().cloned());

    let mut arguments = vec![field_access(ident("self"), &data_field())];
    arguments.extend(tail.iter().map(|param| ident(&param.name)));
    let call = Expression::Primary(PrimaryExpr::CallExpr {
        callee: Box::new(field_access(ident("self"), &fn_field(&method.name))),
        arguments,
    });

    Declaration {
        decl: DeclNode::Function {
            name: format!("{struct_name}_{}", method.name),
            generics: Vec::new(),
            bounds: Vec::new(),
            params,
            return_type: erased_return(method),
            body: Some(return_block(call)),
            is_extern: false,
            is_import_interface: false,
        },
        exported: false,
    }
}

// I__dyn__T__m: (__obj: u8*, ...) -> R { return T_m(__obj as T*, ...) }
fn thunk_declaration(interface: &str, type_name: &str, method: &InterfaceMethod) -> Declaration {
    let tail = erased_tail_params(method);
    let mut params = vec![Parameter {
        name: "__obj".to_owned(),
        ty: u8_pointer(),
    }];
    params.extend(tail.iter().cloned());

    let mut arguments = vec![Expression::Cast {
        target_ty: Type::Pointer(Box::new(named(type_name))),
        expr: Box::new(ident("__obj")),
    }];
    arguments.extend(tail.iter().map(|param| ident(&param.name)));
    let call = Expression::Primary(PrimaryExpr::FunctionCall {
        name: format!("{type_name}_{}", method.name),
        type_arguments: Vec::new(),
        arguments,
    });

    Declaration {
        decl: DeclNode::Function {
            name: thunk_name(interface, type_name, &method.name),
            generics: Vec::new(),
            bounds: Vec::new(),
            params,
            return_type: erased_return(method),
            body: Some(return_block(call)),
            is_extern: false,
            is_import_interface: false,
        },
        exported: false,
    }
}

// --- Program scanning helpers ---

fn collect_concrete_signatures(program: &Program) -> HashMap<String, (Vec<Type>, Option<Type>)> {
    program
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Function {
                name,
                generics,
                params,
                return_type,
                ..
            } if generics.is_empty() => {
                let params = params.iter().map(|p| p.ty.clone()).collect::<Vec<_>>();
                let ret = return_type
                    .as_ref()
                    .map(|ReturnType::Single(ty)| ty.clone());
                Some((name.clone(), (params, ret)))
            }
            _ => None,
        })
        .collect()
}

fn collect_function_returns(program: &Program) -> HashMap<String, Option<Type>> {
    program
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Function {
                name, return_type, ..
            } => Some((
                name.clone(),
                return_type
                    .as_ref()
                    .map(|ReturnType::Single(ty)| ty.clone()),
            )),
            _ => None,
        })
        .collect()
}

fn collect_global_types(program: &Program) -> HashMap<String, Type> {
    program
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Variable { name, ty, .. } => Some((name.clone(), ty.clone())),
            _ => None,
        })
        .collect()
}

// Every interface whose `I__dyn` struct name appears in a declared type.
fn collect_mentioned_dyn_structs(
    program: &Program,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
    used: &mut BTreeSet<String>,
) {
    let mut mark = |ty: &Type| mark_dyn_names(ty, interfaces, used);
    for declaration in &program.declarations {
        match &declaration.decl {
            DeclNode::Function {
                params,
                return_type,
                ..
            } => {
                for param in params {
                    mark(&param.ty);
                }
                if let Some(ReturnType::Single(ty)) = return_type {
                    mark(ty);
                }
            }
            DeclNode::Variable { ty, .. } => mark(ty),
            DeclNode::Type { ty, .. } => mark(ty),
            DeclNode::Struct { fields, .. } => {
                for field in fields {
                    mark(&field.ty);
                }
            }
            DeclNode::Enum { variants, .. } => {
                for variant in variants {
                    for payload in &variant.payload {
                        mark(payload);
                    }
                }
            }
            _ => {}
        }
    }
}

fn mark_dyn_names(
    ty: &Type,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
    used: &mut BTreeSet<String>,
) {
    match ty {
        Type::Named { name, args } => {
            if let Some(interface) = name.strip_suffix("__dyn")
                && interfaces.contains_key(interface)
            {
                used.insert(interface.to_owned());
            }
            for arg in args {
                mark_dyn_names(arg, interfaces, used);
            }
        }
        Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
            mark_dyn_names(inner, interfaces, used);
        }
        Type::Function {
            params,
            return_type,
        } => {
            for param in params {
                mark_dyn_names(param, interfaces, used);
            }
            if let Some(ty) = return_type {
                mark_dyn_names(ty, interfaces, used);
            }
        }
        Type::Struct(fields) => {
            for field in fields {
                mark_dyn_names(&field.ty, interfaces, used);
            }
        }
        Type::Primitive(_) => {}
    }
}

fn substitute_declaration_types(declaration: &mut Declaration, substitutions: &Substitutions) {
    match &mut declaration.decl {
        DeclNode::Function {
            params,
            return_type,
            body,
            ..
        } => {
            for param in params {
                param.ty = substitute_type(&param.ty, substitutions);
            }
            if let Some(ReturnType::Single(ty)) = return_type {
                *ty = substitute_type(ty, substitutions);
            }
            if let Some(body) = body {
                crate::monomorphize::substitute_block_types(body, substitutions);
            }
        }
        DeclNode::Variable { ty, init, .. } => {
            *ty = substitute_type(ty, substitutions);
            if let Some(init) = init {
                crate::monomorphize::substitute_expression_types(init, substitutions);
            }
        }
        DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
            crate::monomorphize::substitute_expression_types(init, substitutions);
        }
        DeclNode::Type { ty, .. } => *ty = substitute_type(ty, substitutions),
        DeclNode::Struct { fields, .. } => {
            for field in fields {
                field.ty = substitute_type(&field.ty, substitutions);
            }
        }
        DeclNode::Enum { variants, .. } => {
            for variant in variants {
                for payload in &mut variant.payload {
                    *payload = substitute_type(payload, substitutions);
                }
            }
        }
        DeclNode::ModuleImport { .. } | DeclNode::Interface { .. } => {}
    }
}

// --- Conversion-site rewriting ---

fn rewrite_block(
    block: &mut Block,
    environment: &mut HashMap<String, Type>,
    context: &mut DynContext<'_>,
) -> Result<(), String> {
    for statement in &mut block.statements {
        rewrite_statement(statement, environment, context)?;
    }
    Ok(())
}

fn rewrite_statement(
    statement: &mut Statement,
    environment: &mut HashMap<String, Type>,
    context: &mut DynContext<'_>,
) -> Result<(), String> {
    match statement {
        Statement::Expression(expr) | Statement::Defer(expr) | Statement::Return(Some(expr)) => {
            rewrite_expression(expr, environment, context)?;
        }
        Statement::Block(block) => {
            let mut nested = environment.clone();
            rewrite_block(block, &mut nested, context)?;
        }
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            rewrite_expression(cond, environment, context)?;
            let mut then_environment = environment.clone();
            rewrite_block(then_block, &mut then_environment, context)?;
            if let Some(branch) = else_branch {
                let mut else_environment = environment.clone();
                rewrite_statement(branch, &mut else_environment, context)?;
            }
        }
        Statement::While { cond, body } => {
            rewrite_expression(cond, environment, context)?;
            let mut nested = environment.clone();
            rewrite_block(body, &mut nested, context)?;
        }
        Statement::For { var, iter, body } => {
            let iteration_type = match &*iter {
                ForIter::Range { start, .. } => infer_type(start, environment, context),
                ForIter::Each(expr) => match infer_type(expr, environment, context) {
                    Some(Type::Array(_, inner) | Type::Slice(inner)) => Some(*inner),
                    _ => None,
                },
            };
            match iter {
                ForIter::Range { start, end, .. } => {
                    rewrite_expression(start, environment, context)?;
                    rewrite_expression(end, environment, context)?;
                }
                ForIter::Each(expr) => rewrite_expression(expr, environment, context)?,
            }
            let mut nested = environment.clone();
            if let Some(iteration_type) = iteration_type {
                nested.insert(var.clone(), iteration_type);
            }
            rewrite_block(body, &mut nested, context)?;
        }
        Statement::VariableDecl { name, ty, init } => {
            if let Some(init) = init {
                rewrite_expression(init, environment, context)?;
            }
            environment.insert(name.clone(), ty.clone());
        }
        Statement::InferredVariableDecl { name, init } => {
            rewrite_expression(init, environment, context)?;
            if let Some(ty) = infer_type(init, environment, context) {
                environment.insert(name.clone(), ty);
            }
        }
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
    Ok(())
}

fn rewrite_expression(
    expression: &mut Expression,
    environment: &mut HashMap<String, Type>,
    context: &mut DynContext<'_>,
) -> Result<(), String> {
    match expression {
        Expression::Assignment { target, rvalue } => {
            rewrite_target(target, environment, context)?;
            rewrite_expression(rvalue, environment, context)?;
        }
        Expression::Binary { left, right, .. } => {
            rewrite_expression(left, environment, context)?;
            rewrite_expression(right, environment, context)?;
        }
        Expression::Unary { expr, .. } | Expression::Try(expr) => {
            rewrite_expression(expr, environment, context)?;
        }
        Expression::Match { scrutinee, arms } => {
            rewrite_expression(scrutinee, environment, context)?;
            for arm in arms {
                let mut nested = environment.clone();
                rewrite_block(&mut arm.body, &mut nested, context)?;
                if let Some(value) = &mut arm.value {
                    rewrite_expression(value, &mut nested, context)?;
                }
            }
        }
        Expression::Cast { target_ty, expr } => {
            rewrite_expression(expr, environment, context)?;
            if let Some(interface) = dyn_cast_interface(target_ty, context) {
                let packed = pack_conversion(&interface, expr, environment, context)?;
                *expression = packed;
            }
        }
        Expression::Primary(primary) => match primary {
            PrimaryExpr::Grouped(expr) => rewrite_expression(expr, environment, context)?,
            PrimaryExpr::FunctionCall { arguments, .. } => {
                for argument in arguments {
                    rewrite_expression(argument, environment, context)?;
                }
            }
            PrimaryExpr::CallExpr { callee, arguments } => {
                rewrite_expression(callee, environment, context)?;
                for argument in arguments {
                    rewrite_expression(argument, environment, context)?;
                }
            }
            PrimaryExpr::ArrayLiteral(elements) => {
                for element in elements {
                    rewrite_expression(element, environment, context)?;
                }
            }
            PrimaryExpr::StructLiteral(fields) | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                for field in fields {
                    rewrite_expression(&mut field.expr, environment, context)?;
                }
            }
            PrimaryExpr::FieldAccess { expr, .. } => {
                rewrite_expression(expr, environment, context)?;
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                rewrite_expression(expr, environment, context)?;
                rewrite_expression(index, environment, context)?;
            }
            PrimaryExpr::Slice {
                expr, start, end, ..
            } => {
                rewrite_expression(expr, environment, context)?;
                if let Some(start) = start {
                    rewrite_expression(start, environment, context)?;
                }
                if let Some(end) = end {
                    rewrite_expression(end, environment, context)?;
                }
            }
            PrimaryExpr::New { args, .. } => {
                for arg in args {
                    rewrite_expression(arg, environment, context)?;
                }
            }
            PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
        },
    }
    Ok(())
}

fn rewrite_target(
    target: &mut crate::ast::AssignTarget,
    environment: &mut HashMap<String, Type>,
    context: &mut DynContext<'_>,
) -> Result<(), String> {
    use crate::ast::AssignTarget;
    match target {
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            rewrite_target(inner, environment, context)
        }
        AssignTarget::ArrayIndex { expr, index } => {
            rewrite_target(expr, environment, context)?;
            rewrite_expression(index, environment, context)
        }
        AssignTarget::Identifier(_) | AssignTarget::StructDestructure(_) => Ok(()),
    }
}

fn infer_type(
    expression: &Expression,
    environment: &HashMap<String, Type>,
    context: &DynContext<'_>,
) -> Option<Type> {
    let no_generics: HashMap<String, GenericSignature> = HashMap::new();
    infer_expression_type(
        expression,
        environment,
        &no_generics,
        &context.function_returns,
    )
}

// The interface behind a rewritten `dyn` cast target, if the cast is one.
fn dyn_cast_interface(target_ty: &Type, context: &DynContext<'_>) -> Option<String> {
    let Type::Named { name, args } = target_ty else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let interface = name.strip_suffix("__dyn")?;
    context
        .interfaces
        .contains_key(interface)
        .then(|| interface.to_owned())
}

// `expr as dyn I` becomes `I__dyn { .__data = expr as u8*, .__fn_m = thunk, ... }`.
fn pack_conversion(
    interface: &str,
    operand: &Expression,
    environment: &HashMap<String, Type>,
    context: &mut DynContext<'_>,
) -> Result<Expression, String> {
    let operand_type = infer_type(operand, environment, context).ok_or_else(|| {
        format!(
            "`as dyn {interface}` needs a pointer whose type is known here; \
             bind the value to a typed local first"
        )
    })?;
    let Type::Pointer(pointee) = &operand_type else {
        return Err(format!(
            "`as dyn {interface}` requires a pointer operand, got a non-pointer value"
        ));
    };
    let Type::Named {
        name: type_name,
        args,
    } = pointee.as_ref()
    else {
        return Err(format!(
            "`as dyn {interface}` requires a pointer to a named struct type"
        ));
    };
    if !args.is_empty() {
        return Err(format!(
            "`as dyn {interface}` does not support generic type instantiations yet"
        ));
    }

    let methods = &context.interfaces[interface];
    check_type_implements(pointee, interface, methods, &context.concrete_signatures)?;
    context.used_interfaces.insert(interface.to_owned());
    context
        .used_pairs
        .insert((interface.to_owned(), type_name.clone()));

    let mut fields = vec![FieldInit {
        name: data_field(),
        ty: None,
        expr: Expression::Cast {
            target_ty: u8_pointer(),
            expr: Box::new(operand.clone()),
        },
    }];
    for method in methods {
        fields.push(FieldInit {
            name: fn_field(&method.name),
            ty: None,
            expr: ident(&thunk_name(interface, type_name, &method.name)),
        });
    }
    Ok(Expression::Primary(PrimaryExpr::NamedStructLiteral {
        name: dyn_struct_name(interface),
        fields,
    }))
}
