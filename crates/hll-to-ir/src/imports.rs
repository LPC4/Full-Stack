//! Interface extraction for `import` / `export` (see `_LANG_SPECIFICATIONS.md` 6.3).
//! Pure source-to-source helpers; import-name resolution lives in the host pipeline.

use crate::ast::{
    BinaryOp, DeclNode, Expression, FieldDecl, Literal, Parameter, PrimaryExpr, Program,
    ReturnType, Type, UnaryOp, Variant,
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
                "external {}: ({}){}",
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
        DeclNode::InferredVariable { .. } | DeclNode::Import { .. } => return Ok(None),
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
