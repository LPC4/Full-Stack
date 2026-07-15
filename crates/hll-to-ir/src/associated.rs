//! Associated constants and type aliases: collects impl `const`/`type` members
//! and resolves `Owner<args>.Member` projections before semantic analysis.

use crate::ast::{
    AssignTarget, Block, DeclNode, Expression, ForIter, Literal, PrimaryExpr, Program, Statement,
    Type,
};
use crate::monomorphize::{Substitutions, substitute_type};
use std::collections::{HashMap, HashSet};

// One owner's compile-time members. Generics are the impl block's parameters;
// every member of that block shares them.
struct OwnerItems {
    generics: Vec<String>,
    consts: HashMap<String, Expression>,
    aliases: HashMap<String, Type>,
}

#[derive(Default)]
pub(crate) struct AssociatedCatalog {
    owners: HashMap<String, OwnerItems>,
}

impl AssociatedCatalog {
    pub(crate) fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

/// Strip `AssociatedItems` declarations out of `program` into a catalog,
/// rejecting duplicate member names per owner.
pub(crate) fn collect(program: &mut Program) -> Result<AssociatedCatalog, String> {
    let mut owners: HashMap<String, OwnerItems> = HashMap::new();
    let declarations = std::mem::take(&mut program.declarations);
    for declaration in declarations {
        let DeclNode::AssociatedItems {
            owner,
            generics,
            consts,
            aliases,
        } = declaration.decl
        else {
            program.declarations.push(declaration);
            continue;
        };
        let entry = owners.entry(owner.clone()).or_insert_with(|| OwnerItems {
            generics: generics.clone(),
            consts: HashMap::new(),
            aliases: HashMap::new(),
        });
        // Merged blocks for one owner must agree on the generic owner pattern
        // so member substitution has a single parameter list.
        if entry.generics != generics {
            return Err(format!(
                "impl blocks for `{owner}` declare different generic parameters"
            ));
        }
        for (name, init) in consts {
            if entry.aliases.contains_key(&name)
                || entry.consts.insert(name.clone(), init).is_some()
            {
                return Err(format!("duplicate associated item `{owner}.{name}`"));
            }
        }
        for (name, ty) in aliases {
            if entry.consts.contains_key(&name) || entry.aliases.insert(name.clone(), ty).is_some()
            {
                return Err(format!("duplicate associated item `{owner}.{name}`"));
            }
        }
    }
    Ok(AssociatedCatalog { owners })
}

/// Resolve every projection; `skip_generics` leaves generic declarations for
/// monomorphize, which resolves their clones via [`resolve_declaration`].
pub(crate) fn resolve_program(
    program: &mut Program,
    catalog: &AssociatedCatalog,
    skip_generics: bool,
) -> Result<(), String> {
    for declaration in &mut program.declarations {
        if skip_generics && declaration_is_generic(&declaration.decl) {
            continue;
        }
        resolve_decl_node(&mut declaration.decl, catalog)?;
    }
    for statement in &mut program.statements {
        resolve_statement(statement, catalog)?;
    }
    Ok(())
}

/// Resolve one (already specialized, concrete) declaration.
pub(crate) fn resolve_declaration(
    declaration: &mut crate::ast::Declaration,
    catalog: &AssociatedCatalog,
) -> Result<(), String> {
    resolve_decl_node(&mut declaration.decl, catalog)
}

fn declaration_is_generic(decl: &DeclNode) -> bool {
    match decl {
        DeclNode::Function { generics, .. }
        | DeclNode::Type { generics, .. }
        | DeclNode::Struct { generics, .. }
        | DeclNode::Enum { generics, .. } => !generics.is_empty(),
        _ => false,
    }
}

fn resolve_decl_node(decl: &mut DeclNode, catalog: &AssociatedCatalog) -> Result<(), String> {
    match decl {
        DeclNode::Variable { ty, init, .. } => {
            resolve_type(ty, catalog)?;
            if let Some(init) = init {
                resolve_expression(init, catalog)?;
            }
        }
        DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
            resolve_expression(init, catalog)?;
        }
        DeclNode::Function {
            params,
            return_type,
            body,
            ..
        } => {
            for param in params {
                resolve_type(&mut param.ty, catalog)?;
            }
            if let Some(crate::ast::ReturnType::Single(ty)) = return_type {
                resolve_type(ty, catalog)?;
            }
            if let Some(body) = body {
                resolve_block(body, catalog)?;
            }
        }
        DeclNode::Type { ty, .. } => resolve_type(ty, catalog)?,
        DeclNode::Struct { fields, .. } => {
            for field in fields {
                resolve_type(&mut field.ty, catalog)?;
                if let Some(init) = &mut field.init {
                    resolve_expression(init, catalog)?;
                }
            }
        }
        DeclNode::Enum { variants, .. } => {
            for variant in variants {
                for payload in &mut variant.payload {
                    resolve_type(payload, catalog)?;
                }
            }
        }
        DeclNode::ModuleImport { .. }
        | DeclNode::Interface { .. }
        | DeclNode::AssociatedItems { .. } => {}
    }
    Ok(())
}

fn resolve_block(block: &mut Block, catalog: &AssociatedCatalog) -> Result<(), String> {
    for statement in &mut block.statements {
        resolve_statement(statement, catalog)?;
    }
    Ok(())
}

fn resolve_statement(statement: &mut Statement, catalog: &AssociatedCatalog) -> Result<(), String> {
    match statement {
        Statement::Expression(expr) | Statement::Defer(expr) | Statement::Return(Some(expr)) => {
            resolve_expression(expr, catalog)
        }
        Statement::Block(block) => resolve_block(block, catalog),
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            resolve_expression(cond, catalog)?;
            resolve_block(then_block, catalog)?;
            if let Some(else_branch) = else_branch {
                resolve_statement(else_branch, catalog)?;
            }
            Ok(())
        }
        Statement::While { cond, body } => {
            resolve_expression(cond, catalog)?;
            resolve_block(body, catalog)
        }
        Statement::For { iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, .. } => {
                    resolve_expression(start, catalog)?;
                    resolve_expression(end, catalog)?;
                }
                ForIter::Each(expr) => resolve_expression(expr, catalog)?,
            }
            resolve_block(body, catalog)
        }
        Statement::VariableDecl { ty, init, .. } => {
            resolve_type(ty, catalog)?;
            if let Some(init) = init {
                resolve_expression(init, catalog)?;
            }
            Ok(())
        }
        Statement::InferredVariableDecl { init, .. }
        | Statement::DestructureBinding { init, .. } => resolve_expression(init, catalog),
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => Ok(()),
    }
}

fn resolve_expression(expr: &mut Expression, catalog: &AssociatedCatalog) -> Result<(), String> {
    // Rewrite projections at this node first, then recurse into children.
    let replacement = match expr {
        Expression::Primary(PrimaryExpr::AssociatedConst {
            owner,
            owner_args,
            member,
        }) => {
            for arg in owner_args.iter_mut() {
                resolve_type(arg, catalog)?;
            }
            Some(resolve_const(owner, owner_args, member, catalog)?)
        }
        // `Config.MAX`: the zero-argument projection parses as a field access.
        // An alias member also matches so misuse gets the projection diagnostic.
        Expression::Primary(PrimaryExpr::FieldAccess { expr: base, field }) => {
            match base.as_ref() {
                Expression::Primary(PrimaryExpr::Identifier(owner))
                    if catalog.owners.get(owner).is_some_and(|items| {
                        items.consts.contains_key(field) || items.aliases.contains_key(field)
                    }) =>
                {
                    Some(resolve_const(owner, &[], field, catalog)?)
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(replacement) = replacement {
        *expr = replacement;
    }

    match expr {
        Expression::Assignment { target, rvalue } => {
            resolve_target(target, catalog)?;
            resolve_expression(rvalue, catalog)
        }
        Expression::Binary { left, right, .. } => {
            resolve_expression(left, catalog)?;
            resolve_expression(right, catalog)
        }
        Expression::Unary { expr, .. } | Expression::Try(expr) => resolve_expression(expr, catalog),
        Expression::Cast { target_ty, expr } => {
            resolve_type(target_ty, catalog)?;
            resolve_expression(expr, catalog)
        }
        Expression::Match { scrutinee, arms } => {
            resolve_expression(scrutinee, catalog)?;
            for arm in arms {
                resolve_block(&mut arm.body, catalog)?;
                if let Some(value) = &mut arm.value {
                    resolve_expression(value, catalog)?;
                }
            }
            Ok(())
        }
        Expression::Primary(primary) => resolve_primary(primary, catalog),
    }
}

fn resolve_target(target: &mut AssignTarget, catalog: &AssociatedCatalog) -> Result<(), String> {
    match target {
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            resolve_target(inner, catalog)
        }
        AssignTarget::ArrayIndex { expr, index } => {
            resolve_target(expr, catalog)?;
            resolve_expression(index, catalog)
        }
        AssignTarget::StructDestructure(fields) => {
            for field in fields {
                if let Some(ty) = &mut field.ty {
                    resolve_type(ty, catalog)?;
                }
            }
            Ok(())
        }
        AssignTarget::Identifier(_) => Ok(()),
    }
}

fn resolve_primary(primary: &mut PrimaryExpr, catalog: &AssociatedCatalog) -> Result<(), String> {
    match primary {
        PrimaryExpr::Grouped(inner) => resolve_expression(inner, catalog),
        PrimaryExpr::FunctionCall {
            type_arguments,
            arguments,
            ..
        } => {
            for ty in type_arguments {
                resolve_type(ty, catalog)?;
            }
            for argument in arguments {
                resolve_expression(argument, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::CallExpr { callee, arguments } => {
            resolve_expression(callee, catalog)?;
            for argument in arguments {
                resolve_expression(argument, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::New { ty, args } => {
            resolve_type(ty, catalog)?;
            for argument in args {
                resolve_expression(argument, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::ArrayLiteral(elements) => {
            for element in elements {
                resolve_expression(element, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::StructLiteral(fields) | PrimaryExpr::NamedStructLiteral { fields, .. } => {
            for field in fields {
                if let Some(ty) = &mut field.ty {
                    resolve_type(ty, catalog)?;
                }
                resolve_expression(&mut field.expr, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::FieldAccess { expr, .. } => resolve_expression(expr, catalog),
        PrimaryExpr::FormatInto { dest, segments, .. } => {
            resolve_expression(dest, catalog)?;
            for segment in segments {
                if let crate::ast::FormatSegment::Hole(expr) = segment {
                    resolve_expression(expr, catalog)?;
                }
            }
            Ok(())
        }
        PrimaryExpr::ArrayIndex { expr, index } => {
            resolve_expression(expr, catalog)?;
            resolve_expression(index, catalog)
        }
        PrimaryExpr::Slice {
            expr, start, end, ..
        } => {
            resolve_expression(expr, catalog)?;
            if let Some(start) = start {
                resolve_expression(start, catalog)?;
            }
            if let Some(end) = end {
                resolve_expression(end, catalog)?;
            }
            Ok(())
        }
        PrimaryExpr::AssociatedConst { .. } => {
            unreachable!("associated constants are rewritten before recursion")
        }
        PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => Ok(()),
    }
}

// --- Type projection ---

fn resolve_type(ty: &mut Type, catalog: &AssociatedCatalog) -> Result<(), String> {
    let mut visiting = HashSet::new();
    resolve_type_guarded(ty, catalog, &mut visiting)
}

fn resolve_type_guarded(
    ty: &mut Type,
    catalog: &AssociatedCatalog,
    visiting: &mut HashSet<String>,
) -> Result<(), String> {
    match ty {
        Type::Associated {
            owner,
            owner_args,
            member,
        } => {
            for arg in owner_args.iter_mut() {
                resolve_type_guarded(arg, catalog, visiting)?;
            }
            // Owner arguments naming generic parameters stay unresolved until
            // monomorphize substitutes them into the specialized clone.
            if owner_args
                .iter()
                .any(|arg| type_names_unknown(arg, catalog))
            {
                return Ok(());
            }
            let resolved = project_alias(owner, owner_args, member, catalog, visiting)?;
            *ty = resolved;
            Ok(())
        }
        // `Config.Row`: a zero-argument projection parses as a two-segment path.
        Type::Named { name, args } if args.is_empty() && name.segments.len() == 2 => {
            let owner = name.segments[0].clone();
            let member = name.segments[1].clone();
            if catalog.owners.contains_key(&owner) {
                *ty = project_alias(&owner, &[], &member, catalog, visiting)?;
            }
            Ok(())
        }
        Type::Named { args, .. } => {
            for arg in args {
                resolve_type_guarded(arg, catalog, visiting)?;
            }
            Ok(())
        }
        Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
            resolve_type_guarded(inner, catalog, visiting)
        }
        Type::Function {
            params,
            return_type,
        } => {
            for param in params {
                resolve_type_guarded(param, catalog, visiting)?;
            }
            if let Some(return_type) = return_type {
                resolve_type_guarded(return_type, catalog, visiting)?;
            }
            Ok(())
        }
        Type::Struct(fields) => {
            for field in fields {
                resolve_type_guarded(&mut field.ty, catalog, visiting)?;
            }
            Ok(())
        }
        Type::DynamicReference(_) | Type::Primitive(_) => Ok(()),
    }
}

// Whether a type mentions a bare name that is neither primitive-like nor a
// catalog owner; inside generic templates that is a generic parameter.
fn type_names_unknown(ty: &Type, catalog: &AssociatedCatalog) -> bool {
    match ty {
        Type::Named { name, args } => {
            (args.is_empty()
                && name.segments.len() == 1
                && name.segments[0]
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
                && !catalog.owners.contains_key(&name.segments[0]))
                || args.iter().any(|arg| type_names_unknown(arg, catalog))
        }
        Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
            type_names_unknown(inner, catalog)
        }
        Type::Function {
            params,
            return_type,
        } => {
            params
                .iter()
                .any(|param| type_names_unknown(param, catalog))
                || return_type
                    .as_deref()
                    .is_some_and(|ty| type_names_unknown(ty, catalog))
        }
        Type::Struct(fields) => fields
            .iter()
            .any(|field| type_names_unknown(&field.ty, catalog)),
        Type::Associated { owner_args, .. } => owner_args
            .iter()
            .any(|arg| type_names_unknown(arg, catalog)),
        Type::DynamicReference(_) | Type::Primitive(_) => false,
    }
}

fn project_alias(
    owner: &str,
    owner_args: &[Type],
    member: &str,
    catalog: &AssociatedCatalog,
    visiting: &mut HashSet<String>,
) -> Result<Type, String> {
    let items = catalog
        .owners
        .get(owner)
        .ok_or_else(|| format!("type `{owner}` has no associated items"))?;
    let target = items.aliases.get(member).ok_or_else(|| {
        if items.consts.contains_key(member) {
            format!("`{owner}.{member}` is an associated constant, not a type")
        } else {
            format!("type `{owner}` has no associated type `{member}`")
        }
    })?;
    let key = projection_key(owner, owner_args, member);
    if !visiting.insert(key.clone()) {
        return Err(format!(
            "associated type `{key}` is defined in terms of itself"
        ));
    }
    let substitutions = owner_substitutions(owner, owner_args, &items.generics, member)?;
    let mut resolved = substitute_type(target, &substitutions);
    resolve_type_guarded(&mut resolved, catalog, visiting)?;
    visiting.remove(&key);
    Ok(resolved)
}

fn owner_substitutions(
    owner: &str,
    owner_args: &[Type],
    generics: &[String],
    member: &str,
) -> Result<Substitutions, String> {
    if owner_args.len() != generics.len() {
        return Err(format!(
            "`{owner}.{member}` expects {} owner type argument(s), got {}",
            generics.len(),
            owner_args.len()
        ));
    }
    Ok(generics
        .iter()
        .cloned()
        .zip(owner_args.iter().cloned())
        .collect())
}

fn projection_key(owner: &str, owner_args: &[Type], member: &str) -> String {
    if owner_args.is_empty() {
        format!("{owner}.{member}")
    } else {
        let args = owner_args
            .iter()
            .map(|arg| format!("{arg:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{owner}<{args}>.{member}")
    }
}

// --- Constant projection ---

fn resolve_const(
    owner: &str,
    owner_args: &[Type],
    member: &str,
    catalog: &AssociatedCatalog,
) -> Result<Expression, String> {
    let mut visiting = HashSet::new();
    let expanded = expand_const(owner, owner_args, member, catalog, &mut visiting)?;
    Ok(Expression::Primary(PrimaryExpr::Grouped(Box::new(
        expanded,
    ))))
}

fn expand_const(
    owner: &str,
    owner_args: &[Type],
    member: &str,
    catalog: &AssociatedCatalog,
    visiting: &mut HashSet<String>,
) -> Result<Expression, String> {
    let items = catalog
        .owners
        .get(owner)
        .ok_or_else(|| format!("type `{owner}` has no associated items"))?;
    let init = items.consts.get(member).ok_or_else(|| {
        if items.aliases.contains_key(member) {
            format!("`{owner}.{member}` is an associated type, not a constant")
        } else {
            format!("type `{owner}` has no associated constant `{member}`")
        }
    })?;
    let key = projection_key(owner, owner_args, member);
    if !visiting.insert(key.clone()) {
        return Err(format!(
            "associated constant `{key}` is defined in terms of itself"
        ));
    }
    let substitutions = owner_substitutions(owner, owner_args, &items.generics, member)?;
    let expanded =
        expand_const_expression(init, owner, owner_args, &substitutions, catalog, visiting)
            .map_err(|error| format!("associated constant `{key}`: {error}"))?;
    visiting.remove(&key);
    Ok(expanded)
}

// Inline an initializer, expanding sibling and cross-owner references, while
// rejecting anything that is not a compile-time scalar expression.
fn expand_const_expression(
    expr: &Expression,
    owner: &str,
    owner_args: &[Type],
    substitutions: &Substitutions,
    catalog: &AssociatedCatalog,
    visiting: &mut HashSet<String>,
) -> Result<Expression, String> {
    match expr {
        Expression::Primary(PrimaryExpr::Literal(
            literal @ (Literal::Integer(_)
            | Literal::HexInteger(_)
            | Literal::Float(_)
            | Literal::Boolean(_)),
        )) => Ok(Expression::Primary(PrimaryExpr::Literal(literal.clone()))),
        Expression::Primary(PrimaryExpr::Grouped(inner)) => Ok(Expression::Primary(
            PrimaryExpr::Grouped(Box::new(expand_const_expression(
                inner,
                owner,
                owner_args,
                substitutions,
                catalog,
                visiting,
            )?)),
        )),
        Expression::Primary(PrimaryExpr::Identifier(sibling)) => {
            Ok(Expression::Primary(PrimaryExpr::Grouped(Box::new(
                expand_const(owner, owner_args, sibling, catalog, visiting)?,
            ))))
        }
        Expression::Primary(PrimaryExpr::AssociatedConst {
            owner: other,
            owner_args: other_args,
            member,
        }) => {
            let mut resolved_args = other_args.clone();
            for arg in &mut resolved_args {
                *arg = substitute_type(arg, substitutions);
            }
            Ok(Expression::Primary(PrimaryExpr::Grouped(Box::new(
                expand_const(other, &resolved_args, member, catalog, visiting)?,
            ))))
        }
        Expression::Unary {
            op: op @ (crate::ast::UnaryOp::Negate | crate::ast::UnaryOp::Not),
            expr,
        } => Ok(Expression::Unary {
            op: op.clone(),
            expr: Box::new(expand_const_expression(
                expr,
                owner,
                owner_args,
                substitutions,
                catalog,
                visiting,
            )?),
        }),
        Expression::Binary { op, left, right } => Ok(Expression::Binary {
            op: op.clone(),
            left: Box::new(expand_const_expression(
                left,
                owner,
                owner_args,
                substitutions,
                catalog,
                visiting,
            )?),
            right: Box::new(expand_const_expression(
                right,
                owner,
                owner_args,
                substitutions,
                catalog,
                visiting,
            )?),
        }),
        Expression::Cast { target_ty, expr } => {
            let target = substitute_type(target_ty, substitutions);
            if !matches!(target, Type::Primitive(_)) {
                return Err("casts in initializers must target a primitive type".to_owned());
            }
            Ok(Expression::Cast {
                target_ty: target,
                expr: Box::new(expand_const_expression(
                    expr,
                    owner,
                    owner_args,
                    substitutions,
                    catalog,
                    visiting,
                )?),
            })
        }
        _ => Err(
            "initializer must be a compile-time scalar expression (literals, arithmetic, \
             primitive casts, and other associated constants)"
                .to_owned(),
        ),
    }
}
