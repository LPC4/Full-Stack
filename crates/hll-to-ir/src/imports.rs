//! Interface extraction for `import` / `export` (see `_LANG_SPECIFICATIONS.md` 6.3).
//! Pure source-to-source helpers; import-name resolution lives in the host pipeline.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Expression, FieldDecl, ForIter, Literal, Parameter,
    PrimaryExpr, Program, ReturnType, Statement, StructDestructureField, Type, UnaryOp, Variant,
};
use crate::lexer::Lexer;
use crate::parser::Parser;

/// Parse `source` into an AST, surfacing lex/parse failures as a message.
fn parse_module(source: &str) -> Result<Program, String> {
    let token_spans = Lexer::tokenize(source);
    if let Some((crate::token::Token::Error(msg), _)) = token_spans
        .iter()
        .find(|(t, _)| matches!(t, crate::token::Token::Error(_)))
    {
        return Err(format!("lexer error: {msg}"));
    }
    Parser::new_with_spans(token_spans)
        .parse_program()
        .map_err(|e| e.to_string())
}

/// The names a module imports, in source order (its direct `import "name"` decls).
pub fn collect_imports(source: &str) -> Result<Vec<String>, String> {
    let program = parse_module(source)?;
    Ok(program
        .declarations
        .iter()
        .filter_map(|d| match &d.decl {
            DeclNode::Import { path } => Some(path.clone()),
            _ => None,
        })
        .collect())
}

/// The qualified module bindings a unit declares, as `(alias, path)` in source order
/// (its `alias := import("path")` / `const alias = import("path")` decls).
pub fn collect_module_imports(source: &str) -> Result<Vec<(String, String)>, String> {
    let program = parse_module(source)?;
    Ok(program
        .declarations
        .iter()
        .filter_map(|d| match &d.decl {
            DeclNode::ModuleImport { alias, path } => Some((alias.clone(), path.clone())),
            _ => None,
        })
        .collect())
}

/// The names a module exports (its `export`ed declarations), for qualified-access
/// visibility checks. Re-exported `external` declarations still count as exports.
pub fn collect_exports(source: &str) -> Result<HashSet<String>, String> {
    let program = parse_module(source)?;
    Ok(program
        .declarations
        .iter()
        .filter(|d| d.exported)
        .filter_map(|d| decl_name(&d.decl).map(str::to_owned))
        .collect())
}

/// The top-level declaration names in a module, including module-import aliases.
pub fn collect_declaration_names(source: &str) -> Result<HashSet<String>, String> {
    let program = parse_module(source)?;
    Ok(program
        .declarations
        .iter()
        .filter_map(|d| decl_name(&d.decl).map(str::to_owned))
        .collect())
}

/// The declared name of a declaration, if it has one.
fn decl_name(decl: &DeclNode) -> Option<&str> {
    match decl {
        DeclNode::Variable { name, .. }
        | DeclNode::InferredVariable { name, .. }
        | DeclNode::Function { name, .. }
        | DeclNode::Type { name, .. }
        | DeclNode::Struct { name, .. }
        | DeclNode::Enum { name, .. }
        | DeclNode::Const { name, .. }
        | DeclNode::ModuleImport { alias: name, .. } => Some(name),
        DeclNode::Import { .. } => None,
    }
}

// --- Qualified-access rewrite (see `_LANG_SPECIFICATIONS.md` 6.3) ---

/// Rewrite `alias.member` / `alias.member(args)` to the flat export reference, validating each
/// member against `aliases` (alias -> exported names). See `_LANG_SPECIFICATIONS.md` 6.3.
pub fn rewrite_qualified_access(
    program: &mut Program,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    if aliases.is_empty() {
        return Ok(());
    }
    for decl in &mut program.declarations {
        match &mut decl.decl {
            DeclNode::Function {
                params,
                return_type,
                body,
                ..
            } => {
                for param in params {
                    rewrite_type(&mut param.ty, aliases)?;
                }
                if let Some(ReturnType::Single(ty)) = return_type {
                    rewrite_type(ty, aliases)?;
                }
                if let Some(body) = body {
                    rewrite_block(body, aliases)?;
                }
            }
            DeclNode::Variable { ty, init, .. } => {
                rewrite_type(ty, aliases)?;
                if let Some(init) = init {
                    rewrite_expr(init, aliases)?;
                }
            }
            DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
                rewrite_expr(init, aliases)?;
            }
            DeclNode::Type { ty, .. } => rewrite_type(ty, aliases)?,
            DeclNode::Struct { fields, .. } => rewrite_fields(fields, aliases)?,
            DeclNode::Enum { variants, .. } => {
                for variant in variants {
                    for payload in &mut variant.payload {
                        rewrite_type(payload, aliases)?;
                    }
                }
            }
            _ => {}
        }
    }
    for stmt in &mut program.statements {
        rewrite_stmt(stmt, aliases)?;
    }
    Ok(())
}

fn rewrite_type(ty: &mut Type, aliases: &HashMap<String, HashSet<String>>) -> Result<(), String> {
    match ty {
        Type::Primitive(_) => {}
        Type::Pointer(inner) | Type::Array(_, inner) | Type::Slice(inner) => {
            rewrite_type(inner, aliases)?;
        }
        Type::Function {
            params,
            return_type,
        } => {
            for param in params {
                rewrite_type(param, aliases)?;
            }
            if let Some(return_type) = return_type {
                rewrite_type(return_type, aliases)?;
            }
        }
        Type::Struct(fields) => rewrite_fields(fields, aliases)?,
        Type::Named { name, args } => {
            if let Some((base, field)) = name.split_once('.')
                && aliases.contains_key(base)
            {
                *name = resolve_member(aliases, base, field)?;
            }
            for arg in args {
                rewrite_type(arg, aliases)?;
            }
        }
    }
    Ok(())
}

fn rewrite_fields(
    fields: &mut [FieldDecl],
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    for field in fields {
        rewrite_type(&mut field.ty, aliases)?;
        if let Some(init) = &mut field.init {
            rewrite_expr(init, aliases)?;
        }
    }
    Ok(())
}

fn rewrite_destructure_fields(
    fields: &mut [StructDestructureField],
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    for field in fields {
        if let Some(ty) = &mut field.ty {
            rewrite_type(ty, aliases)?;
        }
    }
    Ok(())
}

fn resolve_member(
    aliases: &HashMap<String, HashSet<String>>,
    base: &str,
    field: &str,
) -> Result<String, String> {
    match aliases.get(base) {
        Some(exports) if exports.contains(field) => Ok(field.to_owned()),
        Some(_) => Err(format!("module `{base}` has no exported member `{field}`")),
        None => unreachable!("resolve_member called for a non-alias base"),
    }
}

/// If `callee` is `alias.field` (a module-qualified path), return `(alias, field)`.
fn qualified_path(callee: &Expression) -> Option<(String, String)> {
    if let Expression::Primary(PrimaryExpr::FieldAccess { expr, field }) = callee
        && let Expression::Primary(PrimaryExpr::Identifier(base)) = expr.as_ref()
    {
        return Some((base.clone(), field.clone()));
    }
    None
}

fn rewrite_expr(
    expr: &mut Expression,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    // Replace a qualified call or field access at this node, then recurse into the
    // (possibly rewritten) children.
    let replacement = match expr {
        Expression::Primary(PrimaryExpr::CallExpr { callee, arguments }) => {
            match qualified_path(callee) {
                Some((base, field)) if aliases.contains_key(&base) => {
                    let name = resolve_member(aliases, &base, &field)?;
                    Some(Expression::Primary(PrimaryExpr::FunctionCall {
                        name,
                        type_arguments: Vec::new(),
                        arguments: std::mem::take(arguments),
                    }))
                }
                _ => None,
            }
        }
        Expression::Primary(PrimaryExpr::FieldAccess { expr: inner, field }) => {
            match inner.as_ref() {
                Expression::Primary(PrimaryExpr::Identifier(base))
                    if aliases.contains_key(base) =>
                {
                    let name = resolve_member(aliases, base, field)?;
                    Some(Expression::Primary(PrimaryExpr::Identifier(name)))
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(rewritten) = replacement {
        *expr = rewritten;
    }

    rewrite_expr_children(expr, aliases)
}

fn rewrite_expr_children(
    expr: &mut Expression,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    match expr {
        Expression::Assignment { target, rvalue } => {
            rewrite_assign_target(target, aliases)?;
            rewrite_expr(rvalue, aliases)?;
        }
        Expression::Binary { left, right, .. } => {
            rewrite_expr(left, aliases)?;
            rewrite_expr(right, aliases)?;
        }
        Expression::Unary { expr, .. } | Expression::Try(expr) => {
            rewrite_expr(expr, aliases)?;
        }
        Expression::Cast { expr, target_ty } => {
            rewrite_expr(expr, aliases)?;
            rewrite_type(target_ty, aliases)?;
        }
        Expression::Match { scrutinee, arms } => {
            rewrite_expr(scrutinee, aliases)?;
            for arm in arms {
                rewrite_block(&mut arm.body, aliases)?;
                if let Some(value) = &mut arm.value {
                    rewrite_expr(value, aliases)?;
                }
            }
        }
        Expression::Primary(primary) => rewrite_primary_children(primary, aliases)?,
    }
    Ok(())
}

fn rewrite_primary_children(
    primary: &mut PrimaryExpr,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    match primary {
        PrimaryExpr::Grouped(inner) => rewrite_expr(inner, aliases)?,
        PrimaryExpr::FunctionCall {
            type_arguments,
            arguments,
            ..
        } => {
            for ty in type_arguments {
                rewrite_type(ty, aliases)?;
            }
            for arg in arguments {
                rewrite_expr(arg, aliases)?;
            }
        }
        PrimaryExpr::New { ty, args } => {
            rewrite_type(ty, aliases)?;
            for arg in args {
                rewrite_expr(arg, aliases)?;
            }
        }
        PrimaryExpr::CallExpr { callee, arguments } => {
            rewrite_expr(callee, aliases)?;
            for arg in arguments {
                rewrite_expr(arg, aliases)?;
            }
        }
        PrimaryExpr::ArrayLiteral(elements) => {
            for element in elements {
                rewrite_expr(element, aliases)?;
            }
        }
        PrimaryExpr::StructLiteral(fields) => {
            for field in fields {
                rewrite_expr(&mut field.expr, aliases)?;
            }
        }
        PrimaryExpr::NamedStructLiteral { fields, .. } => {
            for field in fields {
                rewrite_expr(&mut field.expr, aliases)?;
            }
        }
        PrimaryExpr::FieldAccess { expr, .. } => rewrite_expr(expr, aliases)?,
        PrimaryExpr::ArrayIndex { expr, index } => {
            rewrite_expr(expr, aliases)?;
            rewrite_expr(index, aliases)?;
        }
        PrimaryExpr::Slice {
            expr, start, end, ..
        } => {
            rewrite_expr(expr, aliases)?;
            if let Some(start) = start {
                rewrite_expr(start, aliases)?;
            }
            if let Some(end) = end {
                rewrite_expr(end, aliases)?;
            }
        }
        PrimaryExpr::Identifier(_) | PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
    }
    Ok(())
}

/// Rewrite a write to an exported global (`alias.global = ...`) to the flat target; unknown
/// members fall through to downstream resolution and nested index expressions are recursed.
fn rewrite_assign_target(
    target: &mut AssignTarget,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    if let AssignTarget::FieldAccess { expr, field } = target
        && let AssignTarget::Identifier(base) = expr.as_ref()
        && aliases.get(base).is_some_and(|e| e.contains(field))
    {
        *target = AssignTarget::Identifier(field.clone());
        return Ok(());
    }
    match target {
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            rewrite_assign_target(inner, aliases)?;
        }
        AssignTarget::ArrayIndex { expr, index } => {
            rewrite_assign_target(expr, aliases)?;
            rewrite_expr(index, aliases)?;
        }
        AssignTarget::StructDestructure(fields) => rewrite_destructure_fields(fields, aliases)?,
        AssignTarget::Identifier(_) => {}
    }
    Ok(())
}

fn rewrite_stmt(
    stmt: &mut Statement,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    match stmt {
        Statement::Expression(expr) | Statement::Defer(expr) => rewrite_expr(expr, aliases)?,
        Statement::Block(block) => rewrite_block(block, aliases)?,
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            rewrite_expr(cond, aliases)?;
            rewrite_block(then_block, aliases)?;
            if let Some(else_stmt) = else_branch {
                rewrite_stmt(else_stmt, aliases)?;
            }
        }
        Statement::While { cond, body } => {
            rewrite_expr(cond, aliases)?;
            rewrite_block(body, aliases)?;
        }
        Statement::For { iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, .. } => {
                    rewrite_expr(start, aliases)?;
                    rewrite_expr(end, aliases)?;
                }
                ForIter::Each(expr) => rewrite_expr(expr, aliases)?,
            }
            rewrite_block(body, aliases)?;
        }
        Statement::Return(Some(expr)) => rewrite_expr(expr, aliases)?,
        Statement::VariableDecl { ty, init, .. } => {
            rewrite_type(ty, aliases)?;
            if let Some(init) = init {
                rewrite_expr(init, aliases)?;
            }
        }
        Statement::InferredVariableDecl { init, .. } => rewrite_expr(init, aliases)?,
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
    Ok(())
}

fn rewrite_block(
    block: &mut Block,
    aliases: &HashMap<String, HashSet<String>>,
) -> Result<(), String> {
    for stmt in &mut block.statements {
        rewrite_stmt(stmt, aliases)?;
    }
    Ok(())
}

/// Render `source`'s exported interface as HLL suitable for prepending to an importer:
/// `type`/`const`/`struct`/`enum` verbatim, `fn`/global as `external`, the rest omitted.
pub fn extract_interface(source: &str) -> Result<String, String> {
    let program = parse_module(source)?;
    let mut out = String::new();
    for decl in &program.declarations {
        if !decl.exported {
            continue;
        }
        if let Some(rendered) = render_export(&decl.decl)? {
            out.push_str(&rendered);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Render one exported declaration as an interface line, or `None` if it contributes
/// nothing an importer can use (e.g. a generic function, which cannot be `external`).
fn render_export(decl: &DeclNode) -> Result<Option<String>, String> {
    let rendered = match decl {
        DeclNode::Type { name, generics, ty } => {
            format!(
                "type {}{} = {}",
                name,
                render_generics(generics),
                render_type(ty)?
            )
        }
        DeclNode::Struct {
            name,
            generics,
            fields,
        } => {
            format!(
                "struct {}{} {{ {} }}",
                name,
                render_generics(generics),
                render_fields(fields)?
            )
        }
        DeclNode::Enum {
            name,
            generics,
            variants,
        } => {
            format!(
                "enum {}{} {{ {} }}",
                name,
                render_generics(generics),
                render_variants(variants)?
            )
        }
        DeclNode::Const { name, init } => {
            format!("const {} = {}", name, render_expr(init)?)
        }
        DeclNode::Function {
            name,
            generics,
            params,
            return_type,
            ..
        } => {
            // A generic function has no single object symbol to link against, so it
            // cannot be exported as an `external`; the importer must define it locally.
            if !generics.is_empty() {
                return Ok(None);
            }
            format!(
                "external {}: ({}){} ; @import-interface",
                name,
                render_params(params)?,
                render_return(return_type.as_ref())?
            )
        }
        // An exported global becomes an `external` reference; an inferred global has no
        // written type, so it cannot be expressed as a header and is skipped.
        DeclNode::Variable {
            name,
            ty,
            is_extern,
            ..
        } => {
            // The module's own re-exported `external` is already a link reference, not a
            // definition; do not duplicate it into the importer.
            if *is_extern {
                return Ok(None);
            }
            format!("external {}: {}", name, render_type(ty)?)
        }
        DeclNode::InferredVariable { .. }
        | DeclNode::Import { .. }
        | DeclNode::ModuleImport { .. } => return Ok(None),
    };
    Ok(Some(rendered))
}

fn render_generics(generics: &[String]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
    }
}

fn render_fields(fields: &[FieldDecl]) -> Result<String, String> {
    let mut parts = Vec::with_capacity(fields.len());
    for field in fields {
        let mut part = format!("{}: {}", field.name, render_type(&field.ty)?);
        if let Some(init) = &field.init {
            part.push_str(&format!(" = {}", render_expr(init)?));
        }
        parts.push(part);
    }
    Ok(parts.join(", "))
}

fn render_variants(variants: &[Variant]) -> Result<String, String> {
    let mut parts = Vec::with_capacity(variants.len());
    for variant in variants {
        if variant.payload.is_empty() {
            parts.push(variant.name.clone());
        } else {
            let payload = variant
                .payload
                .iter()
                .map(render_type)
                .collect::<Result<Vec<_>, _>>()?;
            parts.push(format!("{}({})", variant.name, payload.join(", ")));
        }
    }
    Ok(parts.join(", "))
}

fn render_params(params: &[Parameter]) -> Result<String, String> {
    let mut parts = Vec::with_capacity(params.len());
    for param in params {
        parts.push(format!("{}: {}", param.name, render_type(&param.ty)?));
    }
    Ok(parts.join(", "))
}

fn render_return(ret: Option<&ReturnType>) -> Result<String, String> {
    match ret {
        None => Ok(String::new()),
        Some(ReturnType::Single(ty)) => Ok(format!(" -> {}", render_type(ty)?)),
    }
}

fn render_type(ty: &Type) -> Result<String, String> {
    Ok(match ty {
        Type::Primitive(name) => name.clone(),
        Type::Pointer(inner) => format!("{}*", render_type(inner)?),
        Type::Array(size, inner) => format!("{}[{}]", render_type(inner)?, size),
        Type::Slice(inner) => format!("{}[]", render_type(inner)?),
        Type::Function {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .map(render_type)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            if let Some(return_type) = return_type {
                format!("fn({params}) -> {}", render_type(return_type)?)
            } else {
                format!("fn({params})")
            }
        }
        Type::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let args = args
                    .iter()
                    .map(render_type)
                    .collect::<Result<Vec<_>, _>>()?;
                format!("{}<{}>", name, args.join(", "))
            }
        }
        Type::Struct(fields) => format!("{{ {} }}", render_fields(fields)?),
    })
}

/// Render the constrained expression forms used in exported `const` and field inits.
/// Anything outside this subset is an extraction error, not a silently wrong header.
fn render_expr(expr: &Expression) -> Result<String, String> {
    Ok(match expr {
        Expression::Binary { op, left, right } => {
            format!(
                "{} {} {}",
                render_expr(left)?,
                binary_op(op),
                render_expr(right)?
            )
        }
        Expression::Unary { op, expr } => format!("{}{}", unary_op(op), render_expr(expr)?),
        Expression::Cast { target_ty, expr } => {
            format!("{} as {}", render_expr(expr)?, render_type(target_ty)?)
        }
        Expression::Primary(primary) => render_primary(primary)?,
        Expression::Assignment { .. } | Expression::Match { .. } | Expression::Try(_) => {
            return Err("unsupported expression in exported interface".to_owned());
        }
    })
}

fn render_primary(primary: &PrimaryExpr) -> Result<String, String> {
    Ok(match primary {
        PrimaryExpr::Identifier(name) => name.clone(),
        PrimaryExpr::Literal(lit) => render_literal(lit),
        PrimaryExpr::Grouped(inner) => format!("({})", render_expr(inner)?),
        _ => return Err("unsupported expression in exported interface".to_owned()),
    })
}

fn render_literal(lit: &Literal) -> String {
    match lit {
        Literal::Integer(value) => value.to_string(),
        Literal::HexInteger(value) => format!("0x{value:x}"),
        Literal::Float(value) => format!("{value:?}"),
        Literal::Boolean(value) => value.to_string(),
        Literal::Null => "null".to_owned(),
        Literal::String(value) => format!("{value:?}"),
    }
}

fn binary_op(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Eq => "==",
        BinaryOp::Neq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Lte => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Gte => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitwiseAnd => "&",
        BinaryOp::BitwiseXor => "^",
        BinaryOp::BitwiseOr => "|",
    }
}

fn unary_op(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "!",
        UnaryOp::AddressOf => "&",
        UnaryOp::Dereference => "@",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(entries: &[(&str, &[&str])]) -> HashMap<String, HashSet<String>> {
        entries
            .iter()
            .map(|(alias, exports)| {
                (
                    (*alias).to_owned(),
                    exports.iter().map(|e| (*e).to_owned()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn collects_module_imports_and_exports() {
        let source = r#"
math := import("./math.hll")
const http = import("http")
export const ZERO = 0
export add: (a: i32, b: i32) -> i32 { return a + b }
secret: i32 = 7
"#;
        assert_eq!(
            collect_module_imports(source).unwrap(),
            vec![
                ("math".to_owned(), "./math.hll".to_owned()),
                ("http".to_owned(), "http".to_owned()),
            ]
        );
        let exports = collect_exports(source).unwrap();
        assert!(exports.contains("ZERO"));
        assert!(exports.contains("add"));
        assert!(!exports.contains("secret"));
        // The importer's own module bindings are not exports.
        assert!(!exports.contains("math"));

        let names = collect_declaration_names(source).unwrap();
        assert!(names.contains("math"));
        assert!(names.contains("http"));
        assert!(names.contains("ZERO"));
        assert!(names.contains("add"));
        assert!(names.contains("secret"));
    }

    #[test]
    fn rewrites_qualified_call_and_const() {
        let mut program = parse_module(
            r#"
math := import("./math.hll")
main: () -> i32 {
    return math.add(math.ZERO, 2)
}
"#,
        )
        .unwrap();
        rewrite_qualified_access(&mut program, &aliases(&[("math", &["add", "ZERO"])])).unwrap();

        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected main function");
        };
        let Statement::Return(Some(Expression::Primary(PrimaryExpr::FunctionCall {
            name,
            arguments,
            ..
        }))) = &body.statements[0]
        else {
            panic!("qualified call should rewrite to a flat FunctionCall");
        };
        assert_eq!(name, "add");
        assert!(matches!(
            &arguments[0],
            Expression::Primary(PrimaryExpr::Identifier(n)) if n == "ZERO"
        ));
    }

    #[test]
    fn rejects_unknown_qualified_member() {
        let mut program = parse_module(
            r#"
math := import("./math.hll")
main: () -> i32 {
    return math.nope(1)
}
"#,
        )
        .unwrap();
        let err = rewrite_qualified_access(&mut program, &aliases(&[("math", &["add"])]))
            .expect_err("unknown member must fail");
        assert!(
            err.contains("module `math`") && err.contains("nope"),
            "{err}"
        );
    }

    #[test]
    fn collects_imports_in_source_order() {
        let source = r#"
import "as_layout"
import "as_object"
main: () -> i32 { return 0 }
"#;
        assert_eq!(
            collect_imports(source).unwrap(),
            vec!["as_layout".to_owned(), "as_object".to_owned()]
        );
    }

    #[test]
    fn extracts_only_exported_declarations() {
        let source = r#"
export type Word = u64
export const TF_BYTES = 288
export struct Reloc { sym: u8[16], off: i64, kind: i64 }
export write_object: () -> u64 { return 0 }
export count: i64 = 0
helper: () -> i32 { return 1 }
secret: i64 = 7
external puts: (s: u8*) -> i32
"#;
        let interface = extract_interface(source).unwrap();
        assert!(interface.contains("type Word = u64"));
        assert!(interface.contains("const TF_BYTES = 288"));
        assert!(interface.contains("struct Reloc { sym: u8[16], off: i64, kind: i64 }"));
        // Exported function and global become `external` signatures (no body).
        assert!(interface.contains("external write_object: () -> u64"));
        assert!(interface.contains("external count: i64"));
        // Private declarations and re-declared externs are omitted.
        assert!(!interface.contains("helper"));
        assert!(!interface.contains("secret"));
        assert!(!interface.contains("puts"));
        // No function body leaks into the interface.
        assert!(!interface.contains("return"));
    }

    #[test]
    fn renders_pointer_and_named_generic_types() {
        let source = r#"
export labels: Label* = null
export head: Box<i32>* = null
export resize: (n: u64, items: Reloc[]) -> Reloc* { return null }
"#;
        let interface = extract_interface(source).unwrap();
        assert!(interface.contains("external labels: Label*"), "{interface}");
        assert!(
            interface.contains("external head: Box<i32>*"),
            "{interface}"
        );
        assert!(
            interface.contains("external resize: (n: u64, items: Reloc[]) -> Reloc*"),
            "{interface}"
        );
    }

    #[test]
    fn skips_generic_function_export() {
        let source = r#"
export identity: <T>(value: T) -> T { return value }
"#;
        assert_eq!(extract_interface(source).unwrap(), "");
    }

    #[test]
    fn extracted_interface_parses_again() {
        // The interface is itself valid HLL, so prepending it to an importer lexes
        // and parses cleanly.
        let source = r#"
export struct Reloc { sym: u8[16], off: i64, kind: i64 }
export const TF_BYTES = 288
export write_object: () -> u64 { return 0 }
export out: u8* = null
"#;
        let interface = extract_interface(source).unwrap();
        let program = parse_module(&interface).expect("interface should parse");
        assert_eq!(program.declarations.len(), 4);
    }
}
