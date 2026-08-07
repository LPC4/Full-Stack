//! Non-capturing lambda enforcement: a lambda may use only its parameters, its
//! own locals, and module-level names; enclosing locals are capture errors.

use crate::ast::{
    AssignTarget, Block, DeclNode, Expression, ForIter, PrimaryExpr, Program, Statement,
};
use std::collections::{HashMap, HashSet};

const LAMBDA_PREFIX: &str = "__hll_lambda_";

// Names callable without a declaration: primitive cast forms like `i32(x)`.
const PRIMITIVE_NAMES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool",
];

pub(crate) fn check_lambda_captures(program: &Program) -> Result<(), String> {
    let mut top_level = HashSet::new();
    for declaration in &program.declarations {
        match &declaration.decl {
            DeclNode::Variable { name, .. }
            | DeclNode::InferredVariable { name, .. }
            | DeclNode::Function { name, .. }
            | DeclNode::Type { name, .. }
            | DeclNode::Struct { name, .. }
            | DeclNode::Const { name, .. }
            | DeclNode::Interface { name, .. }
            | DeclNode::ModuleImport { alias: name, .. } => {
                top_level.insert(name.clone());
            }
            DeclNode::Enum { name, variants, .. } => {
                top_level.insert(name.clone());
                for variant in variants {
                    top_level.insert(variant.name.clone());
                }
            }
            // Items are reached through their owner type, never bare.
            DeclNode::AssociatedItems { .. } => {}
        }
    }

    // Free names of each lambda: references minus its parameters, its own
    // locals, module-level names, and primitive cast forms.
    let mut lambda_free: HashMap<&str, Vec<String>> = HashMap::new();
    for declaration in &program.declarations {
        let DeclNode::Function {
            name,
            params,
            body: Some(body),
            ..
        } = &declaration.decl
        else {
            continue;
        };
        if !name.starts_with(LAMBDA_PREFIX) {
            continue;
        }
        let mut refs = HashSet::new();
        crate::imports::collect_block_refs(body, &mut refs);
        let mut binders = HashSet::new();
        collect_block_binders(body, &mut binders);
        let mut free = refs
            .into_iter()
            .filter(|reference| {
                !binders.contains(reference)
                    && !top_level.contains(reference)
                    && !PRIMITIVE_NAMES.contains(&reference.as_str())
                    && params.iter().all(|param| &param.name != reference)
            })
            .collect::<Vec<_>>();
        free.sort();
        lambda_free.insert(name, free);
    }
    if lambda_free.is_empty() {
        return Ok(());
    }

    // A free name that is a local of the function referencing the lambda is a
    // capture; anything else stays for ordinary unknown-name diagnostics.
    for declaration in &program.declarations {
        let DeclNode::Function {
            name,
            params,
            body: Some(body),
            ..
        } = &declaration.decl
        else {
            continue;
        };
        let mut refs = HashSet::new();
        crate::imports::collect_block_refs(body, &mut refs);
        let mut lambdas = refs
            .iter()
            .filter(|reference| reference.starts_with(LAMBDA_PREFIX))
            .collect::<Vec<_>>();
        if lambdas.is_empty() {
            continue;
        }
        lambdas.sort();
        let mut scope = HashSet::new();
        for param in params {
            scope.insert(param.name.clone());
        }
        collect_block_binders(body, &mut scope);
        for lambda in lambdas {
            let Some(free) = lambda_free.get(lambda.as_str()) else {
                continue;
            };
            if let Some(captured) = free.iter().find(|free_name| scope.contains(*free_name)) {
                return Err(format!(
                    "lambda in `{name}` captures `{captured}`; non-capturing lambdas may \
                     only use their parameters and module-level declarations"
                ));
            }
        }
    }
    Ok(())
}

// Every name a block introduces, ignoring shadowing and declaration order;
// use-before-declaration is left to ordinary scope diagnostics.
fn collect_block_binders(block: &Block, binders: &mut HashSet<String>) {
    for statement in &block.statements {
        collect_statement_binders(statement, binders);
    }
}

fn collect_statement_binders(statement: &Statement, binders: &mut HashSet<String>) {
    match statement {
        Statement::VariableDecl { name, init, .. } => {
            binders.insert(name.clone());
            if let Some(init) = init {
                collect_expression_binders(init, binders);
            }
        }
        Statement::InferredVariableDecl { name, init } => {
            binders.insert(name.clone());
            collect_expression_binders(init, binders);
        }
        Statement::DestructureBinding { fields, init } => {
            for field in fields {
                binders.insert(field.binding.clone());
            }
            collect_expression_binders(init, binders);
        }
        Statement::For { var, iter, body } => {
            binders.insert(var.clone());
            match iter {
                ForIter::Range { start, end, .. } => {
                    collect_expression_binders(start, binders);
                    collect_expression_binders(end, binders);
                }
                ForIter::Each(expr) => collect_expression_binders(expr, binders),
            }
            collect_block_binders(body, binders);
        }
        Statement::Expression(expr) | Statement::Defer(expr) | Statement::Return(Some(expr)) => {
            collect_expression_binders(expr, binders);
        }
        Statement::Block(block) => collect_block_binders(block, binders),
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            collect_expression_binders(cond, binders);
            collect_block_binders(then_block, binders);
            if let Some(else_branch) = else_branch {
                collect_statement_binders(else_branch, binders);
            }
        }
        Statement::While { cond, body } => {
            collect_expression_binders(cond, binders);
            collect_block_binders(body, binders);
        }
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
}

fn collect_expression_binders(expr: &Expression, binders: &mut HashSet<String>) {
    match expr {
        Expression::Assignment { target, rvalue } => {
            collect_target_binders(target, binders);
            collect_expression_binders(rvalue, binders);
        }
        Expression::Binary { left, right, .. } => {
            collect_expression_binders(left, binders);
            collect_expression_binders(right, binders);
        }
        Expression::Unary { expr, .. } | Expression::Try(expr) | Expression::Cast { expr, .. } => {
            collect_expression_binders(expr, binders);
        }
        Expression::Match { scrutinee, arms } => {
            collect_expression_binders(scrutinee, binders);
            for arm in arms {
                collect_pattern_binders(&arm.pattern, binders);
                collect_block_binders(&arm.body, binders);
                if let Some(value) = &arm.value {
                    collect_expression_binders(value, binders);
                }
            }
        }
        Expression::Primary(primary) => collect_primary_binders(primary, binders),
    }
}

fn collect_target_binders(target: &AssignTarget, binders: &mut HashSet<String>) {
    match target {
        // Typed destructuring may declare new locals; count every named field.
        AssignTarget::StructDestructure(fields) => {
            for field in fields {
                if let Some(name) = &field.name {
                    binders.insert(name.clone());
                }
            }
        }
        AssignTarget::Dereference(inner) => collect_expression_binders(inner, binders),
        AssignTarget::FieldAccess { expr: inner, .. } => {
            collect_target_binders(inner, binders);
        }
        AssignTarget::ArrayIndex { expr, index } => {
            collect_target_binders(expr, binders);
            collect_expression_binders(index, binders);
        }
        AssignTarget::Identifier(_) => {}
    }
}

fn collect_pattern_binders(pattern: &crate::ast::Pattern, binders: &mut HashSet<String>) {
    match pattern {
        crate::ast::Pattern::Binding(name) => {
            binders.insert(name.clone());
        }
        crate::ast::Pattern::Variant { bindings, .. } => {
            for binding in bindings {
                if binding != "_" {
                    binders.insert(binding.clone());
                }
            }
        }
        crate::ast::Pattern::Wildcard | crate::ast::Pattern::Literal(_) => {}
    }
}

fn collect_primary_binders(primary: &PrimaryExpr, binders: &mut HashSet<String>) {
    match primary {
        PrimaryExpr::Grouped(inner) => collect_expression_binders(inner, binders),
        PrimaryExpr::FunctionCall { arguments, .. } | PrimaryExpr::CallExpr { arguments, .. } => {
            for argument in arguments {
                collect_expression_binders(argument, binders);
            }
            if let PrimaryExpr::CallExpr { callee, .. } = primary {
                collect_expression_binders(callee, binders);
            }
        }
        PrimaryExpr::New { args, .. } => {
            for argument in args {
                collect_expression_binders(argument, binders);
            }
        }
        PrimaryExpr::ArrayLiteral(elements) => {
            for element in elements {
                collect_expression_binders(element, binders);
            }
        }
        PrimaryExpr::StructLiteral { fields, .. }
        | PrimaryExpr::NamedStructLiteral { fields, .. } => {
            for field in fields {
                collect_expression_binders(&field.expr, binders);
            }
        }
        PrimaryExpr::FieldAccess { expr, .. } => collect_expression_binders(expr, binders),
        PrimaryExpr::FormatInto { dest, segments, .. } => {
            collect_expression_binders(dest, binders);
            for segment in segments {
                if let crate::ast::FormatSegment::Hole(expr) = segment {
                    collect_expression_binders(expr, binders);
                }
            }
        }
        PrimaryExpr::ArrayIndex { expr, index } => {
            collect_expression_binders(expr, binders);
            collect_expression_binders(index, binders);
        }
        PrimaryExpr::Slice {
            expr, start, end, ..
        } => {
            collect_expression_binders(expr, binders);
            if let Some(start) = start {
                collect_expression_binders(start, binders);
            }
            if let Some(end) = end {
                collect_expression_binders(end, binders);
            }
        }
        PrimaryExpr::Identifier(_)
        | PrimaryExpr::Literal(_)
        | PrimaryExpr::AsmReg { .. }
        | PrimaryExpr::AssociatedConst { .. } => {}
    }
}
