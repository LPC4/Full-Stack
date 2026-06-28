//! Interface extraction for `import` / `export` (see `_LANG_SPECIFICATIONS.md` 6.3).
//! Pure source-to-source helpers; import-name resolution lives in the host pipeline.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Expression, FieldDecl, ForIter, Literal, Parameter,
    PrimaryExpr, Program, ReturnType, Statement, StructDestructureField, Type, UnaryOp, Variant,
};
use crate::lexer::Lexer;
use crate::parser::Parser;

/// A resolved module-import binding. `mangled` is the link-symbol subset of `exports` that
/// resolves to `prefix__name`; the rest (const/type/...) fold and resolve to the flat name.
#[derive(Debug, Clone, Default)]
pub struct ModuleAlias {
    pub prefix: String,
    pub exports: HashSet<String>,
    pub mangled: HashSet<String>,
    pub member_aliases: HashMap<String, String>,
}

/// `prefix__name`, or `name` verbatim when `prefix` is empty (no mangling).
fn mangled_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}__{name}")
    }
}

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

/// The exported names that become link symbols (non-extern, non-generic functions and
/// non-extern globals), i.e. the exports a closure build mangles to `prefix__name`.
pub fn collect_exported_link_symbols(source: &str) -> Result<HashSet<String>, String> {
    let program = parse_module(source)?;
    Ok(program
        .declarations
        .iter()
        .filter(|d| d.exported)
        .filter_map(|d| match &d.decl {
            DeclNode::Function {
                name,
                generics,
                is_extern: false,
                ..
            } if generics.is_empty() => Some(name.clone()),
            DeclNode::Variable {
                name,
                is_extern: false,
                ..
            } => Some(name.clone()),
            _ => None,
        })
        .collect())
}

/// Whether a module defines code to assemble (a function body or non-extern global). Header-only
/// modules (only consts/types, e.g. `layout`) produce no object and are skipped by the closure.
pub fn defines_code(source: &str) -> Result<bool, String> {
    let program = parse_module(source)?;
    Ok(program.declarations.iter().any(|d| {
        matches!(
            &d.decl,
            DeclNode::Function {
                body: Some(_),
                is_extern: false,
                ..
            } | DeclNode::Variable {
                is_extern: false,
                ..
            } | DeclNode::InferredVariable { .. }
        )
    }))
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
    }
}

// --- Bare-reference collection (cross-module qualified-access enforcement) ---

/// Every bare identifier and unqualified call name in `source` (qualified `alias.member` and
/// inline `asm` surface no member name); intersect with a module's link symbols to flag 6.3 leaks.
pub fn collect_bare_references(source: &str) -> Result<HashSet<String>, String> {
    let program = parse_module(source)?;
    let mut refs = HashSet::new();
    for decl in &program.declarations {
        match &decl.decl {
            DeclNode::Function {
                body: Some(body), ..
            } => collect_block_refs(body, &mut refs),
            DeclNode::Variable {
                init: Some(init), ..
            } => collect_expr_refs(init, &mut refs),
            DeclNode::InferredVariable { init, .. } | DeclNode::Const { init, .. } => {
                collect_expr_refs(init, &mut refs);
            }
            _ => {}
        }
    }
    for stmt in &program.statements {
        collect_stmt_refs(stmt, &mut refs);
    }
    Ok(refs)
}

fn collect_block_refs(block: &Block, refs: &mut HashSet<String>) {
    for stmt in &block.statements {
        collect_stmt_refs(stmt, refs);
    }
}

fn collect_stmt_refs(stmt: &Statement, refs: &mut HashSet<String>) {
    match stmt {
        Statement::Expression(expr) | Statement::Defer(expr) => collect_expr_refs(expr, refs),
        Statement::Block(block) => collect_block_refs(block, refs),
        Statement::If {
            cond,
            then_block,
            else_branch,
        } => {
            collect_expr_refs(cond, refs);
            collect_block_refs(then_block, refs);
            if let Some(else_stmt) = else_branch {
                collect_stmt_refs(else_stmt, refs);
            }
        }
        Statement::While { cond, body } => {
            collect_expr_refs(cond, refs);
            collect_block_refs(body, refs);
        }
        Statement::For { iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, .. } => {
                    collect_expr_refs(start, refs);
                    collect_expr_refs(end, refs);
                }
                ForIter::Each(expr) => collect_expr_refs(expr, refs),
            }
            collect_block_refs(body, refs);
        }
        Statement::Return(Some(expr)) => collect_expr_refs(expr, refs),
        Statement::VariableDecl { init, .. } => {
            if let Some(init) = init {
                collect_expr_refs(init, refs);
            }
        }
        Statement::InferredVariableDecl { init, .. } => collect_expr_refs(init, refs),
        // Inline asm is the qualified-access escape hatch: it names link symbols literally and
        // is not modelled as identifier nodes, so it never contributes a bare reference here.
        Statement::Return(None)
        | Statement::AsmBlock { .. }
        | Statement::Break
        | Statement::Continue => {}
    }
}

fn collect_expr_refs(expr: &Expression, refs: &mut HashSet<String>) {
    match expr {
        Expression::Assignment { target, rvalue } => {
            collect_assign_target_refs(target, refs);
            collect_expr_refs(rvalue, refs);
        }
        Expression::Binary { left, right, .. } => {
            collect_expr_refs(left, refs);
            collect_expr_refs(right, refs);
        }
        Expression::Unary { expr, .. } | Expression::Try(expr) | Expression::Cast { expr, .. } => {
            collect_expr_refs(expr, refs);
        }
        Expression::Match { scrutinee, arms } => {
            collect_expr_refs(scrutinee, refs);
            for arm in arms {
                collect_block_refs(&arm.body, refs);
                if let Some(value) = &arm.value {
                    collect_expr_refs(value, refs);
                }
            }
        }
        Expression::Primary(primary) => collect_primary_refs(primary, refs),
    }
}

fn collect_primary_refs(primary: &PrimaryExpr, refs: &mut HashSet<String>) {
    match primary {
        PrimaryExpr::Identifier(name) => {
            refs.insert(name.clone());
        }
        PrimaryExpr::FunctionCall {
            name, arguments, ..
        } => {
            refs.insert(name.clone());
            for arg in arguments {
                collect_expr_refs(arg, refs);
            }
        }
        PrimaryExpr::Grouped(inner) => collect_expr_refs(inner, refs),
        PrimaryExpr::New { args, .. } => {
            for arg in args {
                collect_expr_refs(arg, refs);
            }
        }
        PrimaryExpr::CallExpr { callee, arguments } => {
            collect_expr_refs(callee, refs);
            for arg in arguments {
                collect_expr_refs(arg, refs);
            }
        }
        PrimaryExpr::ArrayLiteral(elements) => {
            for element in elements {
                collect_expr_refs(element, refs);
            }
        }
        PrimaryExpr::StructLiteral(fields) => {
            for field in fields {
                collect_expr_refs(&field.expr, refs);
            }
        }
        PrimaryExpr::NamedStructLiteral { fields, .. } => {
            for field in fields {
                collect_expr_refs(&field.expr, refs);
            }
        }
        // The member of a qualified access is a field, not an identifier node, so only the
        // base (the import alias) is recursed into here.
        PrimaryExpr::FieldAccess { expr, .. } => collect_expr_refs(expr, refs),
        PrimaryExpr::ArrayIndex { expr, index } => {
            collect_expr_refs(expr, refs);
            collect_expr_refs(index, refs);
        }
        PrimaryExpr::Slice {
            expr, start, end, ..
        } => {
            collect_expr_refs(expr, refs);
            if let Some(start) = start {
                collect_expr_refs(start, refs);
            }
            if let Some(end) = end {
                collect_expr_refs(end, refs);
            }
        }
        PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
    }
}

fn collect_assign_target_refs(target: &AssignTarget, refs: &mut HashSet<String>) {
    match target {
        AssignTarget::Identifier(name) => {
            refs.insert(name.clone());
        }
        AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
            collect_assign_target_refs(inner, refs);
        }
        AssignTarget::ArrayIndex { expr, index } => {
            collect_assign_target_refs(expr, refs);
            collect_expr_refs(index, refs);
        }
        AssignTarget::StructDestructure(_) => {}
    }
}

// --- Qualified-access rewrite (see `_LANG_SPECIFICATIONS.md` 6.3) ---

/// Rewrite `alias.member` / `alias.member(args)` to the flat export reference, validating each
/// member against `aliases` (alias -> exported names). See `_LANG_SPECIFICATIONS.md` 6.3.
pub fn rewrite_qualified_access(
    program: &mut Program,
    aliases: &HashMap<String, ModuleAlias>,
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
            DeclNode::ModuleImport { .. } => {}
        }
    }
    for stmt in &mut program.statements {
        rewrite_stmt(stmt, aliases)?;
    }
    Ok(())
}

fn rewrite_type(ty: &mut Type, aliases: &HashMap<String, ModuleAlias>) -> Result<(), String> {
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
    aliases: &HashMap<String, ModuleAlias>,
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
    aliases: &HashMap<String, ModuleAlias>,
) -> Result<(), String> {
    for field in fields {
        if let Some(ty) = &mut field.ty {
            rewrite_type(ty, aliases)?;
        }
    }
    Ok(())
}

fn resolve_member(
    aliases: &HashMap<String, ModuleAlias>,
    base: &str,
    field: &str,
) -> Result<String, String> {
    match aliases.get(base) {
        Some(alias) => {
            let export = alias
                .member_aliases
                .get(field)
                .map(String::as_str)
                .unwrap_or(field);
            if alias.exports.contains(export) {
                if alias.mangled.contains(export) {
                    Ok(mangled_name(&alias.prefix, export))
                } else {
                    Ok(export.to_owned())
                }
            } else {
                Err(format!("module `{base}` has no exported member `{field}`"))
            }
        }
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
    aliases: &HashMap<String, ModuleAlias>,
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
    aliases: &HashMap<String, ModuleAlias>,
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
    aliases: &HashMap<String, ModuleAlias>,
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
    aliases: &HashMap<String, ModuleAlias>,
) -> Result<(), String> {
    if let AssignTarget::FieldAccess { expr, field } = target
        && let AssignTarget::Identifier(base) = expr.as_ref()
        && let Some(alias) = aliases.get(base)
    {
        let export = alias
            .member_aliases
            .get(field)
            .map(String::as_str)
            .unwrap_or(field);
        if alias.exports.contains(export) {
            let resolved = if alias.mangled.contains(export) {
                mangled_name(&alias.prefix, export)
            } else {
                export.to_owned()
            };
            *target = AssignTarget::Identifier(resolved);
            return Ok(());
        }
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
    aliases: &HashMap<String, ModuleAlias>,
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

fn rewrite_block(block: &mut Block, aliases: &HashMap<String, ModuleAlias>) -> Result<(), String> {
    for stmt in &mut block.statements {
        rewrite_stmt(stmt, aliases)?;
    }
    Ok(())
}

// --- Per-module symbol mangling (see `_LANG_SPECIFICATIONS.md` 6.3) ---

/// Rename a module's own top-level fn/global names (and internal refs) to `prefix__name`.
/// `external` decls, `exempt` names (entry points), types, and consts keep their name.
pub fn mangle_module(program: &mut Program, prefix: &str, exempt: &HashSet<String>) {
    if prefix.is_empty() {
        return;
    }
    let mut targets: HashSet<String> = HashSet::new();
    for decl in &program.declarations {
        match &decl.decl {
            DeclNode::Function {
                name,
                is_extern: false,
                ..
            }
            | DeclNode::Variable {
                name,
                is_extern: false,
                ..
            }
            | DeclNode::InferredVariable { name, .. } => {
                targets.insert(name.clone());
            }
            _ => {}
        }
    }
    for name in exempt {
        targets.remove(name);
    }
    if targets.is_empty() {
        return;
    }
    Mangler {
        prefix,
        targets,
        scopes: Vec::new(),
    }
    .run(program);
}

/// Walks an AST renaming references to a module's mangled top-level symbols, honoring local
/// bindings (params, locals, loop and match bindings) that shadow a top-level name.
struct Mangler<'a> {
    prefix: &'a str,
    targets: HashSet<String>,
    scopes: Vec<HashSet<String>>,
}

impl Mangler<'_> {
    fn run(&mut self, program: &mut Program) {
        for decl in &mut program.declarations {
            match &mut decl.decl {
                DeclNode::Function {
                    name,
                    params,
                    body,
                    is_extern: false,
                    ..
                } => {
                    self.rename_def(name);
                    self.scopes
                        .push(params.iter().map(|p| p.name.clone()).collect());
                    if let Some(body) = body {
                        self.block(body);
                    }
                    self.scopes.pop();
                }
                DeclNode::Variable {
                    name,
                    init,
                    is_extern: false,
                    ..
                } => {
                    self.rename_def(name);
                    if let Some(init) = init {
                        self.expr(init);
                    }
                }
                DeclNode::InferredVariable { name, init } => {
                    self.rename_def(name);
                    self.expr(init);
                }
                DeclNode::Const { init, .. } => self.expr(init),
                _ => {}
            }
        }
        self.scopes.push(HashSet::new());
        for stmt in &mut program.statements {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn rename_def(&self, name: &mut String) {
        if self.targets.contains(name) {
            *name = format!("{}__{}", self.prefix, name);
        }
    }

    fn mangle_ref(&self, name: &mut String) {
        if self.targets.contains(name) && !self.scopes.iter().any(|s| s.contains(name)) {
            *name = format!("{}__{}", self.prefix, name);
        }
    }

    fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_owned());
        }
    }

    fn block(&mut self, block: &mut Block) {
        self.scopes.push(HashSet::new());
        for stmt in &mut block.statements {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &mut Statement) {
        match stmt {
            Statement::Expression(expr) | Statement::Defer(expr) => self.expr(expr),
            Statement::Block(block) => self.block(block),
            Statement::If {
                cond,
                then_block,
                else_branch,
            } => {
                self.expr(cond);
                self.block(then_block);
                if let Some(else_stmt) = else_branch {
                    self.stmt(else_stmt);
                }
            }
            Statement::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            Statement::For { var, iter, body } => {
                match iter {
                    ForIter::Range { start, end, .. } => {
                        self.expr(start);
                        self.expr(end);
                    }
                    ForIter::Each(expr) => self.expr(expr),
                }
                self.scopes.push(HashSet::new());
                self.bind(var);
                for stmt in &mut body.statements {
                    self.stmt(stmt);
                }
                self.scopes.pop();
            }
            Statement::Return(Some(expr)) => self.expr(expr),
            Statement::VariableDecl { name, init, .. } => {
                if let Some(init) = init {
                    self.expr(init);
                }
                self.bind(name);
            }
            Statement::InferredVariableDecl { name, init } => {
                self.expr(init);
                self.bind(name);
            }
            Statement::Return(None)
            | Statement::AsmBlock { .. }
            | Statement::Break
            | Statement::Continue => {}
        }
    }

    fn expr(&mut self, expr: &mut Expression) {
        match expr {
            Expression::Assignment { target, rvalue } => {
                self.assign_target(target);
                self.expr(rvalue);
            }
            Expression::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expression::Unary { expr, .. } | Expression::Try(expr) => self.expr(expr),
            Expression::Cast { expr, .. } => self.expr(expr),
            Expression::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.scopes.push(pattern_bindings(&arm.pattern));
                    self.block(&mut arm.body);
                    if let Some(value) = &mut arm.value {
                        self.expr(value);
                    }
                    self.scopes.pop();
                }
            }
            Expression::Primary(primary) => self.primary(primary),
        }
    }

    fn primary(&mut self, primary: &mut PrimaryExpr) {
        match primary {
            PrimaryExpr::Identifier(name) => self.mangle_ref(name),
            PrimaryExpr::FunctionCall {
                name, arguments, ..
            } => {
                self.mangle_ref(name);
                for arg in arguments {
                    self.expr(arg);
                }
            }
            PrimaryExpr::CallExpr { callee, arguments } => {
                self.expr(callee);
                for arg in arguments {
                    self.expr(arg);
                }
            }
            PrimaryExpr::Grouped(inner) => self.expr(inner),
            PrimaryExpr::New { args, .. } => {
                for arg in args {
                    self.expr(arg);
                }
            }
            PrimaryExpr::ArrayLiteral(elements) => {
                for element in elements {
                    self.expr(element);
                }
            }
            PrimaryExpr::StructLiteral(fields) => {
                for field in fields {
                    self.expr(&mut field.expr);
                }
            }
            PrimaryExpr::NamedStructLiteral { fields, .. } => {
                for field in fields {
                    self.expr(&mut field.expr);
                }
            }
            PrimaryExpr::FieldAccess { expr, .. } => self.expr(expr),
            PrimaryExpr::ArrayIndex { expr, index } => {
                self.expr(expr);
                self.expr(index);
            }
            PrimaryExpr::Slice {
                expr, start, end, ..
            } => {
                self.expr(expr);
                if let Some(start) = start {
                    self.expr(start);
                }
                if let Some(end) = end {
                    self.expr(end);
                }
            }
            PrimaryExpr::Literal(_) | PrimaryExpr::AsmReg { .. } => {}
        }
    }

    fn assign_target(&mut self, target: &mut AssignTarget) {
        match target {
            AssignTarget::Identifier(name) => self.mangle_ref(name),
            AssignTarget::Dereference(inner) | AssignTarget::FieldAccess { expr: inner, .. } => {
                self.assign_target(inner);
            }
            AssignTarget::ArrayIndex { expr, index } => {
                self.assign_target(expr);
                self.expr(index);
            }
            AssignTarget::StructDestructure(_) => {}
        }
    }
}

/// The names a match pattern binds in its arm (catch-all binding or variant payload slots).
fn pattern_bindings(pattern: &crate::ast::Pattern) -> HashSet<String> {
    use crate::ast::Pattern;
    match pattern {
        Pattern::Binding(name) => HashSet::from([name.clone()]),
        Pattern::Variant { bindings, .. } => {
            bindings.iter().filter(|b| *b != "_").cloned().collect()
        }
        Pattern::Wildcard | Pattern::Literal(_) => HashSet::new(),
    }
}

/// Render `source`'s exported interface as HLL suitable for prepending to an importer:
/// `type`/`const`/`struct`/`enum` verbatim, `fn`/global as `external`, the rest omitted.
pub fn extract_interface(source: &str) -> Result<String, String> {
    extract_interface_prefixed(source, "")
}

/// As [`extract_interface`], but mangling exported `fn`/global names to `prefix__name` so an
/// importer's mangled references resolve to the target module's mangled definitions.
pub fn extract_interface_prefixed(source: &str, prefix: &str) -> Result<String, String> {
    let program = parse_module(source)?;
    let mut out = String::new();
    for decl in &program.declarations {
        if !decl.exported {
            continue;
        }
        if let Some(rendered) = render_export(&decl.decl, prefix)? {
            out.push_str(&rendered);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Render one exported declaration as an interface line, or `None` if it contributes
/// nothing an importer can use (e.g. a generic function, which cannot be `external`).
fn render_export(decl: &DeclNode, prefix: &str) -> Result<Option<String>, String> {
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
                mangled_name(prefix, name),
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
            format!(
                "external {}: {}",
                mangled_name(prefix, name),
                render_type(ty)?
            )
        }
        DeclNode::InferredVariable { .. } | DeclNode::ModuleImport { .. } => return Ok(None),
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

    fn aliases(entries: &[(&str, &[&str])]) -> HashMap<String, ModuleAlias> {
        prefixed_aliases(entries.iter().map(|(a, e)| ("", *a, *e)))
    }

    fn prefixed_aliases<'a>(
        entries: impl IntoIterator<Item = (&'a str, &'a str, &'a [&'a str])>,
    ) -> HashMap<String, ModuleAlias> {
        entries
            .into_iter()
            .map(|(prefix, alias, exports)| {
                let exports: HashSet<String> = exports.iter().map(|e| (*e).to_owned()).collect();
                (
                    alias.to_owned(),
                    ModuleAlias {
                        prefix: prefix.to_owned(),
                        mangled: exports.clone(),
                        member_aliases: HashMap::new(),
                        exports,
                    },
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
    fn rewrites_short_qualified_member_aliases() {
        let mut program = parse_module(
            r#"
klog := import("klog")
main: () {
    klog.ok("boot".ptr)
}
"#,
        )
        .unwrap();
        let mut map = HashMap::new();
        map.insert(
            "klog".to_owned(),
            ModuleAlias {
                prefix: String::new(),
                exports: HashSet::from(["klog_ok".to_owned()]),
                mangled: HashSet::new(),
                member_aliases: HashMap::from([("ok".to_owned(), "klog_ok".to_owned())]),
            },
        );
        rewrite_qualified_access(&mut program, &map).unwrap();

        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected main function");
        };
        let Statement::Expression(Expression::Primary(PrimaryExpr::FunctionCall { name, .. })) =
            &body.statements[0]
        else {
            panic!("short qualified call should rewrite to a flat FunctionCall");
        };
        assert_eq!(name, "klog_ok");
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
    fn qualified_access_resolves_to_mangled_export_names() {
        let mut program = parse_module(
            r#"
m := import("vmm")
main: () -> i32 {
    return m.map_page(m.PAGES)
}
"#,
        )
        .unwrap();
        rewrite_qualified_access(
            &mut program,
            &prefixed_aliases([("vmm", "m", &["map_page", "PAGES"][..])]),
        )
        .unwrap();

        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected main");
        };
        let Statement::Return(Some(Expression::Primary(PrimaryExpr::FunctionCall {
            name,
            arguments,
            ..
        }))) = &body.statements[0]
        else {
            panic!("expected a function call");
        };
        assert_eq!(name, "vmm__map_page");
        assert!(matches!(
            &arguments[0],
            Expression::Primary(PrimaryExpr::Identifier(n)) if n == "vmm__PAGES"
        ));
    }

    #[test]
    fn mangle_module_renames_defs_and_internal_refs() {
        let mut program = parse_module(
            r#"
external puts: (s: u8*) -> i32
count: i64 = 0
helper: () -> i64 { return count }
api: () -> i64 {
    helper()
    puts(null)
    count = count + 1
    return count
}
main: () -> i32 { return 0 }
"#,
        )
        .unwrap();
        let exempt = HashSet::from(["main".to_owned()]);
        mangle_module(&mut program, "vmm", &exempt);

        let names: Vec<&str> = program
            .declarations
            .iter()
            .filter_map(|d| decl_name(&d.decl))
            .collect();
        assert!(names.contains(&"vmm__count"));
        assert!(names.contains(&"vmm__helper"));
        assert!(names.contains(&"vmm__api"));
        // Entry point and hand-written external keep their ABI names.
        assert!(names.contains(&"main"));
        assert!(names.contains(&"puts"));

        let DeclNode::Function {
            body: Some(body), ..
        } = &program
            .declarations
            .iter()
            .find(|d| decl_name(&d.decl) == Some("vmm__api"))
            .unwrap()
            .decl
        else {
            panic!("expected api");
        };
        // Internal call to a mangled fn is rewritten; the external call is not.
        let Statement::Expression(Expression::Primary(PrimaryExpr::FunctionCall {
            name: call0,
            ..
        })) = &body.statements[0]
        else {
            panic!("expected helper() call");
        };
        assert_eq!(call0, "vmm__helper");
        let Statement::Expression(Expression::Primary(PrimaryExpr::FunctionCall {
            name: call1,
            ..
        })) = &body.statements[1]
        else {
            panic!("expected puts() call");
        };
        assert_eq!(call1, "puts");
    }

    #[test]
    fn mangle_module_skips_shadowing_local() {
        let mut program = parse_module(
            r#"
count: i64 = 5
f: () -> i64 {
    count := 9
    return count
}
"#,
        )
        .unwrap();
        mangle_module(&mut program, "m", &HashSet::new());

        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected f");
        };
        // The local `count` shadows the global, so the return must not be mangled.
        let Statement::Return(Some(Expression::Primary(PrimaryExpr::Identifier(name)))) =
            &body.statements[1]
        else {
            panic!("expected return of identifier");
        };
        assert_eq!(name, "count");
    }

    #[test]
    fn extract_interface_prefixed_mangles_only_link_symbols() {
        let source = r#"
export const TF_BYTES = 288
export struct Reloc { sym: u8[16], off: i64, kind: i64 }
export write_object: () -> u64 { return 0 }
export count: i64 = 0
"#;
        let interface = extract_interface_prefixed(source, "as").unwrap();
        // Functions and globals are mangled; types and consts fold unmangled.
        assert!(interface.contains("external as__write_object: () -> u64"));
        assert!(interface.contains("external as__count: i64"));
        assert!(interface.contains("const TF_BYTES = 288"));
        assert!(interface.contains("struct Reloc {"));
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
