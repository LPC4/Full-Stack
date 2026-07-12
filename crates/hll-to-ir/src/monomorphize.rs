use crate::ast::{
    AssignTarget, Block, DeclNode, Declaration, Expression, FieldInit, ForIter, InterfaceMethod,
    MatchArm, Pattern, PrimaryExpr, Program, ReturnType, Statement, Type, Variant,
};
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) type Substitutions = HashMap<String, Type>;
struct DeclarationBoundTemplate {
    kind: &'static str,
    generics: Vec<String>,
    bounds: Vec<(String, String)>,
    alias_target: Option<Type>,
}

type DeclarationBoundTemplates = HashMap<String, DeclarationBoundTemplate>;

// Upper bound on distinct function specializations; exceeding it means a generic
// recurses at an unbounded type sequence. Real programs stay far below this.
const SPECIALIZATION_LIMIT: usize = 2048;

// Concrete declarations keep generic types out of semantic analysis and IR.
pub(crate) fn monomorphize_program(program: &Program) -> Result<Program, String> {
    let mut source = program.clone();
    infer_implicit_type_arguments(&mut source)?;

    let definitions = source
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Function { name, generics, .. } if !generics.is_empty() => {
                Some((name.clone(), declaration.clone()))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    // Interface contracts and the concrete function pool for generic-bound checks.
    let interfaces = source
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Interface { name, methods } => Some((name.clone(), methods.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let concrete_signatures = source
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
        .collect::<HashMap<_, _>>();

    // A bound naming a missing interface is an error even if the generic is
    // never instantiated; catch the typo at its declaration.
    for declaration in &source.declarations {
        let (kind, name, generics, bounds) = match &declaration.decl {
            DeclNode::Function {
                name,
                generics,
                bounds,
                ..
            } => ("function", name, generics, bounds),
            DeclNode::Struct {
                name,
                generics,
                bounds,
                ..
            } => ("struct", name, generics, bounds),
            DeclNode::Enum {
                name,
                generics,
                bounds,
                ..
            } => ("enum", name, generics, bounds),
            DeclNode::Type {
                name,
                generics,
                bounds,
                ..
            } => ("type alias", name, generics, bounds),
            _ => continue,
        };
        for (param, interface_name) in bounds {
            if !generics.contains(param) {
                return Err(format!(
                    "generic {kind} `{name}`: bound `{param}: {interface_name}` names an undeclared parameter"
                ));
            }
            if !interfaces.contains_key(interface_name) {
                return Err(format!(
                    "generic {kind} `{name}`: bound `{param}: {interface_name}` \
                     names an unknown interface"
                ));
            }
        }
    }

    validate_declaration_bound_instantiations(&source, &source, &interfaces, &concrete_signatures)?;

    let mut queue = VecDeque::new();
    let mut queued = HashSet::new();
    let mut declarations = Vec::new();
    for declaration in &source.declarations {
        // Interfaces exist only for bound checking; drop them here so no later
        // stage has to know about them.
        if matches!(
            &declaration.decl,
            DeclNode::Function { generics, .. } if !generics.is_empty()
        ) || matches!(&declaration.decl, DeclNode::Interface { .. })
        {
            continue;
        }
        let mut declaration = declaration.clone();
        rewrite_declaration_calls(
            &mut declaration,
            &Substitutions::new(),
            &definitions,
            &mut queue,
            &mut queued,
        )?;
        declarations.push(declaration);
    }

    let mut statements = source.statements.clone();
    for statement in &mut statements {
        rewrite_statement_calls(
            statement,
            &Substitutions::new(),
            &definitions,
            &mut queue,
            &mut queued,
        )?;
    }

    let mut specialized_count = 0usize;
    while let Some((name, type_args)) = queue.pop_front() {
        // A generic instantiating itself at an ever-larger type never converges;
        // cap the work and report a cycle instead of looping forever.
        specialized_count += 1;
        if specialized_count > SPECIALIZATION_LIMIT {
            return Err(format!(
                "recursive generic specialization did not terminate (limit {SPECIALIZATION_LIMIT}); \
                 `{name}` is likely instantiated at an unbounded sequence of types"
            ));
        }
        let definition = definitions
            .get(&name)
            .ok_or_else(|| format!("unknown generic function `{name}`"))?;
        let DeclNode::Function {
            generics, bounds, ..
        } = &definition.decl
        else {
            unreachable!();
        };
        if generics.len() != type_args.len() {
            return Err(format!(
                "generic function `{name}` expects {} type arguments, got {}",
                generics.len(),
                type_args.len()
            ));
        }

        let substitutions = generics
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect::<Substitutions>();
        check_generic_bounds(
            &name,
            bounds,
            &substitutions,
            &interfaces,
            &concrete_signatures,
        )?;
        let mut specialized = definition.clone();
        specialize_function(&mut specialized, &name, &type_args, &substitutions);
        rewrite_declaration_calls(
            &mut specialized,
            &substitutions,
            &definitions,
            &mut queue,
            &mut queued,
        )?;
        declarations.push(specialized);
    }

    let mut program = Program {
        declarations,
        statements,
    };
    validate_declaration_bound_instantiations(
        &source,
        &program,
        &interfaces,
        &concrete_signatures,
    )?;
    // Runs on the fully concrete program so conversion sites see final types.
    crate::dyn_interface::lower_dyn_interfaces(&mut program, &interfaces)?;
    specialize_generic_enums(&mut program)?;
    Ok(program)
}

fn validate_declaration_bound_instantiations(
    template_program: &Program,
    usage_program: &Program,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
    concrete_signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
) -> Result<(), String> {
    let templates = template_program
        .declarations
        .iter()
        .filter_map(|decl| match &decl.decl {
            DeclNode::Struct {
                name,
                generics,
                bounds,
                ..
            } => Some((
                name.clone(),
                DeclarationBoundTemplate {
                    kind: "struct",
                    generics: generics.clone(),
                    bounds: bounds.clone(),
                    alias_target: None,
                },
            )),
            DeclNode::Enum {
                name,
                generics,
                bounds,
                ..
            } => Some((
                name.clone(),
                DeclarationBoundTemplate {
                    kind: "enum",
                    generics: generics.clone(),
                    bounds: bounds.clone(),
                    alias_target: None,
                },
            )),
            DeclNode::Type {
                name,
                generics,
                bounds,
                ty,
            } => Some((
                name.clone(),
                DeclarationBoundTemplate {
                    kind: "type alias",
                    generics: generics.clone(),
                    bounds: bounds.clone(),
                    alias_target: Some(ty.clone()),
                },
            )),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    validate_alias_cycles(&templates)?;

    fn check_type(
        ty: &Type,
        generic_scope: &HashSet<String>,
        templates: &DeclarationBoundTemplates,
        interfaces: &HashMap<String, Vec<InterfaceMethod>>,
        signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
    ) -> Result<(), String> {
        match ty {
            Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
                check_type(inner, generic_scope, templates, interfaces, signatures)?;
            }
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    check_type(param, generic_scope, templates, interfaces, signatures)?;
                }
                if let Some(ret) = return_type {
                    check_type(ret, generic_scope, templates, interfaces, signatures)?;
                }
            }
            Type::Struct(fields) => {
                for field in fields {
                    check_type(&field.ty, generic_scope, templates, interfaces, signatures)?;
                }
            }
            Type::Named { name, args } => {
                for arg in args {
                    check_type(arg, generic_scope, templates, interfaces, signatures)?;
                }
                if !type_contains_generic(ty, generic_scope)
                    && let Some(template) = templates.get(name)
                {
                    if template.generics.len() != args.len() {
                        return Err(format!(
                            "generic {} `{name}` expects {} type arguments, got {}",
                            template.kind,
                            template.generics.len(),
                            args.len()
                        ));
                    }
                    let substitutions = template
                        .generics
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    check_bounds_for_owner(
                        template.kind,
                        name,
                        &template.bounds,
                        &substitutions,
                        interfaces,
                        signatures,
                    )?;
                    if let Some(target) = &template.alias_target {
                        let expanded = substitute_type(target, &substitutions);
                        check_type(&expanded, generic_scope, templates, interfaces, signatures)?;
                    }
                }
            }
            Type::Primitive(_) => {}
        }
        Ok(())
    }

    fn type_contains_generic(ty: &Type, generic_scope: &HashSet<String>) -> bool {
        match ty {
            Type::Named { name, args } => {
                generic_scope.contains(name)
                    || args
                        .iter()
                        .any(|arg| type_contains_generic(arg, generic_scope))
            }
            Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
                type_contains_generic(inner, generic_scope)
            }
            Type::Function {
                params,
                return_type,
            } => {
                params
                    .iter()
                    .any(|param| type_contains_generic(param, generic_scope))
                    || return_type
                        .as_deref()
                        .is_some_and(|ret| type_contains_generic(ret, generic_scope))
            }
            Type::Struct(fields) => fields
                .iter()
                .any(|field| type_contains_generic(&field.ty, generic_scope)),
            Type::Primitive(_) => false,
        }
    }

    fn check_expression(
        expression: &Expression,
        generic_scope: &HashSet<String>,
        templates: &DeclarationBoundTemplates,
        interfaces: &HashMap<String, Vec<InterfaceMethod>>,
        signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
    ) -> Result<(), String> {
        let check_expr =
            |expr| check_expression(expr, generic_scope, templates, interfaces, signatures);
        match expression {
            Expression::Assignment { rvalue, .. } | Expression::Unary { expr: rvalue, .. } => {
                check_expr(rvalue)?;
            }
            Expression::Binary { left, right, .. } => {
                check_expr(left)?;
                check_expr(right)?;
            }
            Expression::Cast { target_ty, expr } => {
                check_type(target_ty, generic_scope, templates, interfaces, signatures)?;
                check_expr(expr)?;
            }
            Expression::Match { scrutinee, arms } => {
                check_expr(scrutinee)?;
                for arm in arms {
                    for statement in &arm.body.statements {
                        check_statement(
                            statement,
                            generic_scope,
                            templates,
                            interfaces,
                            signatures,
                        )?;
                    }
                    if let Some(value) = &arm.value {
                        check_expr(value)?;
                    }
                }
            }
            Expression::Try(expr) => check_expr(expr)?,
            Expression::Primary(primary) => match primary {
                PrimaryExpr::Grouped(expr) | PrimaryExpr::FieldAccess { expr, .. } => {
                    check_expr(expr)?;
                }
                PrimaryExpr::ArrayIndex { expr, index } => {
                    check_expr(expr)?;
                    check_expr(index)?;
                }
                PrimaryExpr::Slice {
                    expr, start, end, ..
                } => {
                    check_expr(expr)?;
                    if let Some(start) = start {
                        check_expr(start)?;
                    }
                    if let Some(end) = end {
                        check_expr(end)?;
                    }
                }
                PrimaryExpr::FunctionCall {
                    type_arguments,
                    arguments,
                    ..
                } => {
                    for ty in type_arguments {
                        check_type(ty, generic_scope, templates, interfaces, signatures)?;
                    }
                    for argument in arguments {
                        check_expr(argument)?;
                    }
                }
                PrimaryExpr::CallExpr { callee, arguments } => {
                    check_expr(callee)?;
                    for argument in arguments {
                        check_expr(argument)?;
                    }
                }
                PrimaryExpr::ArrayLiteral(values) => {
                    for value in values {
                        check_expr(value)?;
                    }
                }
                PrimaryExpr::StructLiteral(fields)
                | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                    for field in fields {
                        if let Some(ty) = &field.ty {
                            check_type(ty, generic_scope, templates, interfaces, signatures)?;
                        }
                        check_expr(&field.expr)?;
                    }
                }
                PrimaryExpr::New { ty, args } => {
                    check_type(ty, generic_scope, templates, interfaces, signatures)?;
                    for arg in args {
                        check_expr(arg)?;
                    }
                }
                PrimaryExpr::Identifier(_)
                | PrimaryExpr::Literal(_)
                | PrimaryExpr::AsmReg { .. } => {}
            },
        }
        Ok(())
    }

    fn check_statement(
        statement: &Statement,
        generic_scope: &HashSet<String>,
        templates: &DeclarationBoundTemplates,
        interfaces: &HashMap<String, Vec<InterfaceMethod>>,
        signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
    ) -> Result<(), String> {
        let check = |ty| check_type(ty, generic_scope, templates, interfaces, signatures);
        let check_expr =
            |expr| check_expression(expr, generic_scope, templates, interfaces, signatures);
        match statement {
            Statement::VariableDecl { ty, init, .. } => {
                check(ty)?;
                if let Some(init) = init {
                    check_expr(init)?;
                }
            }
            Statement::InferredVariableDecl { init, .. }
            | Statement::Expression(init)
            | Statement::Defer(init)
            | Statement::Return(Some(init)) => check_expr(init)?,
            Statement::Block(block) => {
                for stmt in &block.statements {
                    check_statement(stmt, generic_scope, templates, interfaces, signatures)?;
                }
            }
            Statement::If {
                cond,
                then_block,
                else_branch,
                ..
            } => {
                check_expr(cond)?;
                for stmt in &then_block.statements {
                    check_statement(stmt, generic_scope, templates, interfaces, signatures)?;
                }
                if let Some(branch) = else_branch {
                    check_statement(branch, generic_scope, templates, interfaces, signatures)?;
                }
            }
            Statement::While { cond, body } => {
                check_expr(cond)?;
                for stmt in &body.statements {
                    check_statement(stmt, generic_scope, templates, interfaces, signatures)?;
                }
            }
            Statement::For { iter, body, .. } => {
                match iter {
                    ForIter::Range { start, end, .. } => {
                        check_expr(start)?;
                        check_expr(end)?;
                    }
                    ForIter::Each(expr) => check_expr(expr)?,
                }
                for stmt in &body.statements {
                    check_statement(stmt, generic_scope, templates, interfaces, signatures)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    for declaration in &usage_program.declarations {
        match &declaration.decl {
            DeclNode::Function {
                generics,
                params,
                return_type,
                body,
                ..
            } => {
                let generic_scope = generics.iter().cloned().collect();
                for param in params {
                    check_type(
                        &param.ty,
                        &generic_scope,
                        &templates,
                        interfaces,
                        concrete_signatures,
                    )?;
                }
                if let Some(ReturnType::Single(ty)) = return_type {
                    check_type(
                        ty,
                        &generic_scope,
                        &templates,
                        interfaces,
                        concrete_signatures,
                    )?;
                }
                if let Some(body) = body {
                    for stmt in &body.statements {
                        check_statement(
                            stmt,
                            &generic_scope,
                            &templates,
                            interfaces,
                            concrete_signatures,
                        )?;
                    }
                }
            }
            DeclNode::Variable { ty, init, .. } => {
                let scope = HashSet::new();
                check_type(ty, &scope, &templates, interfaces, concrete_signatures)?;
                if let Some(init) = init {
                    check_expression(init, &scope, &templates, interfaces, concrete_signatures)?;
                }
            }
            DeclNode::Type { generics, ty, .. } => {
                let scope = generics.iter().cloned().collect();
                check_type(ty, &scope, &templates, interfaces, concrete_signatures)?;
            }
            DeclNode::Struct {
                generics, fields, ..
            } => {
                let scope = generics.iter().cloned().collect();
                for field in fields {
                    check_type(
                        &field.ty,
                        &scope,
                        &templates,
                        interfaces,
                        concrete_signatures,
                    )?;
                }
            }
            DeclNode::Enum {
                generics, variants, ..
            } => {
                let scope = generics.iter().cloned().collect();
                for variant in variants {
                    for ty in &variant.payload {
                        check_type(ty, &scope, &templates, interfaces, concrete_signatures)?;
                    }
                }
            }
            _ => {}
        }
    }
    let scope = HashSet::new();
    for statement in &usage_program.statements {
        check_statement(
            statement,
            &scope,
            &templates,
            interfaces,
            concrete_signatures,
        )?;
    }
    Ok(())
}

fn validate_alias_cycles(templates: &DeclarationBoundTemplates) -> Result<(), String> {
    fn dependencies(ty: &Type, templates: &DeclarationBoundTemplates, out: &mut Vec<String>) {
        match ty {
            Type::Named { name, args } => {
                if templates
                    .get(name)
                    .is_some_and(|item| item.alias_target.is_some())
                {
                    out.push(name.clone());
                }
                for arg in args {
                    dependencies(arg, templates, out);
                }
            }
            Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
                dependencies(inner, templates, out);
            }
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    dependencies(param, templates, out);
                }
                if let Some(ret) = return_type {
                    dependencies(ret, templates, out);
                }
            }
            Type::Struct(fields) => {
                for field in fields {
                    dependencies(&field.ty, templates, out);
                }
            }
            Type::Primitive(_) => {}
        }
    }

    fn visit(
        name: &str,
        templates: &DeclarationBoundTemplates,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if visited.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_owned()) {
            return Err(format!("generic type alias cycle involving `{name}`"));
        }
        let mut found = Vec::new();
        if let Some(target) = templates
            .get(name)
            .and_then(|item| item.alias_target.as_ref())
        {
            dependencies(target, templates, &mut found);
        }
        for dependency in found {
            visit(&dependency, templates, visiting, visited)?;
        }
        visiting.remove(name);
        visited.insert(name.to_owned());
        Ok(())
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for (name, template) in templates {
        if template.alias_target.is_some() {
            visit(name, templates, &mut visiting, &mut visited)?;
        }
    }
    Ok(())
}

fn check_bounds_for_owner(
    kind: &str,
    owner: &str,
    bounds: &[(String, String)],
    substitutions: &Substitutions,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
    concrete_signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
) -> Result<(), String> {
    for (param, interface_name) in bounds {
        let methods = &interfaces[interface_name];
        if let Some(concrete) = substitutions.get(param) {
            check_type_implements(concrete, interface_name, methods, concrete_signatures)
                .map_err(|error| format!("generic {kind} `{owner}`: {error}"))?;
        }
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct GenericSignature {
    params: Vec<String>,
    arguments: Vec<Type>,
    return_type: Option<Type>,
}

fn infer_implicit_type_arguments(program: &mut Program) -> Result<(), String> {
    let generic_signatures = program
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Function {
                name,
                generics,
                params,
                return_type,
                ..
            } if !generics.is_empty() => Some((
                name.clone(),
                GenericSignature {
                    params: generics.clone(),
                    arguments: params.iter().map(|param| param.ty.clone()).collect(),
                    return_type: return_type.as_ref().map(|return_type| match return_type {
                        ReturnType::Single(ty) => ty.clone(),
                    }),
                },
            )),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let function_returns = program
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.decl {
            DeclNode::Function {
                name, return_type, ..
            } => Some((
                name.clone(),
                return_type.as_ref().map(|return_type| match return_type {
                    ReturnType::Single(ty) => ty.clone(),
                }),
            )),
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    let mut globals = HashMap::new();
    for declaration in &program.declarations {
        if let DeclNode::Variable { name, ty, .. } = &declaration.decl {
            globals.insert(name.clone(), ty.clone());
        }
    }

    for declaration in &mut program.declarations {
        match &mut declaration.decl {
            DeclNode::Function {
                params,
                body: Some(body),
                ..
            } => {
                let mut environment = globals.clone();
                environment.extend(
                    params
                        .iter()
                        .map(|param| (param.name.clone(), param.ty.clone())),
                );
                infer_block_calls(
                    body,
                    &mut environment,
                    &generic_signatures,
                    &function_returns,
                )?;
            }
            DeclNode::Variable {
                init: Some(init), ..
            }
            | DeclNode::InferredVariable { init, .. }
            | DeclNode::Const { init, .. } => {
                infer_expression_calls(init, &globals, &generic_signatures, &function_returns)?;
            }
            _ => {}
        }
    }

    let mut environment = globals;
    for statement in &mut program.statements {
        infer_statement_calls(
            statement,
            &mut environment,
            &generic_signatures,
            &function_returns,
        )?;
    }
    Ok(())
}

fn infer_block_calls(
    block: &mut Block,
    environment: &mut HashMap<String, Type>,
    generic_signatures: &HashMap<String, GenericSignature>,
    function_returns: &HashMap<String, Option<Type>>,
) -> Result<(), String> {
    for statement in &mut block.statements {
        infer_statement_calls(statement, environment, generic_signatures, function_returns)?;
    }
    Ok(())
}

fn infer_statement_calls(
    statement: &mut Statement,
    environment: &mut HashMap<String, Type>,
    generic_signatures: &HashMap<String, GenericSignature>,
    function_returns: &HashMap<String, Option<Type>>,
) -> Result<(), String> {
    match statement {
        Statement::Expression(expr) | Statement::Defer(expr) => {
            infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
        }
        Statement::Block(block) => {
            let mut nested = environment.clone();
            infer_block_calls(block, &mut nested, generic_signatures, function_returns)?;
        }
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            infer_expression_calls(cond, environment, generic_signatures, function_returns)?;
            let mut then_environment = environment.clone();
            infer_block_calls(
                then_block,
                &mut then_environment,
                generic_signatures,
                function_returns,
            )?;
            if let Some(branch) = else_branch {
                let mut else_environment = environment.clone();
                infer_statement_calls(
                    branch,
                    &mut else_environment,
                    generic_signatures,
                    function_returns,
                )?;
            }
        }
        Statement::While { cond, body } => {
            infer_expression_calls(cond, environment, generic_signatures, function_returns)?;
            let mut nested = environment.clone();
            infer_block_calls(body, &mut nested, generic_signatures, function_returns)?;
        }
        Statement::For {
            var, iter, body, ..
        } => {
            let iteration_type = match &*iter {
                ForIter::Range { start, .. } => {
                    infer_expression_type(start, environment, generic_signatures, function_returns)
                }
                ForIter::Each(expr) => match infer_expression_type(
                    expr,
                    environment,
                    generic_signatures,
                    function_returns,
                ) {
                    Some(Type::Array(_, inner) | Type::Slice(inner)) => Some(*inner),
                    _ => None,
                },
            };
            match iter {
                ForIter::Range { start, end, .. } => {
                    infer_expression_calls(
                        start,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                    infer_expression_calls(end, environment, generic_signatures, function_returns)?;
                }
                ForIter::Each(expr) => {
                    infer_expression_calls(
                        expr,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
            }
            let mut nested = environment.clone();
            if let Some(iteration_type) = iteration_type {
                nested.insert(var.clone(), iteration_type);
            }
            infer_block_calls(body, &mut nested, generic_signatures, function_returns)?;
        }
        Statement::Return(Some(expr)) => {
            infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
        }
        Statement::VariableDecl { name, ty, init } => {
            if let Some(init) = init {
                infer_expression_calls(init, environment, generic_signatures, function_returns)?;
            }
            environment.insert(name.clone(), ty.clone());
        }
        Statement::InferredVariableDecl { name, init } => {
            infer_expression_calls(init, environment, generic_signatures, function_returns)?;
            if let Some(ty) =
                infer_expression_type(init, environment, generic_signatures, function_returns)
            {
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

fn infer_expression_calls(
    expression: &mut Expression,
    environment: &HashMap<String, Type>,
    generic_signatures: &HashMap<String, GenericSignature>,
    function_returns: &HashMap<String, Option<Type>>,
) -> Result<(), String> {
    match expression {
        Expression::Assignment { rvalue, .. } => {
            infer_expression_calls(rvalue, environment, generic_signatures, function_returns)?;
        }
        Expression::Binary { left, right, .. } => {
            infer_expression_calls(left, environment, generic_signatures, function_returns)?;
            infer_expression_calls(right, environment, generic_signatures, function_returns)?;
        }
        Expression::Unary { expr, .. }
        | Expression::Cast { expr, .. }
        | Expression::Try(expr)
        | Expression::Primary(PrimaryExpr::Grouped(expr)) => {
            infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
        }
        Expression::Match { scrutinee, arms } => {
            infer_expression_calls(scrutinee, environment, generic_signatures, function_returns)?;
            for arm in arms {
                let mut nested = environment.clone();
                infer_block_calls(
                    &mut arm.body,
                    &mut nested,
                    generic_signatures,
                    function_returns,
                )?;
                if let Some(value) = &mut arm.value {
                    infer_expression_calls(value, &nested, generic_signatures, function_returns)?;
                }
            }
        }
        Expression::Primary(primary) => match primary {
            PrimaryExpr::FunctionCall {
                name,
                type_arguments,
                arguments,
            } => {
                for argument in arguments.iter_mut() {
                    infer_expression_calls(
                        argument,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
                if type_arguments.is_empty()
                    && let Some(signature) = generic_signatures.get(name)
                {
                    let mut inferred = HashMap::new();
                    for (parameter, argument) in signature.arguments.iter().zip(arguments.iter()) {
                        let Some(actual) = infer_expression_type(
                            argument,
                            environment,
                            generic_signatures,
                            function_returns,
                        ) else {
                            continue;
                        };
                        unify_generic_type(parameter, &actual, &signature.params, &mut inferred)?;
                    }
                    if signature
                        .params
                        .iter()
                        .all(|param| inferred.contains_key(param))
                    {
                        *type_arguments = signature
                            .params
                            .iter()
                            .map(|param| inferred[param].clone())
                            .collect();
                    }
                }
            }
            PrimaryExpr::ArrayLiteral(elements) => {
                for element in elements {
                    infer_expression_calls(
                        element,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
            }
            PrimaryExpr::StructLiteral(fields) | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                for field in fields {
                    infer_expression_calls(
                        &mut field.expr,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
            }
            PrimaryExpr::CallExpr { callee, arguments } => {
                infer_expression_calls(callee, environment, generic_signatures, function_returns)?;
                for argument in arguments.iter_mut() {
                    infer_expression_calls(
                        argument,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
            }
            PrimaryExpr::FieldAccess { expr, .. } => {
                infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
                infer_expression_calls(index, environment, generic_signatures, function_returns)?;
            }
            PrimaryExpr::Slice {
                expr, start, end, ..
            } => {
                infer_expression_calls(expr, environment, generic_signatures, function_returns)?;
                if let Some(start) = start {
                    infer_expression_calls(
                        start,
                        environment,
                        generic_signatures,
                        function_returns,
                    )?;
                }
                if let Some(end) = end {
                    infer_expression_calls(end, environment, generic_signatures, function_returns)?;
                }
            }
            PrimaryExpr::New { args, .. } => {
                for arg in args {
                    infer_expression_calls(arg, environment, generic_signatures, function_returns)?;
                }
            }
            PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
            PrimaryExpr::Grouped(_) => unreachable!(),
        },
    }
    Ok(())
}

fn unify_generic_type(
    parameter: &Type,
    actual: &Type,
    generic_params: &[String],
    inferred: &mut HashMap<String, Type>,
) -> Result<(), String> {
    if let Type::Named { name, args } = parameter
        && args.is_empty()
        && generic_params.contains(name)
    {
        if let Some(previous) = inferred.get(name) {
            if previous != actual {
                return Err(format!(
                    "conflicting inferred types for `{name}`: `{}` and `{}`",
                    type_mangle(previous),
                    type_mangle(actual)
                ));
            }
        } else {
            inferred.insert(name.clone(), actual.clone());
        }
        return Ok(());
    }

    match (parameter, actual) {
        (Type::Pointer(parameter), Type::Pointer(actual))
        | (Type::Slice(parameter), Type::Slice(actual)) => {
            unify_generic_type(parameter, actual, generic_params, inferred)
        }
        (Type::Array(parameter_len, parameter), Type::Array(actual_len, actual))
            if parameter_len == actual_len =>
        {
            unify_generic_type(parameter, actual, generic_params, inferred)
        }
        (
            Type::Named {
                name: parameter_name,
                args: parameter_args,
            },
            Type::Named {
                name: actual_name,
                args: actual_args,
            },
        ) if parameter_name == actual_name && parameter_args.len() == actual_args.len() => {
            for (parameter, actual) in parameter_args.iter().zip(actual_args) {
                unify_generic_type(parameter, actual, generic_params, inferred)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn infer_expression_type(
    expression: &Expression,
    environment: &HashMap<String, Type>,
    generic_signatures: &HashMap<String, GenericSignature>,
    function_returns: &HashMap<String, Option<Type>>,
) -> Option<Type> {
    match expression {
        Expression::Primary(PrimaryExpr::Identifier(name)) => environment.get(name).cloned(),
        Expression::Primary(PrimaryExpr::Literal(literal)) => match literal {
            crate::ast::Literal::Integer(_) | crate::ast::Literal::HexInteger(_) => {
                Some(Type::Primitive("i32".to_owned()))
            }
            crate::ast::Literal::Float(_) => Some(Type::Primitive("f32".to_owned())),
            crate::ast::Literal::Boolean(_) => Some(Type::Primitive("bool".to_owned())),
            crate::ast::Literal::String(_) | crate::ast::Literal::Null => None,
        },
        Expression::Primary(PrimaryExpr::Grouped(expr)) => {
            infer_expression_type(expr, environment, generic_signatures, function_returns)
        }
        Expression::Primary(PrimaryExpr::New { ty, .. }) => {
            Some(Type::Pointer(Box::new(ty.clone())))
        }
        Expression::Primary(PrimaryExpr::FunctionCall {
            name,
            type_arguments,
            ..
        }) => {
            if let Some(signature) = generic_signatures.get(name) {
                let return_type = signature.return_type.as_ref()?;
                if signature.params.len() != type_arguments.len() {
                    return None;
                }
                let substitutions = signature
                    .params
                    .iter()
                    .cloned()
                    .zip(type_arguments.iter().cloned())
                    .collect();
                Some(substitute_type(return_type, &substitutions))
            } else {
                function_returns.get(name).cloned().flatten()
            }
        }
        Expression::Primary(PrimaryExpr::ArrayLiteral(elements)) => {
            let element = elements.first().and_then(|element| {
                infer_expression_type(element, environment, generic_signatures, function_returns)
            })?;
            Some(Type::Array(elements.len(), Box::new(element)))
        }
        Expression::Primary(PrimaryExpr::NamedStructLiteral { name, .. }) => Some(Type::Named {
            name: name.clone(),
            args: Vec::new(),
        }),
        Expression::Primary(PrimaryExpr::ArrayIndex { expr, .. }) => {
            match infer_expression_type(expr, environment, generic_signatures, function_returns)? {
                Type::Array(_, inner) | Type::Slice(inner) | Type::Pointer(inner) => Some(*inner),
                _ => None,
            }
        }
        Expression::Unary {
            op: crate::ast::UnaryOp::AddressOf,
            expr,
        } => infer_expression_type(expr, environment, generic_signatures, function_returns)
            .map(|ty| Type::Pointer(Box::new(ty))),
        Expression::Unary {
            op: crate::ast::UnaryOp::Dereference,
            expr,
        } => {
            match infer_expression_type(expr, environment, generic_signatures, function_returns)? {
                Type::Pointer(inner) => Some(*inner),
                _ => None,
            }
        }
        Expression::Unary { expr, .. } | Expression::Binary { left: expr, .. } => {
            infer_expression_type(expr, environment, generic_signatures, function_returns)
        }
        Expression::Cast { target_ty, .. } => Some(target_ty.clone()),
        Expression::Assignment { rvalue, .. } => {
            infer_expression_type(rvalue, environment, generic_signatures, function_returns)
        }
        Expression::Match { .. } | Expression::Try(_) => None,
        Expression::Primary(
            PrimaryExpr::StructLiteral(_)
            | PrimaryExpr::FieldAccess { .. }
            | PrimaryExpr::Slice { .. }
            | PrimaryExpr::CallExpr { .. }
            | PrimaryExpr::AsmReg { .. },
        ) => None,
    }
}

// Each bound in `<T: Writer + Debug>` holds when the substituted type provides
// every interface method as `{Type}_{method}` with `Self` = that type (see spec 6.5).
fn check_generic_bounds(
    function: &str,
    bounds: &[(String, String)],
    substitutions: &Substitutions,
    interfaces: &HashMap<String, Vec<InterfaceMethod>>,
    concrete_signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
) -> Result<(), String> {
    for (param, interface_name) in bounds {
        let methods = interfaces.get(interface_name).ok_or_else(|| {
            format!(
                "generic function `{function}`: bound `{param}: {interface_name}` \
                 names an unknown interface"
            )
        })?;
        let Some(concrete) = substitutions.get(param) else {
            continue;
        };
        let Type::Named { .. } = concrete else {
            return Err(format!(
                "generic function `{function}`: `{}` cannot satisfy the bound \
                 `{param}: {interface_name}`; only named struct types have methods",
                type_mangle(concrete)
            ));
        };
        check_type_implements(concrete, interface_name, methods, concrete_signatures)?;
    }
    Ok(())
}

// Shared by bound checking and `dyn` conversion: `type_name` implements the
// interface when every method exists as `{type}_{method}` with `Self` = type.
pub(crate) fn check_type_implements(
    concrete: &Type,
    interface_name: &str,
    methods: &[InterfaceMethod],
    concrete_signatures: &HashMap<String, (Vec<Type>, Option<Type>)>,
) -> Result<(), String> {
    let Type::Named {
        name: type_name,
        args,
    } = concrete
    else {
        return Err(format!(
            "type `{}` cannot implement interface `{interface_name}`; expected a named type",
            type_mangle(concrete)
        ));
    };
    let concrete_name = if args.is_empty() {
        type_name.clone()
    } else {
        format!(
            "{}<{}>",
            type_name,
            args.iter().map(type_mangle).collect::<Vec<_>>().join(", ")
        )
    };
    let self_substitution = Substitutions::from([("Self".to_owned(), concrete.clone())]);
    for method in methods {
        let implementation = format!("{type_name}_{}", method.name);
        let imported_suffix = format!("__{implementation}");
        let mut candidates = concrete_signatures
            .iter()
            .filter(|(name, _)| *name == &implementation || name.ends_with(&imported_suffix));
        let Some((resolved_name, (found_params, found_return))) = candidates.next() else {
            return Err(format!(
                "type `{concrete_name}` does not implement interface `{interface_name}`: \
                 missing method `{}` (expected a function `{implementation}`)",
                method.name
            ));
        };
        if candidates.next().is_some() {
            return Err(format!(
                "type `{concrete_name}` has ambiguous implementations for interface \
                 `{interface_name}` method `{}`; qualify the concrete type's module",
                method.name
            ));
        }
        let expected_params = method
            .params
            .iter()
            .map(|p| substitute_type(&p.ty, &self_substitution))
            .collect::<Vec<_>>();
        let expected_return = method
            .return_type
            .as_ref()
            .map(|ReturnType::Single(ty)| substitute_type(ty, &self_substitution));
        if found_params != &expected_params || found_return != &expected_return {
            return Err(format!(
                "type `{concrete_name}` does not implement interface `{interface_name}`: \
                 method `{}` has a mismatched signature (check `{resolved_name}` \
                 against the interface with `Self` = `{type_name}`)",
                method.name
            ));
        }
    }
    Ok(())
}

fn specialize_function(
    declaration: &mut Declaration,
    base_name: &str,
    type_args: &[Type],
    substitutions: &Substitutions,
) {
    let DeclNode::Function {
        name,
        generics,
        bounds,
        params,
        return_type,
        body,
        ..
    } = &mut declaration.decl
    else {
        unreachable!();
    };
    *name = specialized_name(base_name, type_args);
    generics.clear();
    bounds.clear();
    for param in params {
        param.ty = substitute_type(&param.ty, substitutions);
    }
    if let Some(ReturnType::Single(ty)) = return_type {
        *ty = substitute_type(ty, substitutions);
    }
    if let Some(body) = body {
        substitute_block_types(body, substitutions);
    }
}

fn rewrite_declaration_calls(
    declaration: &mut Declaration,
    substitutions: &Substitutions,
    definitions: &HashMap<String, Declaration>,
    queue: &mut VecDeque<(String, Vec<Type>)>,
    queued: &mut HashSet<String>,
) -> Result<(), String> {
    match &mut declaration.decl {
        DeclNode::Variable { init, .. } => {
            if let Some(init) = init {
                rewrite_expression_calls(init, substitutions, definitions, queue, queued)?;
            }
        }
        DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
            rewrite_expression_calls(init, substitutions, definitions, queue, queued)?;
        }
        DeclNode::Function {
            body: Some(body), ..
        } => {
            rewrite_block_calls(body, substitutions, definitions, queue, queued)?;
        }
        _ => {}
    }
    Ok(())
}

fn rewrite_block_calls(
    block: &mut Block,
    substitutions: &Substitutions,
    definitions: &HashMap<String, Declaration>,
    queue: &mut VecDeque<(String, Vec<Type>)>,
    queued: &mut HashSet<String>,
) -> Result<(), String> {
    for statement in &mut block.statements {
        rewrite_statement_calls(statement, substitutions, definitions, queue, queued)?;
    }
    Ok(())
}

fn rewrite_statement_calls(
    statement: &mut Statement,
    substitutions: &Substitutions,
    definitions: &HashMap<String, Declaration>,
    queue: &mut VecDeque<(String, Vec<Type>)>,
    queued: &mut HashSet<String>,
) -> Result<(), String> {
    match statement {
        Statement::Expression(expr) | Statement::Defer(expr) => {
            rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
        }
        Statement::Block(block) => {
            rewrite_block_calls(block, substitutions, definitions, queue, queued)?;
        }
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            rewrite_expression_calls(cond, substitutions, definitions, queue, queued)?;
            rewrite_block_calls(then_block, substitutions, definitions, queue, queued)?;
            if let Some(branch) = else_branch {
                rewrite_statement_calls(branch, substitutions, definitions, queue, queued)?;
            }
        }
        Statement::While { cond, body } => {
            rewrite_expression_calls(cond, substitutions, definitions, queue, queued)?;
            rewrite_block_calls(body, substitutions, definitions, queue, queued)?;
        }
        Statement::For { iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, .. } => {
                    rewrite_expression_calls(start, substitutions, definitions, queue, queued)?;
                    rewrite_expression_calls(end, substitutions, definitions, queue, queued)?;
                }
                ForIter::Each(expr) => {
                    rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
                }
            }
            rewrite_block_calls(body, substitutions, definitions, queue, queued)?;
        }
        Statement::Return(Some(expr)) => {
            rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
        }
        Statement::VariableDecl {
            init: Some(init), ..
        }
        | Statement::InferredVariableDecl { init, .. } => {
            rewrite_expression_calls(init, substitutions, definitions, queue, queued)?;
        }
        Statement::Return(None)
        | Statement::VariableDecl { init: None, .. }
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
    Ok(())
}

fn rewrite_expression_calls(
    expression: &mut Expression,
    substitutions: &Substitutions,
    definitions: &HashMap<String, Declaration>,
    queue: &mut VecDeque<(String, Vec<Type>)>,
    queued: &mut HashSet<String>,
) -> Result<(), String> {
    match expression {
        Expression::Assignment { target, rvalue } => {
            rewrite_target_calls(target, substitutions, definitions, queue, queued)?;
            rewrite_expression_calls(rvalue, substitutions, definitions, queue, queued)?;
        }
        Expression::Binary { left, right, .. } => {
            rewrite_expression_calls(left, substitutions, definitions, queue, queued)?;
            rewrite_expression_calls(right, substitutions, definitions, queue, queued)?;
        }
        Expression::Unary { expr, .. }
        | Expression::Cast { expr, .. }
        | Expression::Try(expr)
        | Expression::Primary(PrimaryExpr::Grouped(expr)) => {
            rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
        }
        Expression::Match { scrutinee, arms } => {
            rewrite_expression_calls(scrutinee, substitutions, definitions, queue, queued)?;
            for arm in arms {
                rewrite_block_calls(&mut arm.body, substitutions, definitions, queue, queued)?;
                if let Some(value) = &mut arm.value {
                    rewrite_expression_calls(value, substitutions, definitions, queue, queued)?;
                }
            }
        }
        Expression::Primary(primary) => match primary {
            PrimaryExpr::FunctionCall {
                name,
                type_arguments,
                arguments,
            } => {
                for argument in arguments {
                    rewrite_expression_calls(argument, substitutions, definitions, queue, queued)?;
                }
                let concrete_args = type_arguments
                    .iter()
                    .map(|ty| substitute_type(ty, substitutions))
                    .collect::<Vec<_>>();
                if definitions.contains_key(name) {
                    if concrete_args.is_empty() {
                        return Err(format!(
                            "generic function `{name}` requires explicit type arguments"
                        ));
                    }
                    let base_name = name.clone();
                    let concrete_name = specialized_name(&base_name, &concrete_args);
                    if queued.insert(concrete_name.clone()) {
                        queue.push_back((base_name, concrete_args));
                    }
                    *name = concrete_name;
                    type_arguments.clear();
                } else if !concrete_args.is_empty() {
                    return Err(format!("function `{name}` is not generic"));
                }
            }
            PrimaryExpr::ArrayLiteral(elements) => {
                for element in elements {
                    rewrite_expression_calls(element, substitutions, definitions, queue, queued)?;
                }
            }
            PrimaryExpr::StructLiteral(fields) | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                for FieldInit { expr, .. } in fields {
                    rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
                }
            }
            PrimaryExpr::CallExpr { callee, arguments } => {
                rewrite_expression_calls(callee, substitutions, definitions, queue, queued)?;
                for argument in arguments {
                    rewrite_expression_calls(argument, substitutions, definitions, queue, queued)?;
                }
            }
            PrimaryExpr::FieldAccess { expr, .. } => {
                rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
                rewrite_expression_calls(index, substitutions, definitions, queue, queued)?;
            }
            PrimaryExpr::Slice {
                expr, start, end, ..
            } => {
                rewrite_expression_calls(expr, substitutions, definitions, queue, queued)?;
                if let Some(start) = start {
                    rewrite_expression_calls(start, substitutions, definitions, queue, queued)?;
                }
                if let Some(end) = end {
                    rewrite_expression_calls(end, substitutions, definitions, queue, queued)?;
                }
            }
            PrimaryExpr::New { args, .. } => {
                for arg in args {
                    rewrite_expression_calls(arg, substitutions, definitions, queue, queued)?;
                }
            }
            PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
            PrimaryExpr::Grouped(_) => unreachable!(),
        },
    }
    Ok(())
}

fn rewrite_target_calls(
    target: &mut AssignTarget,
    substitutions: &Substitutions,
    definitions: &HashMap<String, Declaration>,
    queue: &mut VecDeque<(String, Vec<Type>)>,
    queued: &mut HashSet<String>,
) -> Result<(), String> {
    match target {
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            rewrite_target_calls(inner, substitutions, definitions, queue, queued)?;
        }
        AssignTarget::ArrayIndex { expr, index } => {
            rewrite_target_calls(expr, substitutions, definitions, queue, queued)?;
            rewrite_expression_calls(index, substitutions, definitions, queue, queued)?;
        }
        AssignTarget::Identifier(_) | AssignTarget::StructDestructure(_) => {}
    }
    Ok(())
}

pub(crate) fn substitute_block_types(block: &mut Block, substitutions: &Substitutions) {
    for statement in &mut block.statements {
        substitute_statement_types(statement, substitutions);
    }
}

pub(crate) fn substitute_statement_types(statement: &mut Statement, substitutions: &Substitutions) {
    match statement {
        Statement::Expression(expr) | Statement::Defer(expr) => {
            substitute_expression_types(expr, substitutions);
        }
        Statement::Block(block) => {
            substitute_block_types(block, substitutions);
        }
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            substitute_expression_types(cond, substitutions);
            substitute_block_types(then_block, substitutions);
            if let Some(branch) = else_branch {
                substitute_statement_types(branch, substitutions);
            }
        }
        Statement::While { cond, body } => {
            substitute_expression_types(cond, substitutions);
            substitute_block_types(body, substitutions);
        }
        Statement::For { iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, .. } => {
                    substitute_expression_types(start, substitutions);
                    substitute_expression_types(end, substitutions);
                }
                ForIter::Each(expr) => substitute_expression_types(expr, substitutions),
            }
            substitute_block_types(body, substitutions);
        }
        Statement::Return(Some(expr)) => substitute_expression_types(expr, substitutions),
        Statement::VariableDecl { ty, init, .. } => {
            *ty = substitute_type(ty, substitutions);
            if let Some(init) = init {
                substitute_expression_types(init, substitutions);
            }
        }
        Statement::InferredVariableDecl { init, .. } => {
            substitute_expression_types(init, substitutions);
        }
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
}

pub(crate) fn substitute_expression_types(
    expression: &mut Expression,
    substitutions: &Substitutions,
) {
    match expression {
        Expression::Assignment { target, rvalue } => {
            substitute_target_types(target, substitutions);
            substitute_expression_types(rvalue, substitutions);
        }
        Expression::Binary { left, right, .. } => {
            substitute_expression_types(left, substitutions);
            substitute_expression_types(right, substitutions);
        }
        Expression::Unary { expr, .. }
        | Expression::Try(expr)
        | Expression::Primary(PrimaryExpr::Grouped(expr)) => {
            substitute_expression_types(expr, substitutions);
        }
        Expression::Match { scrutinee, arms } => {
            substitute_expression_types(scrutinee, substitutions);
            for arm in arms {
                substitute_block_types(&mut arm.body, substitutions);
                if let Some(value) = &mut arm.value {
                    substitute_expression_types(value, substitutions);
                }
            }
        }
        Expression::Cast { target_ty, expr } => {
            *target_ty = substitute_type(target_ty, substitutions);
            substitute_expression_types(expr, substitutions);
        }
        Expression::Primary(primary) => match primary {
            PrimaryExpr::FunctionCall {
                type_arguments,
                arguments,
                ..
            } => {
                for ty in type_arguments {
                    *ty = substitute_type(ty, substitutions);
                }
                for argument in arguments {
                    substitute_expression_types(argument, substitutions);
                }
            }
            PrimaryExpr::ArrayLiteral(elements) => {
                for element in elements {
                    substitute_expression_types(element, substitutions);
                }
            }
            PrimaryExpr::StructLiteral(fields) | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                for field in fields {
                    if let Some(ty) = &mut field.ty {
                        *ty = substitute_type(ty, substitutions);
                    }
                    substitute_expression_types(&mut field.expr, substitutions);
                }
            }
            PrimaryExpr::CallExpr { callee, arguments } => {
                substitute_expression_types(callee, substitutions);
                for argument in arguments {
                    substitute_expression_types(argument, substitutions);
                }
            }
            PrimaryExpr::FieldAccess { expr, .. } => {
                substitute_expression_types(expr, substitutions);
            }
            PrimaryExpr::ArrayIndex { expr, index } => {
                substitute_expression_types(expr, substitutions);
                substitute_expression_types(index, substitutions);
            }
            PrimaryExpr::Slice {
                expr, start, end, ..
            } => {
                substitute_expression_types(expr, substitutions);
                if let Some(start) = start {
                    substitute_expression_types(start, substitutions);
                }
                if let Some(end) = end {
                    substitute_expression_types(end, substitutions);
                }
            }
            PrimaryExpr::New { ty, args } => {
                *ty = substitute_type(ty, substitutions);
                for arg in args {
                    substitute_expression_types(arg, substitutions);
                }
            }
            PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
            PrimaryExpr::Grouped(_) => unreachable!(),
        },
    }
}

fn substitute_target_types(target: &mut AssignTarget, substitutions: &Substitutions) {
    match target {
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            substitute_target_types(inner, substitutions);
        }
        AssignTarget::ArrayIndex { expr, index } => {
            substitute_target_types(expr, substitutions);
            substitute_expression_types(index, substitutions);
        }
        AssignTarget::StructDestructure(fields) => {
            for field in fields {
                if let Some(ty) = &mut field.ty {
                    *ty = substitute_type(ty, substitutions);
                }
            }
        }
        AssignTarget::Identifier(_) => {}
    }
}

pub(crate) fn substitute_type(ty: &Type, substitutions: &Substitutions) -> Type {
    match ty {
        Type::Named { name, args } if args.is_empty() => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Type::Named { name, args } => Type::Named {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_type(arg, substitutions))
                .collect(),
        },
        Type::Pointer(inner) => Type::Pointer(Box::new(substitute_type(inner, substitutions))),
        Type::Array(len, inner) => {
            Type::Array(*len, Box::new(substitute_type(inner, substitutions)))
        }
        Type::Slice(inner) => Type::Slice(Box::new(substitute_type(inner, substitutions))),
        Type::Function {
            params,
            return_type,
        } => Type::Function {
            params: params
                .iter()
                .map(|param| substitute_type(param, substitutions))
                .collect(),
            return_type: return_type
                .as_ref()
                .map(|ty| Box::new(substitute_type(ty, substitutions))),
        },
        Type::Struct(fields) => Type::Struct(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.ty = substitute_type(&field.ty, substitutions);
                    field
                })
                .collect(),
        ),
        Type::Primitive(_) => ty.clone(),
    }
}

fn specialized_name(name: &str, type_args: &[Type]) -> String {
    let suffix = type_args
        .iter()
        .map(type_mangle)
        .collect::<Vec<_>>()
        .join("__");
    format!("{name}__{suffix}")
}

fn type_mangle(ty: &Type) -> String {
    match ty {
        Type::Primitive(name) => name.clone(),
        Type::Pointer(inner) => format!("ptr_{}", type_mangle(inner)),
        Type::Array(len, inner) => format!("arr{len}_{}", type_mangle(inner)),
        Type::Slice(inner) => format!("slice_{}", type_mangle(inner)),
        Type::Function {
            params,
            return_type,
        } => {
            let params = params.iter().map(type_mangle).collect::<Vec<_>>().join("_");
            let ret = return_type
                .as_deref()
                .map(type_mangle)
                .unwrap_or_else(|| "void".to_owned());
            format!("fn_{params}_ret_{ret}")
        }
        Type::Struct(_) => "struct".to_owned(),
        Type::Named { name, args } if args.is_empty() => name.clone(),
        Type::Named { name, args } => format!(
            "{}_{}",
            name,
            args.iter().map(type_mangle).collect::<Vec<_>>().join("_")
        ),
    }
}

// --- Generic enum specialization (M7) ---
// Concretize generic enums (incl. the Option/Result prelude) so only mangled,
// monomorphic enums reach semantics/IR. See _LANG_SPECIFICATIONS.md 7.2.

struct EnumTemplate {
    generics: Vec<String>,
    variants: Vec<Variant>,
}

struct EnumSpecializer {
    templates: HashMap<String, EnumTemplate>,
    // Generic-enum variant constructor name -> owning base enum name.
    variant_owner: HashMap<String, String>,
    // mangled enum -> base name.
    base_of: HashMap<String, String>,
    // mangled enum -> concrete type args.
    args_of: HashMap<String, Vec<Type>>,
    // mangled enum -> (base variant -> mangled variant).
    variant_maps: HashMap<String, HashMap<String, String>>,
    pending: VecDeque<String>,
    emitted: HashSet<String>,
    function_returns: HashMap<String, Type>,
    function_params: HashMap<String, Vec<Type>>,
    current_return: Option<Type>,
}

pub(crate) fn specialize_generic_enums(program: &mut Program) -> Result<(), String> {
    let mut spec = EnumSpecializer::collect(program);
    if spec.templates.is_empty() {
        return Ok(());
    }

    // Pass 1: rewrite every concrete type reference, queuing each instantiation.
    spec.rewrite_program_types(program);

    // Build concrete function signatures for use-site expected types (post-rewrite).
    spec.collect_signatures(program);

    // Pass 2: rewrite constructor expressions and match patterns using expected types.
    spec.rewrite_program_uses(program);

    // Pass 3: emit one concrete enum declaration per queued instantiation.
    spec.emit_specializations(program)
}

impl EnumSpecializer {
    fn collect(program: &mut Program) -> Self {
        let mut templates = HashMap::new();
        let mut defined: HashSet<String> = HashSet::new();
        // Lift generic enum declarations out of the program; they are templates.
        program.declarations.retain(|declaration| {
            if let DeclNode::Enum {
                name,
                generics,
                variants,
                ..
            } = &declaration.decl
            {
                defined.insert(name.clone());
                if !generics.is_empty() {
                    templates.insert(
                        name.clone(),
                        EnumTemplate {
                            generics: generics.clone(),
                            variants: variants.clone(),
                        },
                    );
                    return false;
                }
            }
            true
        });

        // Inject the Option/Result prelude unless the user defined those names.
        if !defined.contains("Option") {
            templates.insert(
                "Option".to_owned(),
                EnumTemplate {
                    generics: vec!["T".to_owned()],
                    variants: vec![
                        Variant {
                            name: "Some".to_owned(),
                            payload: vec![named("T")],
                        },
                        Variant {
                            name: "None".to_owned(),
                            payload: Vec::new(),
                        },
                    ],
                },
            );
        }
        if !defined.contains("Result") {
            templates.insert(
                "Result".to_owned(),
                EnumTemplate {
                    generics: vec!["T".to_owned(), "E".to_owned()],
                    variants: vec![
                        Variant {
                            name: "Ok".to_owned(),
                            payload: vec![named("T")],
                        },
                        Variant {
                            name: "Err".to_owned(),
                            payload: vec![named("E")],
                        },
                    ],
                },
            );
        }

        let mut variant_owner = HashMap::new();
        for (base, template) in &templates {
            for variant in &template.variants {
                variant_owner.insert(variant.name.clone(), base.clone());
            }
        }

        Self {
            templates,
            variant_owner,
            base_of: HashMap::new(),
            args_of: HashMap::new(),
            variant_maps: HashMap::new(),
            pending: VecDeque::new(),
            emitted: HashSet::new(),
            function_returns: HashMap::new(),
            function_params: HashMap::new(),
            current_return: None,
        }
    }

    // Record an instantiation and return its mangled enum name. Idempotent.
    fn queue(&mut self, base: &str, args: &[Type]) -> String {
        let mangled = mangle_enum(base, args);
        if !self.base_of.contains_key(&mangled) {
            let template = &self.templates[base];
            let mut map = HashMap::new();
            for variant in &template.variants {
                map.insert(variant.name.clone(), format!("{}__{mangled}", variant.name));
            }
            self.variant_maps.insert(mangled.clone(), map);
            self.base_of.insert(mangled.clone(), base.to_owned());
            self.args_of.insert(mangled.clone(), args.to_vec());
            self.pending.push_back(mangled.clone());
        }
        mangled
    }

    // Rewrite a type in place: a generic-enum instantiation becomes its mangled name.
    fn rewrite_type(&mut self, ty: &mut Type) {
        match ty {
            Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
                self.rewrite_type(inner);
            }
            Type::Function {
                params,
                return_type,
            } => {
                for param in params {
                    self.rewrite_type(param);
                }
                if let Some(return_type) = return_type {
                    self.rewrite_type(return_type);
                }
            }
            Type::Struct(fields) => {
                for field in fields {
                    self.rewrite_type(&mut field.ty);
                }
            }
            Type::Named { name, args } => {
                for arg in args.iter_mut() {
                    self.rewrite_type(arg);
                }
                if !args.is_empty() && self.templates.contains_key(name) {
                    let mangled = self.queue(name, args);
                    *ty = Type::Named {
                        name: mangled,
                        args: Vec::new(),
                    };
                }
            }
            Type::Primitive(_) => {}
        }
    }

    fn collect_signatures(&mut self, program: &Program) {
        for declaration in &program.declarations {
            if let DeclNode::Function {
                name,
                params,
                return_type,
                ..
            } = &declaration.decl
            {
                self.function_params
                    .insert(name.clone(), params.iter().map(|p| p.ty.clone()).collect());
                if let Some(ReturnType::Single(ty)) = return_type {
                    self.function_returns.insert(name.clone(), ty.clone());
                }
            }
        }
    }

    // Concrete payload types of `variant` for a specific mangled instantiation.
    fn payload_types(&self, mangled: &str, variant: &str) -> Option<Vec<Type>> {
        let base = self.base_of.get(mangled)?;
        let args = self.args_of.get(mangled)?;
        let template = self.templates.get(base)?;
        let substitutions = template
            .generics
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<Substitutions>();
        template
            .variants
            .iter()
            .find(|candidate| candidate.name == variant)
            .map(|found| {
                found
                    .payload
                    .iter()
                    .map(|ty| substitute_type(ty, &substitutions))
                    .collect()
            })
    }

    // --- Pass 1: type rewriting across the whole program ---

    fn rewrite_program_types(&mut self, program: &mut Program) {
        for declaration in &mut program.declarations {
            match &mut declaration.decl {
                DeclNode::Function {
                    generics,
                    params,
                    return_type,
                    body,
                    ..
                } if generics.is_empty() => {
                    for param in params {
                        self.rewrite_type(&mut param.ty);
                    }
                    if let Some(ReturnType::Single(ty)) = return_type {
                        self.rewrite_type(ty);
                    }
                    if let Some(body) = body {
                        self.rewrite_block_types(body);
                    }
                }
                DeclNode::Variable { ty, init, .. } => {
                    self.rewrite_type(ty);
                    if let Some(init) = init {
                        self.rewrite_expr_types(init);
                    }
                }
                DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
                    self.rewrite_expr_types(init);
                }
                DeclNode::Struct {
                    generics, fields, ..
                } if generics.is_empty() => {
                    for field in fields {
                        self.rewrite_type(&mut field.ty);
                    }
                }
                _ => {}
            }
        }
        let mut statements = std::mem::take(&mut program.statements);
        for statement in &mut statements {
            self.rewrite_stmt_types(statement);
        }
        program.statements = statements;
    }

    fn rewrite_block_types(&mut self, block: &mut Block) {
        for statement in &mut block.statements {
            self.rewrite_stmt_types(statement);
        }
    }

    fn rewrite_stmt_types(&mut self, statement: &mut Statement) {
        match statement {
            Statement::Expression(expr)
            | Statement::Defer(expr)
            | Statement::Return(Some(expr)) => {
                self.rewrite_expr_types(expr);
            }
            Statement::Block(block) => self.rewrite_block_types(block),
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                self.rewrite_expr_types(cond);
                self.rewrite_block_types(then_block);
                if let Some(branch) = else_branch {
                    self.rewrite_stmt_types(branch);
                }
            }
            Statement::While { cond, body } => {
                self.rewrite_expr_types(cond);
                self.rewrite_block_types(body);
            }
            Statement::For { iter, body, .. } => {
                match iter {
                    ForIter::Range { start, end, .. } => {
                        self.rewrite_expr_types(start);
                        self.rewrite_expr_types(end);
                    }
                    ForIter::Each(expr) => self.rewrite_expr_types(expr),
                }
                self.rewrite_block_types(body);
            }
            Statement::VariableDecl { ty, init, .. } => {
                self.rewrite_type(ty);
                if let Some(init) = init {
                    self.rewrite_expr_types(init);
                }
            }
            Statement::InferredVariableDecl { init, .. } => self.rewrite_expr_types(init),
            Statement::Return(None)
            | Statement::AsmBlock { .. }
            | Statement::Break
            | Statement::Continue => {}
        }
    }

    fn rewrite_expr_types(&mut self, expression: &mut Expression) {
        match expression {
            Expression::Assignment { rvalue, .. } => self.rewrite_expr_types(rvalue),
            Expression::Binary { left, right, .. } => {
                self.rewrite_expr_types(left);
                self.rewrite_expr_types(right);
            }
            Expression::Unary { expr, .. } | Expression::Try(expr) => self.rewrite_expr_types(expr),
            Expression::Cast { target_ty, expr } => {
                self.rewrite_type(target_ty);
                self.rewrite_expr_types(expr);
            }
            Expression::Match { scrutinee, arms } => {
                self.rewrite_expr_types(scrutinee);
                for arm in arms {
                    self.rewrite_block_types(&mut arm.body);
                    if let Some(value) = &mut arm.value {
                        self.rewrite_expr_types(value);
                    }
                }
            }
            Expression::Primary(primary) => match primary {
                PrimaryExpr::Grouped(expr) | PrimaryExpr::FieldAccess { expr, .. } => {
                    self.rewrite_expr_types(expr);
                }
                PrimaryExpr::CallExpr { callee, arguments } => {
                    self.rewrite_expr_types(callee);
                    for argument in arguments {
                        self.rewrite_expr_types(argument);
                    }
                }
                PrimaryExpr::FunctionCall {
                    type_arguments,
                    arguments,
                    ..
                } => {
                    for ty in type_arguments {
                        self.rewrite_type(ty);
                    }
                    for argument in arguments {
                        self.rewrite_expr_types(argument);
                    }
                }
                PrimaryExpr::ArrayLiteral(elements) => {
                    for element in elements {
                        self.rewrite_expr_types(element);
                    }
                }
                PrimaryExpr::StructLiteral(fields)
                | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                    for field in fields {
                        if let Some(ty) = &mut field.ty {
                            self.rewrite_type(ty);
                        }
                        self.rewrite_expr_types(&mut field.expr);
                    }
                }
                PrimaryExpr::ArrayIndex { expr, index } => {
                    self.rewrite_expr_types(expr);
                    self.rewrite_expr_types(index);
                }
                PrimaryExpr::Slice {
                    expr, start, end, ..
                } => {
                    self.rewrite_expr_types(expr);
                    if let Some(start) = start {
                        self.rewrite_expr_types(start);
                    }
                    if let Some(end) = end {
                        self.rewrite_expr_types(end);
                    }
                }
                PrimaryExpr::New { ty, args } => {
                    self.rewrite_type(ty);
                    for arg in args {
                        self.rewrite_expr_types(arg);
                    }
                }
                PrimaryExpr::Identifier(_)
                | PrimaryExpr::Literal(_)
                | PrimaryExpr::AsmReg { .. } => {}
            },
        }
    }

    // --- Pass 2: constructor and pattern rewriting ---

    fn rewrite_program_uses(&mut self, program: &mut Program) {
        let mut globals: HashMap<String, Type> = HashMap::new();
        for declaration in &program.declarations {
            if let DeclNode::Variable { name, ty, .. } = &declaration.decl {
                globals.insert(name.clone(), ty.clone());
            }
        }
        for declaration in &mut program.declarations {
            if let DeclNode::Function {
                generics,
                params,
                return_type,
                body: Some(body),
                ..
            } = &mut declaration.decl
            {
                if !generics.is_empty() {
                    continue;
                }
                let mut env = globals.clone();
                for param in params.iter() {
                    env.insert(param.name.clone(), param.ty.clone());
                }
                self.current_return = return_type
                    .as_mut()
                    .map(|ReturnType::Single(ty)| ty.clone());
                self.rewrite_block_uses(body, &mut env);
            }
        }
        self.current_return = None;
        let mut env = globals;
        let mut statements = std::mem::take(&mut program.statements);
        for statement in &mut statements {
            self.rewrite_stmt_uses(statement, &mut env);
        }
        program.statements = statements;
    }

    fn rewrite_block_uses(&mut self, block: &mut Block, env: &mut HashMap<String, Type>) {
        let mut scoped = env.clone();
        for statement in &mut block.statements {
            self.rewrite_stmt_uses(statement, &mut scoped);
        }
    }

    fn rewrite_stmt_uses(&mut self, statement: &mut Statement, env: &mut HashMap<String, Type>) {
        match statement {
            Statement::Expression(expr) | Statement::Defer(expr) => {
                self.rewrite_expr_uses(expr, None, env);
            }
            Statement::Return(Some(expr)) => {
                let expected = self.current_return.clone();
                self.rewrite_expr_uses(expr, expected.as_ref(), env);
            }
            Statement::Block(block) => self.rewrite_block_uses(block, env),
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                self.rewrite_expr_uses(cond, None, env);
                self.rewrite_block_uses(then_block, env);
                if let Some(branch) = else_branch {
                    self.rewrite_stmt_uses(branch, env);
                }
            }
            Statement::While { cond, body } => {
                self.rewrite_expr_uses(cond, None, env);
                self.rewrite_block_uses(body, env);
            }
            Statement::For { var, iter, body } => {
                let element = match iter {
                    ForIter::Range { start, end, .. } => {
                        self.rewrite_expr_uses(start, None, env);
                        self.rewrite_expr_uses(end, None, env);
                        None
                    }
                    ForIter::Each(expr) => {
                        self.rewrite_expr_uses(expr, None, env);
                        match self.infer_type(expr, env) {
                            Some(Type::Array(_, inner) | Type::Slice(inner)) => Some(*inner),
                            _ => None,
                        }
                    }
                };
                let mut scoped = env.clone();
                if let Some(element) = element {
                    scoped.insert(var.clone(), element);
                }
                self.rewrite_block_uses(body, &mut scoped);
            }
            Statement::VariableDecl { name, ty, init } => {
                if let Some(init) = init {
                    let expected = ty.clone();
                    self.rewrite_expr_uses(init, Some(&expected), env);
                }
                env.insert(name.clone(), ty.clone());
            }
            Statement::InferredVariableDecl { name, init } => {
                self.rewrite_expr_uses(init, None, env);
                if let Some(ty) = self.infer_type(init, env) {
                    env.insert(name.clone(), ty);
                }
            }
            Statement::Return(None)
            | Statement::AsmBlock { .. }
            | Statement::Break
            | Statement::Continue => {}
        }
    }

    fn rewrite_expr_uses(
        &mut self,
        expression: &mut Expression,
        expected: Option<&Type>,
        env: &mut HashMap<String, Type>,
    ) {
        match expression {
            Expression::Assignment { target, rvalue } => {
                let target_ty = self.target_type(target, env);
                self.rewrite_expr_uses(rvalue, target_ty.as_ref(), env);
            }
            Expression::Binary { left, right, .. } => {
                self.rewrite_expr_uses(left, None, env);
                self.rewrite_expr_uses(right, None, env);
            }
            Expression::Unary { expr, .. } | Expression::Try(expr) => {
                self.rewrite_expr_uses(expr, None, env);
            }
            Expression::Cast { expr, .. } => self.rewrite_expr_uses(expr, None, env),
            Expression::Match { scrutinee, arms } => self.rewrite_match(scrutinee, arms, env),
            Expression::Primary(primary) => match primary {
                PrimaryExpr::Identifier(name) => {
                    if let Some(mangled) = self.constructor_target(name, expected) {
                        *name = mangled;
                    }
                }
                PrimaryExpr::FunctionCall {
                    name, arguments, ..
                } => {
                    if let Some((mangled, payloads)) =
                        self.constructor_call_target(name, expected, arguments.len())
                    {
                        *name = mangled;
                        for (argument, payload) in arguments.iter_mut().zip(payloads) {
                            self.rewrite_expr_uses(argument, Some(&payload), env);
                        }
                    } else {
                        let params = self.function_params.get(name).cloned();
                        for (index, argument) in arguments.iter_mut().enumerate() {
                            let expected = params.as_ref().and_then(|p| p.get(index));
                            self.rewrite_expr_uses(argument, expected, env);
                        }
                    }
                }
                PrimaryExpr::Grouped(expr) => self.rewrite_expr_uses(expr, expected, env),
                PrimaryExpr::FieldAccess { expr, .. } => self.rewrite_expr_uses(expr, None, env),
                PrimaryExpr::CallExpr { callee, arguments } => {
                    self.rewrite_expr_uses(callee, None, env);
                    for argument in arguments {
                        self.rewrite_expr_uses(argument, None, env);
                    }
                }
                PrimaryExpr::ArrayLiteral(elements) => {
                    let element = match expected {
                        Some(Type::Array(_, inner) | Type::Slice(inner)) => Some((**inner).clone()),
                        _ => None,
                    };
                    for item in elements {
                        self.rewrite_expr_uses(item, element.as_ref(), env);
                    }
                }
                PrimaryExpr::StructLiteral(fields)
                | PrimaryExpr::NamedStructLiteral { fields, .. } => {
                    for field in fields {
                        self.rewrite_expr_uses(&mut field.expr, None, env);
                    }
                }
                PrimaryExpr::ArrayIndex { expr, index } => {
                    self.rewrite_expr_uses(expr, None, env);
                    self.rewrite_expr_uses(index, None, env);
                }
                PrimaryExpr::Slice {
                    expr, start, end, ..
                } => {
                    self.rewrite_expr_uses(expr, None, env);
                    if let Some(start) = start {
                        self.rewrite_expr_uses(start, None, env);
                    }
                    if let Some(end) = end {
                        self.rewrite_expr_uses(end, None, env);
                    }
                }
                PrimaryExpr::New { args, .. } => {
                    for arg in args {
                        self.rewrite_expr_uses(arg, None, env);
                    }
                }
                PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
            },
        }
    }

    // Resolve a unit-variant constructor (`None`) against the expected enum type,
    // returning its mangled variant name.
    fn constructor_target(&self, name: &str, expected: Option<&Type>) -> Option<String> {
        let mangled_enum = expected_enum_name(expected)?;
        let owner = self.variant_owner.get(name)?;
        if self.base_of.get(&mangled_enum)? != owner {
            return None;
        }
        self.variant_maps.get(&mangled_enum)?.get(name).cloned()
    }

    // A payload-variant constructor call (`Some(x)`). Returns the mangled variant
    // name and the concrete payload types to use as expected types for arguments.
    fn constructor_call_target(
        &self,
        name: &str,
        expected: Option<&Type>,
        arity: usize,
    ) -> Option<(String, Vec<Type>)> {
        let mangled = self.constructor_target(name, expected)?;
        let mangled_enum = expected_enum_name(expected)?;
        let payloads = self.payload_types(&mangled_enum, name)?;
        if payloads.len() != arity {
            return None;
        }
        Some((mangled, payloads))
    }

    fn rewrite_match(
        &mut self,
        scrutinee: &mut Expression,
        arms: &mut [MatchArm],
        env: &mut HashMap<String, Type>,
    ) {
        self.rewrite_expr_uses(scrutinee, None, env);
        let mangled_enum = self
            .infer_type(scrutinee, env)
            .and_then(|ty| expected_enum_name(Some(&ty)));

        for arm in arms.iter_mut() {
            let mut scoped = env.clone();
            if let (
                Some(mangled_enum),
                Pattern::Variant {
                    variant, bindings, ..
                },
            ) = (&mangled_enum, &mut arm.pattern)
            {
                if let Some(payloads) = self.payload_types(mangled_enum, variant) {
                    for (binding, payload) in bindings.iter().zip(payloads) {
                        if binding != "_" {
                            scoped.insert(binding.clone(), payload);
                        }
                    }
                }
                if let Some(mangled_variant) = self
                    .variant_maps
                    .get(mangled_enum)
                    .and_then(|map| map.get(variant))
                {
                    *variant = mangled_variant.clone();
                }
            }
            self.rewrite_block_uses(&mut arm.body, &mut scoped);
            if let Some(value) = &mut arm.value {
                self.rewrite_expr_uses(value, None, &mut scoped);
            }
        }
    }

    fn target_type(&self, target: &AssignTarget, env: &HashMap<String, Type>) -> Option<Type> {
        match target {
            AssignTarget::Identifier(name) => env.get(name).cloned(),
            _ => None,
        }
    }

    // Minimal type inference, sufficient for match scrutinees and inferred decls.
    fn infer_type(&self, expr: &Expression, env: &HashMap<String, Type>) -> Option<Type> {
        match expr {
            Expression::Primary(PrimaryExpr::Identifier(name)) => env.get(name).cloned(),
            Expression::Primary(PrimaryExpr::Grouped(inner)) => self.infer_type(inner, env),
            Expression::Cast { target_ty, .. } => Some(target_ty.clone()),
            Expression::Primary(PrimaryExpr::New { ty, .. }) => {
                Some(Type::Pointer(Box::new(ty.clone())))
            }
            Expression::Primary(PrimaryExpr::FunctionCall { name, .. }) => {
                self.function_returns.get(name).cloned()
            }
            Expression::Primary(PrimaryExpr::ArrayIndex { expr, .. }) => {
                match self.infer_type(expr, env)? {
                    Type::Array(_, inner) | Type::Slice(inner) | Type::Pointer(inner) => {
                        Some(*inner)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // --- Pass 3: emit concrete enum declarations ---

    fn emit_specializations(&mut self, program: &mut Program) -> Result<(), String> {
        let mut emitted_count = 0usize;
        while let Some(mangled) = self.pending.pop_front() {
            if !self.emitted.insert(mangled.clone()) {
                continue;
            }
            // A generic enum referencing itself at a growing type never converges.
            emitted_count += 1;
            if emitted_count > SPECIALIZATION_LIMIT {
                return Err(format!(
                    "recursive generic enum specialization did not terminate \
                     (limit {SPECIALIZATION_LIMIT}); `{mangled}` is instantiated unboundedly"
                ));
            }
            let base = self.base_of[&mangled].clone();
            let args = self.args_of[&mangled].clone();
            let substitutions = self.templates[&base]
                .generics
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<Substitutions>();
            let variant_map = self.variant_maps[&mangled].clone();
            let template_variants = self.templates[&base].variants.clone();

            let mut variants = Vec::with_capacity(template_variants.len());
            for variant in template_variants {
                let mut payload: Vec<Type> = variant
                    .payload
                    .iter()
                    .map(|ty| substitute_type(ty, &substitutions))
                    .collect();
                // Payloads may themselves reference generic enums; concretize them.
                for ty in &mut payload {
                    self.rewrite_type(ty);
                }
                variants.push(Variant {
                    name: variant_map[&variant.name].clone(),
                    payload,
                });
            }

            program.declarations.push(Declaration {
                decl: DeclNode::Enum {
                    name: mangled,
                    generics: Vec::new(),
                    bounds: Vec::new(),
                    variants,
                },
                exported: false,
            });
        }
        Ok(())
    }
}

fn named(name: &str) -> Type {
    Type::Named {
        name: name.to_owned(),
        args: Vec::new(),
    }
}

// A concrete enum type's mangled name, or None if `expected` is not a plain Named.
fn expected_enum_name(expected: Option<&Type>) -> Option<String> {
    match expected? {
        Type::Named { name, args } if args.is_empty() => Some(name.clone()),
        _ => None,
    }
}

fn mangle_enum(base: &str, args: &[Type]) -> String {
    let suffix = args.iter().map(type_mangle).collect::<Vec<_>>().join("__");
    format!("{base}__{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn program_of(src: &str) -> Program {
        let tokens = Lexer::tokenize(src);
        Parser::new_with_spans(tokens)
            .parse_program()
            .expect("program should parse")
    }

    #[test]
    fn recursive_generic_specialization_is_diagnosed() {
        // `f` instantiates itself at a strictly larger pointer type, so the
        // specialization set is infinite; the cap turns the hang into an error.
        let src = "\
f: <T>(x: T) -> i32 {
    return f<T*>(x)
}

main: () -> i32 {
    return f<i32>(0)
}
";
        let err = monomorphize_program(&program_of(src)).unwrap_err();
        assert!(
            err.contains("did not terminate"),
            "expected a non-termination diagnostic, got: {err}"
        );
    }

    #[test]
    fn bounded_generic_specialization_succeeds() {
        let src = "\
id: <T>(x: T) -> T {
    return x
}

main: () -> i32 {
    return id<i32>(42)
}
";
        assert!(monomorphize_program(&program_of(src)).is_ok());
    }
}
