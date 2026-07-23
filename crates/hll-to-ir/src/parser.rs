use super::ast::{
    ArrayLen, AssignTarget, BinaryOp, BindingField, Block, DeclNode, Declaration, Expression,
    FieldDecl, FieldInit, ForIter, FormatSegment, InterfaceMethod, Literal, MatchArm, Parameter,
    Path, Pattern, PrimaryExpr, Program, ReturnType, Statement, StructDestructureField, Type,
    UnaryOp, Variant,
};
use crate::lexer::Lexer;

use super::token::{CompoundOp, Span, Token};

// Parsed `<...>` type parameters: the parameter names plus any `(param, interface)`
// bound pairs (bounds are only legal on functions; see parse_generic_params).
type GenericParams = (Vec<String>, Vec<(String, Path)>);

// The parsed contents of a `[...]` subscript: a plain index or a range slice.
enum Subscript {
    Index(Expression),
    Range {
        start: Option<Expression>,
        end: Option<Expression>,
        inclusive: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.span.location(), self.message)?;
        if !self.span.source_line.is_empty() {
            write!(f, "\n  | {}", self.span.source_line)?;
        }
        Ok(())
    }
}

// Digit separators are insignificant; the lexer already rejected misplaced ones.
fn strip_separators(text: &str) -> String {
    text.replace('_', "")
}

// `Option` and `Result` are injected after parsing, so seed them as enum names to
// let `Option.Some(x)` qualify; a user declaration of either re-inserts the name.
fn prelude_enum_names() -> std::collections::HashSet<String> {
    ["Option", "Result"]
        .iter()
        .map(|n| (*n).to_owned())
        .collect()
}

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    pub tokens: Vec<Token<'a>>,
    pub spans: Vec<Span>,
    pub pos: usize,
    pub pending_gt_from_shr: bool, // Track if we have a virtual `>` waiting from a split `>>`
    type_names: std::collections::HashSet<String>,
    enum_names: std::collections::HashSet<String>,
    synthesized_declarations: Vec<Declaration>,
    next_lambda_id: usize,
}

impl<'a> Parser<'a> {
    // Span-less constructor used only by parser/compiler unit tests; the real
    // pipeline always carries source spans via `new_with_spans`.
    #[cfg(test)]
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        let spans = vec![Span::default(); tokens.len()];
        Self {
            tokens,
            spans,
            pos: 0,
            pending_gt_from_shr: false,
            type_names: std::collections::HashSet::new(),
            enum_names: prelude_enum_names(),
            synthesized_declarations: Vec::new(),
            next_lambda_id: 0,
        }
    }

    pub fn new_with_spans(token_spans: Vec<(Token<'a>, Span)>) -> Self {
        let (tokens, spans): (Vec<_>, Vec<_>) = token_spans.into_iter().unzip();
        Self {
            tokens,
            spans,
            pos: 0,
            pending_gt_from_shr: false,
            type_names: std::collections::HashSet::new(),
            enum_names: prelude_enum_names(),
            synthesized_declarations: Vec::new(),
            next_lambda_id: 0,
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParserError> {
        let mut declarations = Vec::new();
        let mut statements = Vec::new();

        self.consume_terminators();
        while !self.is_eof() {
            if matches!(self.peek(), Some(Token::Impl)) {
                // `impl Type { ... }` desugars in place to flat `Type_method`
                // functions, so no later stage needs to know about methods.
                declarations.extend(self.parse_impl_block(false)?);
            } else if matches!(self.peek(), Some(Token::Export))
                && matches!(self.peek_n(1), Some(Token::Impl))
            {
                self.advance();
                declarations.extend(self.parse_impl_block(true)?);
            } else if self.is_declaration_start() {
                declarations.push(self.parse_declaration()?);
            } else {
                statements.push(self.parse_statement()?);
            }
            self.consume_terminators();
        }

        declarations.append(&mut self.synthesized_declarations);
        Ok(Program {
            declarations,
            statements,
        })
    }

    pub fn parse_declaration(&mut self) -> Result<Declaration, ParserError> {
        self.consume_terminators();

        // `export` marks the following declaration visible to importers. Record the
        // flag and parse the underlying declaration; it is otherwise unchanged.
        let exported = if matches!(self.peek(), Some(Token::Export)) {
            self.advance();
            self.consume_terminators();
            true
        } else {
            false
        };

        let decl = match self.peek() {
            Some(Token::Const) => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect_assign()?;
                if matches!(self.peek(), Some(Token::Import)) {
                    let path = self.parse_import_call()?;
                    DeclNode::ModuleImport { alias: name, path }
                } else {
                    let init = self.parse_expression()?;
                    DeclNode::Const { name, init }
                }
            }
            Some(Token::Type) => {
                self.advance();
                let name = self.expect_ident()?;
                let (generics, bounds) = self.parse_generic_params_with_bounds()?;
                self.expect_assign()?;
                let ty = self.parse_type()?;
                if matches!(ty, Type::Struct(_)) {
                    return Err(self.error(
                        "record declarations use `struct Name { ... }`; `type` is only for aliases",
                    ));
                }
                DeclNode::Type {
                    name,
                    generics,
                    bounds,
                    ty,
                }
            }
            Some(Token::Struct) => {
                self.advance();
                self.parse_struct_decl()?
            }
            Some(Token::Enum) => {
                self.advance();
                self.parse_enum_decl()?
            }
            Some(Token::Interface) => {
                self.advance();
                self.parse_interface_decl()?
            }
            Some(Token::External) => {
                let is_import_interface = self
                    .current_span()
                    .source_line
                    .contains("; @import-interface");
                self.advance();
                // `external name: (params) -> ret` is a function; `external name: type`
                // is a global defined in another module (resolved at link).
                if self.peek_n(1) == Some(&Token::Colon)
                    && (self.peek_n(2) == Some(&Token::LParen)
                        || self.peek_n(2) == Some(&Token::Lt))
                {
                    self.parse_function_decl(true, is_import_interface)?
                } else {
                    let name = self.expect_ident()?;
                    self.expect_colon()?;
                    let ty = self.parse_type()?;
                    DeclNode::Variable {
                        name,
                        ty,
                        init: None,
                        is_extern: true,
                    }
                }
            }
            Some(Token::Ident(_)) => {
                // Look ahead to determine if this is a function or variable declaration
                if self.peek_n(1) == Some(&Token::ColonEqual) {
                    // `alias := import("path")` is a module binding, not an ordinary
                    // inferred variable; its RHS is the `import(...)` builtin.
                    if self.peek_n(2) == Some(&Token::Import) {
                        let alias = self.expect_ident()?;
                        self.expect_colon_equal()?;
                        let path = self.parse_import_call()?;
                        DeclNode::ModuleImport { alias, path }
                    } else {
                        self.parse_inferred_variable_decl()?
                    }
                } else if self.peek_n(1) == Some(&Token::Colon)
                    && (self.peek_n(2) == Some(&Token::LParen)
                        || self.peek_n(2) == Some(&Token::Lt))
                {
                    self.parse_function_decl(false, false)?
                } else {
                    self.parse_variable_decl()?
                }
            }
            Some(tok) => {
                return Err(self.error_with_token("unexpected token at declaration start", tok));
            }
            None => return Err(self.error("unexpected end of input")),
        };

        if let DeclNode::Type { name, .. }
        | DeclNode::Struct { name, .. }
        | DeclNode::Enum { name, .. } = &decl
        {
            self.type_names.insert(name.clone());
        }
        if let DeclNode::Enum { name, .. } = &decl {
            self.enum_names.insert(name.clone());
        }
        Ok(Declaration { decl, exported })
    }

    pub fn parse_block(&mut self) -> Result<Block, ParserError> {
        self.expect_lbrace()?;
        let mut statements = Vec::new();
        let mut stmt_lines = Vec::new();

        self.consume_terminators();
        while !self.check_rbrace() {
            let line = self.current_line();
            statements.push(self.parse_statement()?);
            stmt_lines.push(line);
            self.consume_terminators();
        }

        self.expect_rbrace()?;
        Ok(Block {
            statements,
            stmt_lines,
        })
    }

    /// 1-based source line of the current token (0 if unavailable, e.g. tests).
    fn current_line(&self) -> u32 {
        self.spans.get(self.pos).map(|s| s.line).unwrap_or(0)
    }

    pub fn parse_statement(&mut self) -> Result<Statement, ParserError> {
        self.consume_terminators();

        match self.peek() {
            Some(Token::If) => self.parse_if_statement(),
            Some(Token::While) => self.parse_while_statement(),
            Some(Token::For) => self.parse_for_statement(),
            Some(Token::Return) => self.parse_return_statement(),
            Some(Token::Defer) => self.parse_defer_statement(),
            Some(Token::Asm) => self.parse_asm_block(),
            Some(Token::Break) => {
                self.advance();
                Ok(Statement::Break)
            }
            Some(Token::Continue) => {
                self.advance();
                Ok(Statement::Continue)
            }
            Some(Token::LBrace) => {
                if self.brace_followed_by_colon_equal() {
                    let fields = self.parse_binding_pattern()?;
                    self.expect_colon_equal()?;
                    let init = self.parse_expression()?;
                    return Ok(Statement::DestructureBinding { fields, init });
                }
                let mut trial = self.clone();
                if let Ok(target) = trial.parse_struct_destructure_target()
                    && trial.match_assign()
                {
                    *self = trial;
                    let rvalue = self.parse_assignment()?;
                    return Ok(Statement::Expression(Expression::Assignment {
                        target: Box::new(target),
                        rvalue: Box::new(rvalue),
                    }));
                }

                Ok(Statement::Block(self.parse_block()?))
            }
            Some(Token::Ident(_)) if self.peek_n(1) == Some(&Token::Colon) => {
                let name = self.expect_ident()?;
                self.expect_colon()?;
                let ty = self.parse_type()?;
                let init = if self.match_assign() {
                    Some(self.parse_expression()?)
                } else {
                    return Err(self.error(
                        "explicit declarations require an initializer: `name: Type = expression`",
                    ));
                };

                Ok(Statement::VariableDecl { name, ty, init })
            }
            Some(Token::Ident(_)) if self.peek_n(1) == Some(&Token::ColonEqual) => {
                let name = self.expect_ident()?;
                self.expect_colon_equal()?;
                if matches!(self.peek(), Some(Token::Import)) {
                    return Err(self.error(
                        "`import(...)` is only valid as a top-level module binding \
                         (`alias := import(\"...\")`), not inside a function body",
                    ));
                }
                let init = self.parse_expression()?;
                Ok(Statement::InferredVariableDecl { name, init })
            }
            Some(Token::Import) => Err(self.error(
                "`import(...)` is only valid as a top-level module binding \
                 (`alias := import(\"...\")`), not inside a function body",
            )),
            Some(_) => Ok(Statement::Expression(self.parse_expression()?)),
            None => Err(self.error("unexpected end of input while parsing statement")),
        }
    }

    pub fn parse_expression(&mut self) -> Result<Expression, ParserError> {
        self.parse_assignment()
    }

    pub fn parse_type(&mut self) -> Result<Type, ParserError> {
        if self.match_lbracket() {
            let size = self.parse_array_length()?;
            self.expect_rbracket()?;
            let ty = self.parse_type()?;
            return Ok(Type::Array(size, Box::new(ty)));
        }

        let mut ty = self.parse_type_atom()?;

        loop {
            // A virtual `>` still belongs to the enclosing generic type. Suffixes
            // after `>>` therefore apply only after that outer close is consumed.
            if self.pending_gt_from_shr {
                break;
            }

            if self.match_star() {
                ty = Type::Pointer(Box::new(ty));
                continue;
            }

            if self.match_lbracket() {
                // T[] is a slice; T[N] is a fixed array.
                if self.match_rbracket() {
                    ty = Type::Slice(Box::new(ty));
                    continue;
                }
                let size = self.parse_array_length()?;
                self.expect_rbracket()?;
                ty = Type::Array(size, Box::new(ty));
                continue;
            }

            break;
        }

        Ok(ty)
    }

    fn parse_variable_decl(&mut self) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        self.expect_colon()?;
        let ty = self.parse_type()?;
        let init = if self.match_assign() {
            Some(self.parse_expression()?)
        } else {
            return Err(self
                .error("explicit declarations require an initializer: `name: Type = expression`"));
        };

        Ok(DeclNode::Variable {
            name,
            ty,
            init,
            is_extern: false,
        })
    }

    fn parse_inferred_variable_decl(&mut self) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        self.expect_colon_equal()?;
        let init = self.parse_expression()?;
        Ok(DeclNode::InferredVariable { name, init })
    }

    /// Consume `import ( string )` and return the path literal, with `import` next.
    /// Used by the `alias := import(...)` and `const alias = import(...)` module forms.
    fn parse_import_call(&mut self) -> Result<String, ParserError> {
        self.advance(); // `import`
        self.expect_lparen()?;
        let path = self.expect_string_literal()?;
        self.expect_rparen()?;
        Ok(path)
    }

    fn parse_struct_decl(&mut self) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        let (generics, bounds) = self.parse_generic_params_with_bounds()?;
        self.expect_lbrace()?;
        let mut fields = Vec::new();
        self.consume_terminators();

        while !self.check_rbrace() {
            let field_name = self.expect_ident()?;
            self.expect_colon()?;
            let ty = self.parse_type()?;
            fields.push(FieldDecl {
                name: field_name,
                ty,
                init: None,
            });

            let newline_separated = matches!(self.peek(), Some(Token::StatementTerminator));
            self.consume_terminators();
            if self.match_comma() {
                self.consume_terminators();
            } else if !newline_separated && !self.check_rbrace() {
                return Err(self.error("expected `,` or newline between struct fields"));
            }
        }

        self.expect_rbrace()?;
        Ok(DeclNode::Struct {
            name,
            generics,
            bounds,
            fields,
        })
    }

    fn parse_enum_decl(&mut self) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        let (generics, bounds) = self.parse_generic_params_with_bounds()?;
        self.expect_lbrace()?;
        let mut variants = Vec::new();
        self.consume_terminators();

        while !self.check_rbrace() {
            let variant_name = self.expect_ident()?;
            let mut payload = Vec::new();
            if self.match_lparen() {
                if !self.check_rparen() {
                    loop {
                        payload.push(self.parse_type()?);
                        if self.match_comma() {
                            if self.check_rparen() {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.expect_rparen()?;
            }
            variants.push(Variant {
                name: variant_name,
                payload,
            });

            let newline_separated = matches!(self.peek(), Some(Token::StatementTerminator));
            self.consume_terminators();
            if self.match_comma() {
                self.consume_terminators();
            } else if !newline_separated && !self.check_rbrace() {
                return Err(self.error("expected `,` or newline between enum variants"));
            }
        }

        self.expect_rbrace()?;
        Ok(DeclNode::Enum {
            name,
            generics,
            bounds,
            variants,
        })
    }

    fn parse_function_decl(
        &mut self,
        is_extern: bool,
        is_import_interface: bool,
    ) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        self.expect_colon()?;

        let (generics, bounds) = self.parse_generic_params_with_bounds()?;

        let params = self.parse_param_list()?;

        let return_type = if self.match_arrow() {
            if self.peek() == Some(&Token::LParen) && self.peek_n(1) == Some(&Token::RParen) {
                return Err(self.error("void functions omit `->`; `-> ()` is not valid"));
            }
            Some(self.parse_return_type()?)
        } else {
            None
        };

        let body = if is_extern {
            None
        } else if self.check_lbrace() {
            Some(self.parse_block()?)
        } else {
            return Err(self.error("expected function body block"));
        };

        Ok(DeclNode::Function {
            name,
            generics,
            bounds,
            params,
            return_type,
            body,
            is_extern,
            is_import_interface,
        })
    }

    /// Parse `interface Name { method: (self: Self*, ...) -> R ... }`: required
    /// method signatures only, with `Self` standing for the implementing type.
    fn parse_interface_decl(&mut self) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        self.expect_lbrace()?;
        self.consume_terminators();

        let mut methods = Vec::new();
        while !self.check_rbrace() {
            let method_name = self.expect_ident()?;
            self.expect_colon()?;
            let params = self.parse_param_list()?;
            let return_type = if self.match_arrow() {
                Some(self.parse_return_type()?)
            } else {
                None
            };

            let receiver_ok = params.first().is_some_and(|receiver| {
                receiver.name == "self"
                    && matches!(
                        &receiver.ty,
                        Type::Pointer(inner)
                            if matches!(inner.as_ref(), Type::Named { name, .. } if name == "Self")
                    )
            });
            if !receiver_ok {
                return Err(self.error("interface method's first parameter must be `self: Self*`"));
            }

            methods.push(InterfaceMethod {
                name: method_name,
                params,
                return_type,
            });
            self.consume_terminators();
        }

        self.expect_rbrace()?;
        Ok(DeclNode::Interface { name, methods })
    }

    /// Parse an impl block; function members desugar to flat `Type_name`
    /// functions, `const`/`type` members become one `AssociatedItems` node.
    fn parse_impl_block(&mut self, exported: bool) -> Result<Vec<Declaration>, ParserError> {
        self.advance(); // `impl`
        let type_name = self.expect_ident()?;
        let (owner_generics, owner_bounds) = self.parse_generic_params_with_bounds()?;
        self.expect_lbrace()?;
        self.consume_terminators();

        let mut methods = Vec::new();
        let mut consts = Vec::new();
        let mut aliases = Vec::new();
        while !self.check_rbrace() {
            if matches!(self.peek(), Some(Token::Const)) {
                self.advance();
                let name = self.expect_ident()?;
                self.expect_assign()?;
                consts.push((name, self.parse_expression()?));
                self.consume_terminators();
                continue;
            }
            if matches!(self.peek(), Some(Token::Type)) {
                self.advance();
                let name = self.expect_ident()?;
                self.expect_assign()?;
                aliases.push((name, self.parse_type()?));
                self.consume_terminators();
                continue;
            }
            let DeclNode::Function {
                name,
                generics,
                bounds,
                params,
                return_type,
                body,
                ..
            } = self.parse_function_decl(false, false)?
            else {
                unreachable!("parse_function_decl always returns a Function");
            };

            if let Some(receiver) = params.first().filter(|param| param.name == "self") {
                let receiver_ok = matches!(
                    &receiver.ty,
                    Type::Named { name, .. } if **name == *type_name
                ) || matches!(
                    &receiver.ty,
                    Type::Pointer(inner) if matches!(inner.as_ref(), Type::Named { name, .. } if **name == *type_name)
                );
                if !receiver_ok {
                    return Err(self.error(&format!(
                        "impl method `self` must be `{type_name}` or `{type_name}*`"
                    )));
                }
            }

            // Owner generics apply to every function member; explicit member
            // generics follow them.
            let mut merged_generics = owner_generics.clone();
            for generic in generics {
                if merged_generics.contains(&generic) {
                    return Err(self.error(&format!(
                        "method generic `{generic}` shadows an impl owner generic"
                    )));
                }
                merged_generics.push(generic);
            }
            let mut merged_bounds = owner_bounds.clone();
            merged_bounds.extend(bounds);

            methods.push(Declaration {
                decl: DeclNode::Function {
                    name: format!("{type_name}_{name}"),
                    generics: merged_generics,
                    bounds: merged_bounds,
                    params,
                    return_type,
                    body,
                    is_extern: false,
                    is_import_interface: false,
                },
                exported,
            });
            self.consume_terminators();
        }

        self.expect_rbrace()?;
        if !consts.is_empty() || !aliases.is_empty() {
            if exported {
                return Err(self.error(
                    "associated constants and type aliases cannot be exported; keep the impl private",
                ));
            }
            methods.push(Declaration {
                decl: DeclNode::AssociatedItems {
                    owner: type_name,
                    generics: owner_generics,
                    consts,
                    aliases,
                },
                exported: false,
            });
        }
        Ok(methods)
    }

    fn parse_param_list(&mut self) -> Result<Vec<Parameter>, ParserError> {
        self.expect_lparen()?;
        let mut params = Vec::new();

        if self.match_rparen() {
            return Ok(params);
        }

        loop {
            let name = self.expect_ident()?;
            self.expect_colon()?;
            let ty = self.parse_type()?;
            params.push(Parameter { name, ty });

            if self.match_comma() {
                if self.check_rparen() {
                    break;
                }
                continue;
            }

            break;
        }

        self.expect_rparen()?;
        Ok(params)
    }

    fn parse_return_type(&mut self) -> Result<ReturnType, ParserError> {
        Ok(ReturnType::Single(self.parse_type()?))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_if()?;
        let cond = self.parse_expression()?;
        let then_block = self.parse_block()?;

        let else_branch = if self.match_else() {
            if self.check_if() {
                Some(Box::new(self.parse_if_statement()?))
            } else {
                Some(Box::new(Statement::Block(self.parse_block()?)))
            }
        } else {
            None
        };

        Ok(Statement::If {
            cond,
            then_block,
            else_branch,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_while()?;
        let cond = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(Statement::While { cond, body })
    }

    fn parse_for_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_for()?;
        let var = self.expect_ident()?;
        if !self.match_in() {
            return Err(self.error("expected `in` after the `for` loop variable"));
        }
        let first = self.parse_expression()?;
        let iter = if self.match_dot_dot_eq() {
            let end = self.parse_expression()?;
            ForIter::Range {
                start: first,
                end,
                inclusive: true,
            }
        } else if self.match_dot_dot() {
            let end = self.parse_expression()?;
            ForIter::Range {
                start: first,
                end,
                inclusive: false,
            }
        } else {
            // No range operator: iterate the expression's elements.
            ForIter::Each(first)
        };
        let body = self.parse_block()?;
        Ok(Statement::For { var, iter, body })
    }

    fn parse_return_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_return()?;
        if self.is_expression_terminator() || self.check_rbrace() || self.is_eof() {
            Ok(Statement::Return(None))
        } else {
            let expr = self.parse_expression()?;
            Ok(Statement::Return(Some(expr)))
        }
    }

    fn parse_defer_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_defer()?;
        Ok(Statement::Defer(self.parse_expression()?))
    }

    fn parse_asm_block(&mut self) -> Result<Statement, ParserError> {
        self.advance(); // consume Token::Asm
        self.expect_lbrace()?;
        self.consume_terminators(); // skip the newline after `{`

        let mut lines = Vec::new();

        while !self.check_rbrace() && !self.is_eof() {
            if matches!(self.peek(), Some(Token::StatementTerminator)) {
                let source = self.spans[self.pos].source_line.trim().to_owned();
                self.advance();
                if !source.is_empty() {
                    lines.push(source);
                }
            } else {
                self.advance();
            }
        }

        self.expect_rbrace()?;
        Ok(Statement::AsmBlock { lines })
    }

    fn parse_assignment(&mut self) -> Result<Expression, ParserError> {
        if matches!(self.peek(), Some(Token::LBrace)) {
            let saved_pos = self.pos;
            let mut trial = self.clone();

            if let Ok(target) = trial.parse_struct_destructure_target()
                && trial.match_assign()
            {
                *self = trial;
                let rvalue = self.parse_assignment()?;
                return Ok(Expression::Assignment {
                    target: Box::new(target),
                    rvalue: Box::new(rvalue),
                });
            }

            self.pos = saved_pos;
        }

        let left = self.parse_or()?;
        if self.match_assign() {
            let target = self.expression_to_target(left)?;
            let rvalue = self.parse_assignment()?;
            Ok(Expression::Assignment {
                target: Box::new(target),
                rvalue: Box::new(rvalue),
            })
        } else if let Some(op) = self.match_compound_assign() {
            // Desugar `lhs OP= rhs` into `lhs = lhs OP (rhs)`. The lhs expression
            // is reused as the binary's left operand, so it is evaluated twice;
            // HLL targets are simple lvalues, so this is benign.
            let target = self.expression_to_target(left.clone())?;
            let rhs = self.parse_assignment()?;
            let rvalue = Expression::Binary {
                op,
                left: Box::new(left),
                right: Box::new(rhs),
            };
            Ok(Expression::Assignment {
                target: Box::new(target),
                rvalue: Box::new(rvalue),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_or(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_and()?;
        while self.match_or() {
            self.consume_terminators();
            let right = self.parse_and()?;
            expr = Expression::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_equality()?;
        while self.match_and() {
            self.consume_terminators();
            let right = self.parse_equality()?;
            expr = Expression::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Bitwise operators bind tighter than the comparisons (as in Rust, unlike C),
    // so `flags & MASK == 0` is `(flags & MASK) == 0`.
    fn parse_bitwise_or(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_bitwise_xor()?;
        while self.match_bitwise_or() {
            self.consume_terminators();
            let right = self.parse_bitwise_xor()?;
            expr = Expression::Binary {
                op: BinaryOp::BitwiseOr,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_bitwise_and()?;
        while self.match_bitwise_xor() {
            self.consume_terminators();
            let right = self.parse_bitwise_and()?;
            expr = Expression::Binary {
                op: BinaryOp::BitwiseXor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_shift()?;
        while self.match_bitwise_and() {
            self.consume_terminators();
            let right = self.parse_shift()?;
            expr = Expression::Binary {
                op: BinaryOp::BitwiseAnd,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_comparison()?;
        loop {
            self.consume_terminators();
            let op = if self.match_eq() {
                Some(BinaryOp::Eq)
            } else if self.match_neq() {
                Some(BinaryOp::Neq)
            } else {
                None
            };

            let Some(op) = op else { break };
            self.consume_terminators();
            let right = self.parse_comparison()?;
            expr = Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_bitwise_or()?;
        loop {
            self.consume_terminators();
            let op = if self.match_lt() {
                Some(BinaryOp::Lt)
            } else if self.match_lte() {
                Some(BinaryOp::Lte)
            } else if self.match_gt() {
                Some(BinaryOp::Gt)
            } else if self.match_gte() {
                Some(BinaryOp::Gte)
            } else {
                None
            };

            let Some(op) = op else { break };
            self.consume_terminators();
            let right = self.parse_bitwise_or()?;
            expr = Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_shift(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_additive()?;
        loop {
            self.consume_terminators();
            let op = if self.match_shl() {
                Some(BinaryOp::Shl)
            } else if self.match_shr() {
                Some(BinaryOp::Shr)
            } else {
                None
            };

            let Some(op) = op else { break };
            self.consume_terminators();
            let right = self.parse_additive()?;
            expr = Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            self.consume_terminators();
            let op = if self.match_plus() {
                Some(BinaryOp::Add)
            } else if self.match_minus() {
                Some(BinaryOp::Sub)
            } else {
                None
            };

            let Some(op) = op else { break };
            self.consume_terminators();
            let right = self.parse_multiplicative()?;
            expr = Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, ParserError> {
        let mut expr = self.parse_prefix()?;
        loop {
            self.consume_terminators();

            // Check for pointer-cast syntax: type*(expr)
            if self.peek() == Some(&Token::Star)
                && let Expression::Primary(PrimaryExpr::Identifier(name)) = &expr
                && let Some(base_ty) = self.parse_type_name_as_cast(name)
                && self.peek_n(1) == Some(&Token::LParen)
            {
                self.advance();

                let ptr_ty = Type::Pointer(Box::new(base_ty));

                self.advance(); // consume LParen
                let arguments = self.parse_argument_list_after_open_paren()?;

                if arguments.len() != 1 {
                    return Err(self.error("type cast expects exactly one argument"));
                }

                expr = Expression::Cast {
                    target_ty: ptr_ty,
                    expr: Box::new(arguments.into_iter().next().unwrap()),
                };

                expr = self.parse_postfix(expr)?;
                continue;
            }

            let op = if self.match_star() {
                Some(BinaryOp::Mul)
            } else if self.match_slash() {
                Some(BinaryOp::Div)
            } else if self.match_percent() {
                Some(BinaryOp::Mod)
            } else {
                None
            };

            let Some(op) = op else { break };
            self.consume_terminators();
            let right = self.parse_prefix()?;
            expr = Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_prefix(&mut self) -> Result<Expression, ParserError> {
        let mut ops = Vec::new();

        loop {
            let op = if self.match_minus() {
                Some(UnaryOp::Negate)
            } else if self.match_not() {
                Some(UnaryOp::Not)
            } else if self.match_tilde() {
                Some(UnaryOp::BitNot)
            } else if self.match_ampersand() {
                Some(UnaryOp::AddressOf)
            } else if self.match_at() {
                Some(UnaryOp::Dereference)
            } else {
                None
            };

            let Some(op) = op else { break };
            ops.push(op);
        }

        let mut expr = self.parse_primary()?;
        expr = self.parse_postfix(expr)?;
        for op in ops.into_iter().rev() {
            expr = Expression::Unary {
                op,
                expr: Box::new(expr),
            };
        }
        Ok(expr)
    }

    // Parse the contents of a `[...]` subscript: either a single index or a range
    // (`a..b`, `a..=b`, `a..`, `..b`, `..`). Brackets are consumed by the caller.
    fn parse_subscript(&mut self) -> Result<Subscript, ParserError> {
        // Open-start range: `..end` or `..`.
        if self.match_dot_dot_eq() {
            return Ok(Subscript::Range {
                start: None,
                end: self.parse_optional_range_end()?,
                inclusive: true,
            });
        }
        if self.match_dot_dot() {
            return Ok(Subscript::Range {
                start: None,
                end: self.parse_optional_range_end()?,
                inclusive: false,
            });
        }

        let first = self.parse_expression()?;
        if self.match_dot_dot_eq() {
            return Ok(Subscript::Range {
                start: Some(first),
                end: self.parse_optional_range_end()?,
                inclusive: true,
            });
        }
        if self.match_dot_dot() {
            return Ok(Subscript::Range {
                start: Some(first),
                end: self.parse_optional_range_end()?,
                inclusive: false,
            });
        }
        Ok(Subscript::Index(first))
    }

    // A range end is omitted when the next token closes the subscript (`..]`).
    fn parse_optional_range_end(&mut self) -> Result<Option<Expression>, ParserError> {
        if self.check_rbracket() {
            Ok(None)
        } else {
            Ok(Some(self.parse_expression()?))
        }
    }

    fn parse_postfix(&mut self, mut expr: Expression) -> Result<Expression, ParserError> {
        loop {
            if self.match_dot() {
                let field = self.expect_ident()?;
                // `Enum.Variant` is a qualified variant constructor; a lowercase
                // member on an enum name stays a field access or method call.
                if let Expression::Primary(PrimaryExpr::Identifier(base)) = &expr
                    && self.enum_names.contains(base)
                    && field.chars().next().is_some_and(char::is_uppercase)
                {
                    expr = Expression::Primary(PrimaryExpr::Identifier(format!("{base}.{field}")));
                    continue;
                }
                expr = Expression::Primary(PrimaryExpr::FieldAccess {
                    expr: Box::new(expr),
                    field,
                });
                continue;
            }

            if self.match_lbracket() {
                let subscript = self.parse_subscript()?;
                self.expect_rbracket()?;
                expr = match subscript {
                    Subscript::Index(index) => Expression::Primary(PrimaryExpr::ArrayIndex {
                        expr: Box::new(expr),
                        index: Box::new(index),
                    }),
                    Subscript::Range {
                        start,
                        end,
                        inclusive,
                    } => Expression::Primary(PrimaryExpr::Slice {
                        expr: Box::new(expr),
                        start: start.map(Box::new),
                        end: end.map(Box::new),
                        inclusive,
                    }),
                };
                continue;
            }

            if matches!(self.peek(), Some(Token::As)) {
                self.advance();
                let target_ty = self.parse_type()?;
                expr = Expression::Cast {
                    target_ty,
                    expr: Box::new(expr),
                };
                continue;
            }

            if matches!(self.peek(), Some(Token::Question)) {
                self.advance();
                expr = Expression::Try(Box::new(expr));
                continue;
            }

            if matches!(&expr, Expression::Primary(PrimaryExpr::Identifier(_)))
                && let Some((owner_args, member)) = self.try_parse_associated_member()
            {
                let Expression::Primary(PrimaryExpr::Identifier(owner)) = expr else {
                    unreachable!();
                };
                if matches!(self.peek(), Some(Token::LParen)) {
                    return Err(
                        self.error("type arguments on associated function calls are not supported")
                    );
                }
                expr = Expression::Primary(PrimaryExpr::AssociatedConst {
                    owner,
                    owner_args,
                    member,
                });
                continue;
            }

            let type_arguments = self.try_parse_call_type_arguments();

            if type_arguments.is_some() || self.match_lparen() {
                // A non-identifier callee (field access, index, deref) is an
                // indirect call through a function-pointer value.
                let name = match expr {
                    Expression::Primary(PrimaryExpr::Identifier(name)) => name,
                    Expression::Primary(PrimaryExpr::FieldAccess { expr: base, field }) if matches!(base.as_ref(), Expression::Primary(PrimaryExpr::Identifier(name)) if self.type_names.contains(name)) =>
                    {
                        if type_arguments.is_some() {
                            return Err(self.error(
                                "type arguments on associated function calls are not supported",
                            ));
                        }
                        let Expression::Primary(PrimaryExpr::Identifier(type_name)) = *base else {
                            unreachable!();
                        };
                        let arguments = self.parse_argument_list_after_open_paren()?;
                        expr = Expression::Primary(PrimaryExpr::FunctionCall {
                            name: format!("{type_name}_{field}"),
                            type_arguments: Vec::new(),
                            arguments,
                        });
                        continue;
                    }
                    // `alias.member<T>(...)`: a qualified generic call. The dotted name
                    // resolves against the module alias table before monomorphization.
                    Expression::Primary(PrimaryExpr::FieldAccess { expr: base, field })
                        if type_arguments.is_some()
                            && matches!(
                                base.as_ref(),
                                Expression::Primary(PrimaryExpr::Identifier(_))
                            ) =>
                    {
                        let Expression::Primary(PrimaryExpr::Identifier(module)) = *base else {
                            unreachable!();
                        };
                        format!("{module}.{field}")
                    }
                    callee => {
                        if type_arguments.is_some() {
                            return Err(
                                self.error("type arguments are only valid on named function calls")
                            );
                        }
                        if let Some(format_into) = self.try_parse_format_into(&callee)? {
                            expr = format_into;
                            continue;
                        }
                        let arguments = self.parse_argument_list_after_open_paren()?;
                        expr = Expression::Primary(PrimaryExpr::CallExpr {
                            callee: Box::new(callee),
                            arguments,
                        });
                        continue;
                    }
                };

                let type_arguments = type_arguments.unwrap_or_default();

                if name == "asm_reg" {
                    if !type_arguments.is_empty() {
                        return Err(self.error("asm_reg does not accept type arguments"));
                    }
                    let reg = self.expect_ident()?;
                    self.expect_rparen()?;
                    expr = Expression::Primary(PrimaryExpr::AsmReg { reg });
                } else if let Some(target_ty) = self.parse_type_name_as_cast(&name) {
                    if !type_arguments.is_empty() {
                        return Err(self.error("type casts do not accept type arguments"));
                    }
                    let arguments = self.parse_argument_list_after_open_paren()?;
                    if arguments.len() != 1 {
                        return Err(self.error("type cast expects exactly one argument"));
                    }
                    expr = Expression::Cast {
                        target_ty,
                        expr: Box::new(arguments.into_iter().next().unwrap()),
                    };
                } else {
                    let arguments = self.parse_argument_list_after_open_paren()?;
                    expr = Expression::Primary(PrimaryExpr::FunctionCall {
                        name,
                        type_arguments,
                        arguments,
                    });
                }
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn try_parse_call_type_arguments(&mut self) -> Option<Vec<Type>> {
        if self.peek() != Some(&Token::Lt) {
            return None;
        }

        let saved_pos = self.pos;
        let saved_pending_gt = self.pending_gt_from_shr;
        self.advance();
        let parsed = (|| -> Result<Vec<Type>, ParserError> {
            let mut args = Vec::new();
            loop {
                args.push(self.parse_type()?);
                if self.match_comma() {
                    continue;
                }
                break;
            }
            self.expect_generic_close()?;
            self.expect_lparen()?;
            Ok(args)
        })();

        if let Ok(args) = parsed {
            Some(args)
        } else {
            self.pos = saved_pos;
            self.pending_gt_from_shr = saved_pending_gt;
            None
        }
    }

    // `Owner<args>.MEMBER`: an associated-constant projection in expression
    // position. Backtracks (like call type arguments) when the shape mismatches.
    fn try_parse_associated_member(&mut self) -> Option<(Vec<Type>, String)> {
        if self.peek() != Some(&Token::Lt) {
            return None;
        }

        let saved_pos = self.pos;
        let saved_pending_gt = self.pending_gt_from_shr;
        self.advance();
        let parsed = (|| -> Result<(Vec<Type>, String), ParserError> {
            let mut args = Vec::new();
            loop {
                args.push(self.parse_type()?);
                if self.match_comma() {
                    continue;
                }
                break;
            }
            self.expect_generic_close()?;
            if !self.match_dot() {
                return Err(self.error("expected `.` after associated owner arguments"));
            }
            Ok((args, self.expect_ident()?))
        })();

        if let Ok(result) = parsed {
            Some(result)
        } else {
            self.pos = saved_pos;
            self.pending_gt_from_shr = saved_pending_gt;
            None
        }
    }

    fn expect_generic_close(&mut self) -> Result<(), ParserError> {
        if self.pending_gt_from_shr {
            self.pending_gt_from_shr = false;
            return Ok(());
        }
        if self.match_gt() {
            return Ok(());
        }
        if matches!(self.peek(), Some(Token::Shr | Token::Gte)) {
            self.advance();
            self.pending_gt_from_shr = true;
            return Ok(());
        }
        Err(self.error("expected `>` to close type arguments"))
    }

    fn parse_type_atom(&mut self) -> Result<Type, ParserError> {
        match self.peek() {
            Some(Token::I8) => {
                self.advance();
                Ok(Type::Primitive("i8".into()))
            }
            Some(Token::I16) => {
                self.advance();
                Ok(Type::Primitive("i16".into()))
            }
            Some(Token::I32) => {
                self.advance();
                Ok(Type::Primitive("i32".into()))
            }
            Some(Token::I64) => {
                self.advance();
                Ok(Type::Primitive("i64".into()))
            }
            Some(Token::U8) => {
                self.advance();
                Ok(Type::Primitive("u8".into()))
            }
            Some(Token::U16) => {
                self.advance();
                Ok(Type::Primitive("u16".into()))
            }
            Some(Token::U32) => {
                self.advance();
                Ok(Type::Primitive("u32".into()))
            }
            Some(Token::U64) => {
                self.advance();
                Ok(Type::Primitive("u64".into()))
            }
            Some(Token::F32) => {
                self.advance();
                Ok(Type::Primitive("f32".into()))
            }
            Some(Token::F64) => {
                self.advance();
                Ok(Type::Primitive("f64".into()))
            }
            Some(Token::Bool) => {
                self.advance();
                Ok(Type::Primitive("bool".into()))
            }
            Some(Token::Fn) => Err(self.error(
                "legacy function-pointer syntax is not supported; write `(T, U) -> R` instead of `fn(T, U) -> R`",
            )),
            Some(Token::Dyn) => {
                // `dyn I` is carried as a marker until the dyn
                // lowering pass rewrites it to the synthesized `I__dyn` struct.
                self.advance();
                let mut segments = vec![self.expect_ident()?];
                while self.match_dot() {
                    segments.push(self.expect_ident()?);
                }
                Ok(Type::DynamicReference(Path::from_segments(segments)))
            }
            Some(Token::Ident(_)) => {
                let mut segments = vec![self.expect_ident()?];
                while self.match_dot() {
                    segments.push(self.expect_ident()?);
                }
                let args = if self.match_lt() {
                    let mut args = Vec::new();
                    // Check for closing bracket, accounting for `>>` in nested generics
                    let should_close = self.check_gt_or_shr();

                    if !should_close {
                        loop {
                            args.push(self.parse_type()?);
                            // After parsing a type, check what's next
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.advance(); // consume comma
                                    // Check if we're at closing bracket now
                                    if self.check_gt_or_shr() {
                                        break;
                                    }
                                }
                                Some(Token::Gt | Token::Shr | Token::Gte) => {
                                    break;
                                }
                                _ => {
                                    if self.pending_gt_from_shr {
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Consume the closing bracket
                    if self.pending_gt_from_shr {
                        // We have a pending virtual >
                        self.pending_gt_from_shr = false;
                    } else if self.match_gt() {
                        // Simple case: plain >
                    } else if matches!(self.peek(), Some(Token::Shr | Token::Gte)) {
                        // In nested generic, >> and >= both contain a closing >
                        // Consume and set flag for virtual second >
                        self.advance();
                        self.pending_gt_from_shr = true;
                    } else {
                        return Err(self.error("expected `>` to close generic parameters"));
                    }

                    args
                } else {
                    Vec::new()
                };

                // `Matrix<i32>.Row` projects an associated type alias; the
                // zero-argument form `Config.Row` stays a two-segment path.
                if !args.is_empty() && self.match_dot() {
                    let member = self.expect_ident()?;
                    return Ok(Type::Associated {
                        owner: segments.join("."),
                        owner_args: args,
                        member,
                    });
                }

                Ok(Type::Named {
                    name: Path::from_segments(segments),
                    args,
                })
            }
            Some(Token::LBrace) => self.parse_struct_type(),
            Some(Token::LParen) => self.parse_function_pointer_type(),
            Some(tok) => Err(self.error_with_token("expected type", tok)),
            None => Err(self.error("unexpected end of input while parsing type")),
        }
    }

    fn parse_function_pointer_type(&mut self) -> Result<Type, ParserError> {
        self.expect_lparen()?;
        let mut params = Vec::new();
        self.consume_terminators();
        if !self.check_rparen() {
            loop {
                params.push(self.parse_type()?);
                self.consume_terminators();
                if self.match_comma() {
                    self.consume_terminators();
                    if self.check_rparen() {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect_rparen()?;

        let return_type = if self.match_arrow() {
            if matches!(self.peek(), Some(Token::LParen))
                && matches!(self.peek_n(1), Some(Token::RParen))
            {
                return Err(self.error("void function pointer types omit `->`; use `(...)`"));
            }
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        Ok(Type::Function {
            params,
            return_type,
        })
    }

    fn parse_struct_type(&mut self) -> Result<Type, ParserError> {
        self.expect_lbrace()?;
        let mut fields = Vec::new();

        if !self.check_rbrace() {
            loop {
                self.consume_terminators();
                let name = self.expect_ident()?;
                self.expect_colon()?;
                let ty = self.parse_type()?;
                let init = if self.match_assign() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                fields.push(FieldDecl { name, ty, init });

                self.consume_terminators();
                if self.match_comma() {
                    self.consume_terminators();
                    if self.check_rbrace() {
                        break;
                    }
                    continue;
                }
                if self.check_rbrace() {
                    break;
                }
                return Err(self.error("expected `,` between struct type fields"));
            }
        }

        self.expect_rbrace()?;
        Ok(Type::Struct(fields))
    }

    // `<T, U: Writer + Debug>`: each parameter may carry interface bounds. Bounds are
    // returned as `(param, interface)` pairs and checked at specialization time.
    fn parse_generic_params_with_bounds(&mut self) -> Result<GenericParams, ParserError> {
        if !self.match_lt() {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut params = Vec::new();
        let mut bounds = Vec::new();

        // Check for closing bracket
        let should_close = self.check_gt_or_shr();

        if !should_close {
            loop {
                let param = self.expect_ident()?;
                if params.contains(&param) {
                    return Err(self.error(&format!("duplicate generic parameter `{param}`")));
                }
                if matches!(self.peek(), Some(Token::Colon)) {
                    self.advance();
                    let mut seen = std::collections::HashSet::new();
                    loop {
                        let mut segments = vec![self.expect_ident()?];
                        while self.match_dot() {
                            segments.push(self.expect_ident()?);
                        }
                        let interface = Path::from_segments(segments);
                        if !seen.insert(interface.clone()) {
                            return Err(
                                self.error(&format!("duplicate bound `{param}: {interface}`"))
                            );
                        }
                        bounds.push((param.clone(), interface));
                        if !self.match_plus() {
                            break;
                        }
                    }
                }
                params.push(param);
                if self.match_comma() {
                    if self.check_gt_or_shr() {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        // Consume the closing bracket
        if self.pending_gt_from_shr {
            // Virtual > available from previous Shr split
            self.pending_gt_from_shr = false;
        } else if self.match_gt() {
            // plain >
        } else if matches!(self.peek(), Some(Token::Shr | Token::Gte)) {
            self.advance();
            self.pending_gt_from_shr = true;
        } else {
            return Err(self.error("expected `>` to close generic parameters"));
        }

        Ok((params, bounds))
    }

    fn parse_argument_list_after_open_paren(&mut self) -> Result<Vec<Expression>, ParserError> {
        let mut args = Vec::new();
        self.consume_terminators(); // after '('
        if self.match_rparen() {
            return Ok(args);
        }

        loop {
            self.consume_terminators(); // before each argument
            args.push(self.parse_expression()?);
            self.consume_terminators(); // after argument
            if self.match_comma() {
                self.consume_terminators(); // after comma
                if self.check_rparen() {
                    break;
                }
                continue;
            }
            break;
        }

        self.consume_terminators(); // before ')'
        self.expect_rparen()?;
        Ok(args)
    }

    fn parse_type_name_as_cast(&self, name: &str) -> Option<Type> {
        match name {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64"
            | "bool" => Some(Type::Primitive(name.to_owned())),
            _ => None,
        }
    }

    fn parse_struct_destructure_target(&mut self) -> Result<AssignTarget, ParserError> {
        self.expect_lbrace()?;
        let mut fields = Vec::new();

        if self.check_rbrace() {
            return Err(self.error("empty struct destructuring is not allowed"));
        }

        loop {
            fields.push(self.parse_struct_destructure_field()?);
            if self.match_comma() {
                if self.check_rbrace() {
                    break;
                }
                continue;
            }
            break;
        }

        self.expect_rbrace()?;
        Ok(AssignTarget::StructDestructure(fields))
    }

    fn parse_binding_pattern(&mut self) -> Result<Vec<BindingField>, ParserError> {
        self.expect_lbrace()?;
        if self.check_rbrace() {
            return Err(self.error("empty binding patterns are not allowed"));
        }

        let mut fields = Vec::new();
        let mut sources = std::collections::HashSet::new();
        let mut bindings = std::collections::HashSet::new();
        loop {
            // `binding = source` mirrors declaration order; bare `name` binds
            // the field of the same name.
            let first = self.expect_ident()?;
            let (binding, source) = if self.match_assign() {
                (first, self.expect_ident()?)
            } else {
                (first.clone(), first)
            };
            if !sources.insert(source.clone()) {
                return Err(self.error(&format!(
                    "duplicate source field `{source}` in binding pattern"
                )));
            }
            if !bindings.insert(binding.clone()) {
                return Err(
                    self.error(&format!("duplicate binding `{binding}` in binding pattern"))
                );
            }
            fields.push(BindingField { source, binding });
            if !self.match_comma() {
                break;
            }
            if self.check_rbrace() {
                break;
            }
        }
        self.expect_rbrace()?;
        Ok(fields)
    }

    fn brace_followed_by_colon_equal(&self) -> bool {
        let mut depth = 0usize;
        for index in self.pos..self.tokens.len() {
            match self.tokens[index] {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self.tokens.get(index + 1) == Some(&Token::ColonEqual);
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn parse_struct_destructure_field(&mut self) -> Result<StructDestructureField, ParserError> {
        let name = self.expect_ident()?;
        self.expect_colon()?;
        let ty = self.parse_type()?;

        Ok(StructDestructureField {
            name: Some(name),
            ty: Some(ty),
        })
    }

    fn expression_to_target(&self, expr: Expression) -> Result<AssignTarget, ParserError> {
        match expr {
            Expression::Primary(PrimaryExpr::Identifier(name)) => {
                Ok(AssignTarget::Identifier(name))
            }
            Expression::Primary(PrimaryExpr::Grouped(expr)) => self.expression_to_target(*expr),
            Expression::Unary {
                op: UnaryOp::Dereference,
                expr,
            } => Ok(AssignTarget::Dereference(Box::new(
                self.expression_to_target(*expr)?,
            ))),
            Expression::Primary(PrimaryExpr::FieldAccess { expr, field }) => {
                Ok(AssignTarget::FieldAccess {
                    expr: Box::new(self.expression_to_target(*expr)?),
                    field,
                })
            }
            Expression::Primary(PrimaryExpr::ArrayIndex { expr, index }) => {
                Ok(AssignTarget::ArrayIndex {
                    expr: Box::new(self.expression_to_target(*expr)?),
                    index,
                })
            }
            _ => Err(self.error("left side of assignment is not assignable")),
        }
    }

    fn consume_terminators(&mut self) {
        while self.match_statement_terminator() {}
    }

    fn process_string_escapes(&self, input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars();

        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('r') => result.push('\r'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some(other) => {
                        result.push('\\');
                        result.push(other);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    // An array length in type position: a literal, or a compile-time constant
    // expression (const decls and associated constants) folded before analysis.
    fn parse_array_length(&mut self) -> Result<ArrayLen, ParserError> {
        if let Some(Token::Integer(text)) = self.peek()
            && matches!(self.peek_n(1), Some(Token::RBracket))
        {
            let value = strip_separators(text)
                .parse::<usize>()
                .map_err(|_| self.error("invalid array size"))?;
            self.advance();
            return Ok(ArrayLen::Fixed(value));
        }
        match self.peek() {
            Some(Token::RBracket) | None => Err(self.error("expected array size")),
            _ => Ok(ArrayLen::Const(Box::new(self.parse_expression()?))),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParserError> {
        match self.peek() {
            Some(Token::Ident(name)) => {
                let out = (*name).to_owned();
                self.advance();
                Ok(out)
            }
            Some(tok) => Err(self.error_with_token("expected identifier", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_integer(&mut self) -> Result<i64, ParserError> {
        match self.peek() {
            Some(Token::Integer(text)) => {
                let value = strip_separators(text)
                    .parse::<i64>()
                    .map_err(|_| self.error("invalid integer literal"))?;
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected integer literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_hex_integer(&mut self) -> Result<i64, ParserError> {
        match self.peek() {
            Some(Token::HexInteger(text)) => {
                let digits = strip_separators(text);
                let trimmed = digits.trim_start_matches("0x").trim_start_matches("0X");
                // Parse as u64 first to handle large addresses, then cast to i64
                let value_u64 = u64::from_str_radix(trimmed, 16)
                    .map_err(|_| self.error("invalid hex literal"))?;
                let value = crate::conv::u64_bits_to_i64(value_u64);
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected hex integer literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_bin_integer(&mut self) -> Result<i64, ParserError> {
        match self.peek() {
            Some(Token::BinInteger(text)) => {
                let digits = strip_separators(text);
                let trimmed = digits.trim_start_matches("0b").trim_start_matches("0B");
                // Parse as u64 first so a full-width mask keeps its bit pattern.
                let value_u64 = u64::from_str_radix(trimmed, 2)
                    .map_err(|_| self.error("invalid binary literal"))?;
                let value = crate::conv::u64_bits_to_i64(value_u64);
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected binary integer literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_float(&mut self) -> Result<f64, ParserError> {
        match self.peek() {
            Some(Token::Float(text)) => {
                let value = strip_separators(text)
                    .parse::<f64>()
                    .map_err(|_| self.error("invalid float literal"))?;
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected float literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_assign(&mut self) -> Result<(), ParserError> {
        if self.match_assign() {
            Ok(())
        } else {
            Err(self.error("expected `=`"))
        }
    }

    fn expect_colon(&mut self) -> Result<(), ParserError> {
        if self.match_colon() {
            Ok(())
        } else {
            Err(self.error("expected `:`"))
        }
    }

    fn expect_colon_equal(&mut self) -> Result<(), ParserError> {
        if self.match_colon_equal() {
            Ok(())
        } else {
            Err(self.error("expected `:=`"))
        }
    }

    fn expect_lparen(&mut self) -> Result<(), ParserError> {
        if self.match_lparen() {
            Ok(())
        } else {
            Err(self.error("expected `(`"))
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ParserError> {
        if self.match_rparen() {
            Ok(())
        } else {
            Err(self.error("expected `)`"))
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), ParserError> {
        if self.match_lbrace() {
            Ok(())
        } else {
            Err(self.error("expected `{`"))
        }
    }

    fn expect_rbrace(&mut self) -> Result<(), ParserError> {
        if self.match_rbrace() {
            Ok(())
        } else {
            Err(self.error("expected `}`"))
        }
    }

    fn expect_rbracket(&mut self) -> Result<(), ParserError> {
        if self.match_rbracket() {
            Ok(())
        } else {
            Err(self.error("expected `]`"))
        }
    }

    fn expect_if(&mut self) -> Result<(), ParserError> {
        if self.match_if() {
            Ok(())
        } else {
            Err(self.error("expected `if`"))
        }
    }

    fn expect_while(&mut self) -> Result<(), ParserError> {
        if self.match_while() {
            Ok(())
        } else {
            Err(self.error("expected `while`"))
        }
    }

    fn expect_return(&mut self) -> Result<(), ParserError> {
        if self.match_return() {
            Ok(())
        } else {
            Err(self.error("expected `return`"))
        }
    }

    fn expect_for(&mut self) -> Result<(), ParserError> {
        if self.match_for() {
            Ok(())
        } else {
            Err(self.error("expected `for`"))
        }
    }

    fn expect_defer(&mut self) -> Result<(), ParserError> {
        if self.match_defer() {
            Ok(())
        } else {
            Err(self.error("expected `defer`"))
        }
    }

    fn is_declaration_start(&self) -> bool {
        match self.peek() {
            Some(
                Token::Const
                | Token::Type
                | Token::Struct
                | Token::Enum
                | Token::Interface
                | Token::External
                | Token::Export,
            ) => true,
            Some(Token::Ident(_)) => {
                self.peek_n(1) == Some(&Token::Colon) || self.peek_n(1) == Some(&Token::ColonEqual)
            }
            _ => false,
        }
    }

    fn is_expression_terminator(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::StatementTerminator | Token::Eof) | None
        )
    }

    fn check_if(&self) -> bool {
        matches!(self.peek(), Some(Token::If))
    }

    fn check_lbrace(&self) -> bool {
        matches!(self.peek(), Some(Token::LBrace))
    }

    fn check_rbrace(&self) -> bool {
        matches!(self.peek(), Some(Token::RBrace))
    }

    fn check_rparen(&self) -> bool {
        matches!(self.peek(), Some(Token::RParen))
    }

    fn check_rbracket(&self) -> bool {
        matches!(self.peek(), Some(Token::RBracket))
    }

    fn check_gt_or_shr(&self) -> bool {
        // Used in type contexts where `>>` can close nested generics
        // Also check for pending virtual > from a split >>
        if self.pending_gt_from_shr {
            return true;
        }
        matches!(self.peek(), Some(Token::Gt | Token::Shr | Token::Gte))
    }

    fn match_assign(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Assign))
    }

    // Consume a compound-assign operator (`+=`, `<<=`, ...) and map it to the
    // matching binary operator, or return None and consume nothing.
    fn match_compound_assign(&mut self) -> Option<BinaryOp> {
        let op = match self.peek() {
            Some(Token::CompoundAssign(op)) => *op,
            _ => return None,
        };
        self.advance();
        Some(match op {
            CompoundOp::Add => BinaryOp::Add,
            CompoundOp::Sub => BinaryOp::Sub,
            CompoundOp::Mul => BinaryOp::Mul,
            CompoundOp::Div => BinaryOp::Div,
            CompoundOp::Mod => BinaryOp::Mod,
            CompoundOp::BitAnd => BinaryOp::BitwiseAnd,
            CompoundOp::BitOr => BinaryOp::BitwiseOr,
            CompoundOp::BitXor => BinaryOp::BitwiseXor,
            CompoundOp::Shl => BinaryOp::Shl,
            CompoundOp::Shr => BinaryOp::Shr,
        })
    }

    fn match_colon(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Colon))
    }

    fn match_colon_equal(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::ColonEqual))
    }

    fn match_lparen(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::LParen))
    }

    fn parse_primary(&mut self) -> Result<Expression, ParserError> {
        let expr = match self.peek() {
            Some(Token::Fn) => {
                return Err(self.error(
                    "lambda expressions omit `fn`; write `(name: Type) -> Return { ... }`",
                ));
            }
            Some(Token::Integer(_)) => Expression::Primary(PrimaryExpr::Literal(Literal::Integer(
                self.expect_integer()?,
            ))),
            // A char literal is just its ascii byte as an integer literal.
            Some(Token::Char(value)) => {
                let v = *value as i64;
                self.advance();
                Expression::Primary(PrimaryExpr::Literal(Literal::Integer(v)))
            }
            Some(Token::HexInteger(_)) => Expression::Primary(PrimaryExpr::Literal(
                Literal::HexInteger(self.expect_hex_integer()?),
            )),
            Some(Token::BinInteger(_)) => Expression::Primary(PrimaryExpr::Literal(
                Literal::BinInteger(self.expect_bin_integer()?),
            )),
            Some(Token::Float(_)) => {
                Expression::Primary(PrimaryExpr::Literal(Literal::Float(self.expect_float()?)))
            }
            Some(Token::True) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Literal(Literal::Boolean(true)))
            }
            Some(Token::False) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Literal(Literal::Boolean(false)))
            }
            Some(Token::Null) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Literal(Literal::Null))
            }
            Some(Token::String(text)) => {
                if text.len() < 2 || !text.starts_with('"') || !text.ends_with('"') {
                    return Err(self.error("invalid string token"));
                }
                let content = &text[1..text.len() - 1];
                let processed = self.process_string_escapes(content);
                self.advance();
                Expression::Primary(PrimaryExpr::Literal(Literal::String(processed)))
            }
            Some(Token::Ident(name)) => {
                let id = name.to_string();
                self.advance();
                if self.type_names.contains(&id) && self.check_lbrace() {
                    let (fields, rest) = self.parse_struct_literal_fields()?;
                    Expression::Primary(PrimaryExpr::NamedStructLiteral {
                        name: id,
                        fields,
                        rest,
                    })
                } else {
                    Expression::Primary(PrimaryExpr::Identifier(id))
                }
            }
            Some(Token::I8) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("i8".to_owned()))
            }
            Some(Token::I16) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("i16".to_owned()))
            }
            Some(Token::I32) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("i32".to_owned()))
            }
            Some(Token::I64) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("i64".to_owned()))
            }
            Some(Token::U8) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("u8".to_owned()))
            }
            Some(Token::U16) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("u16".to_owned()))
            }
            Some(Token::U32) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("u32".to_owned()))
            }
            Some(Token::U64) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("u64".to_owned()))
            }
            Some(Token::F32) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("f32".to_owned()))
            }
            Some(Token::F64) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("f64".to_owned()))
            }
            Some(Token::Bool) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("bool".to_owned()))
            }
            Some(Token::LParen) => {
                let mut trial = self.clone();
                if let Ok(lambda) = trial.parse_lambda_expression() {
                    *self = trial;
                    lambda
                } else {
                    self.advance();
                    let expr = self.parse_expression()?;
                    self.expect_rparen()?;
                    Expression::Primary(PrimaryExpr::Grouped(Box::new(expr)))
                }
            }
            Some(Token::LBracket) => {
                self.advance();
                let mut elements = Vec::new();
                self.consume_terminators(); // after '['
                if !self.check_rbracket() {
                    loop {
                        self.consume_terminators(); // before element
                        elements.push(self.parse_expression()?);
                        self.consume_terminators(); // after element
                        if self.match_comma() {
                            self.consume_terminators(); // after comma
                            if self.check_rbracket() {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.consume_terminators(); // before ']'
                self.expect_rbracket()?;
                Expression::Primary(PrimaryExpr::ArrayLiteral(elements))
            }
            Some(Token::LBrace) => {
                let (fields, rest) = self.parse_struct_literal_fields()?;
                Expression::Primary(PrimaryExpr::StructLiteral { fields, rest })
            }
            Some(Token::New) => {
                self.advance();
                self.expect_lparen()?;
                let ty = self.parse_type()?;
                let mut args = Vec::new();
                self.consume_terminators(); // after '('
                if self.match_comma() && !self.check_rparen() {
                    loop {
                        self.consume_terminators(); // before arg
                        args.push(self.parse_expression()?);
                        self.consume_terminators(); // after arg
                        if self.match_comma() {
                            self.consume_terminators(); // after comma
                            if self.check_rparen() {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.consume_terminators(); // before ')'
                self.expect_rparen()?;
                Expression::Primary(PrimaryExpr::New { ty, args })
            }
            Some(Token::Match) => {
                return self.parse_match_expression();
            }
            Some(tok) => {
                return Err(self.error_with_token("unexpected token in primary expression", tok));
            }
            None => return Err(self.error("unexpected end of input")),
        };
        Ok(expr)
    }

    /// Parse `alias.format_into(dest, "a{x}b")` (spec 5.5), called after `(`.
    /// None (state restored) when the shape does not match the string form.
    fn try_parse_format_into(
        &mut self,
        callee: &Expression,
    ) -> Result<Option<Expression>, ParserError> {
        let Expression::Primary(PrimaryExpr::FieldAccess { expr: base, field }) = callee else {
            return Ok(None);
        };
        if field != "format_into" {
            return Ok(None);
        }
        let Expression::Primary(PrimaryExpr::Identifier(alias)) = base.as_ref() else {
            return Ok(None);
        };
        let saved_pos = self.pos;
        let saved_synth = self.synthesized_declarations.len();
        let saved_lambda = self.next_lambda_id;
        let restore = |parser: &mut Self| {
            parser.pos = saved_pos;
            parser.synthesized_declarations.truncate(saved_synth);
            parser.next_lambda_id = saved_lambda;
        };
        self.consume_terminators();
        let Ok(dest) = self.parse_expression() else {
            restore(self);
            return Ok(None);
        };
        if !self.match_comma() {
            restore(self);
            return Ok(None);
        }
        self.consume_terminators();
        let Some(Token::String(raw)) = self.tokens.get(self.pos) else {
            restore(self);
            return Ok(None);
        };
        let raw: &'a str = raw;
        if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
            return Err(self.error("invalid string token"));
        }
        // Committed: destination + comma + string literal is the interpolation
        // form, so everything from here on is a hard error.
        let template_span = self.spans[self.pos].clone();
        self.advance();
        self.match_comma();
        self.consume_terminators();
        self.expect_rparen()?;
        let segments = self.parse_format_segments(raw, &template_span)?;
        Ok(Some(Expression::Primary(PrimaryExpr::FormatInto {
            alias: alias.clone(),
            dest: Box::new(dest),
            segments,
        })))
    }

    /// Split a raw string-literal token (quotes included) into literal chunks
    /// and interpolation holes; holes are lexed in place so spans stay real.
    fn parse_format_segments(
        &mut self,
        raw: &'a str,
        span: &Span,
    ) -> Result<Vec<FormatSegment>, ParserError> {
        let content = &raw[1..raw.len() - 1];
        let error_at = |offset: usize, len: u32, message: &str| ParserError {
            message: message.to_owned(),
            span: Span {
                line: span.line,
                col: span.col + 1 + offset as u32,
                len,
                source_line: span.source_line.clone(),
            },
        };
        let bytes = content.as_bytes();
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut piece_start = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'{' if bytes.get(i + 1) == Some(&b'{') => {
                    literal.push_str(&self.process_string_escapes(&content[piece_start..i]));
                    literal.push('{');
                    i += 2;
                    piece_start = i;
                }
                b'}' if bytes.get(i + 1) == Some(&b'}') => {
                    literal.push_str(&self.process_string_escapes(&content[piece_start..i]));
                    literal.push('}');
                    i += 2;
                    piece_start = i;
                }
                b'}' => {
                    return Err(error_at(
                        i,
                        1,
                        "unmatched `}` in format string; use `}}` for a literal brace",
                    ));
                }
                b'{' => {
                    literal.push_str(&self.process_string_escapes(&content[piece_start..i]));
                    if !literal.is_empty() {
                        segments.push(FormatSegment::Literal(std::mem::take(&mut literal)));
                    }
                    let hole_start = i + 1;
                    let mut j = hole_start;
                    loop {
                        match bytes.get(j) {
                            None => {
                                return Err(error_at(
                                    i,
                                    1,
                                    "unterminated `{` in format string; use `{{` for a literal brace",
                                ));
                            }
                            Some(b'{') => {
                                return Err(error_at(
                                    j,
                                    1,
                                    "nested `{` in a format string interpolation",
                                ));
                            }
                            Some(b'\n') => {
                                return Err(error_at(
                                    j,
                                    1,
                                    "format string interpolations must be single-line",
                                ));
                            }
                            Some(b'}') => break,
                            Some(_) => j += 1,
                        }
                    }
                    if j == hole_start {
                        return Err(error_at(i, 2, "empty format string interpolation"));
                    }
                    let expr =
                        self.parse_hole_expression(&content[hole_start..j], span, hole_start)?;
                    segments.push(FormatSegment::Hole(expr));
                    i = j + 1;
                    piece_start = i;
                }
                _ => i += 1,
            }
        }
        literal.push_str(&self.process_string_escapes(&content[piece_start.min(content.len())..]));
        if !literal.is_empty() {
            segments.push(FormatSegment::Literal(literal));
        }
        Ok(segments)
    }

    /// Lex and parse one interpolation hole from its original source slice,
    /// shifting sub-token spans to the hole's true position in the line.
    fn parse_hole_expression(
        &mut self,
        hole_src: &'a str,
        span: &Span,
        offset: usize,
    ) -> Result<Expression, ParserError> {
        let mut token_spans = Lexer::tokenize(hole_src);
        for (_, s) in &mut token_spans {
            s.line = span.line;
            s.col = span.col + 1 + offset as u32 + s.col.saturating_sub(1);
            s.source_line = span.source_line.clone();
        }
        let mut sub = Parser::new_with_spans(token_spans);
        sub.type_names = self.type_names.clone();
        sub.enum_names = self.enum_names.clone();
        sub.next_lambda_id = self.next_lambda_id;
        let expr = sub.parse_expression()?;
        if !matches!(sub.peek(), Some(Token::Eof) | None) {
            return Err(sub.error("unexpected trailing tokens in format string interpolation"));
        }
        self.next_lambda_id = sub.next_lambda_id;
        self.synthesized_declarations
            .append(&mut sub.synthesized_declarations);
        Ok(expr)
    }

    fn parse_lambda_expression(&mut self) -> Result<Expression, ParserError> {
        let params = self.parse_param_list()?;
        let return_type = if self.match_arrow() {
            Some(self.parse_return_type()?)
        } else {
            None
        };
        if !self.check_lbrace() {
            return Err(self.error("expected lambda body block"));
        }
        let body = self.parse_block()?;
        let name = format!("__hll_lambda_{}", self.next_lambda_id);
        self.next_lambda_id += 1;
        self.synthesized_declarations.push(Declaration {
            exported: false,
            decl: DeclNode::Function {
                name: name.clone(),
                generics: Vec::new(),
                bounds: Vec::new(),
                params,
                return_type,
                body: Some(body),
                is_extern: false,
                is_import_interface: false,
            },
        });
        Ok(Expression::Primary(PrimaryExpr::Identifier(name)))
    }

    fn parse_match_expression(&mut self) -> Result<Expression, ParserError> {
        self.advance(); // consume `match`
        let scrutinee = self.parse_expression()?;
        self.expect_lbrace()?;
        let mut arms = Vec::new();
        self.consume_terminators();

        while !self.check_rbrace() {
            let pattern = self.parse_pattern()?;
            if !self.match_arrow() {
                return Err(self.error("expected `->` after a match pattern"));
            }
            // `Pattern -> { ... }` is a statement arm; `Pattern -> expr` is a value
            // arm whose expression becomes the match's value in that case.
            let (body, value) = if self.check_lbrace() {
                (self.parse_block()?, None)
            } else {
                (
                    Block {
                        statements: Vec::new(),
                        stmt_lines: Vec::new(),
                    },
                    Some(self.parse_expression()?),
                )
            };
            arms.push(MatchArm {
                pattern,
                body,
                value,
            });
            self.consume_terminators();
            // Arms may be comma-separated; the block boundary already separates them.
            if self.match_comma() {
                self.consume_terminators();
            }
        }

        self.expect_rbrace()?;
        Ok(Expression::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    // A match pattern: `_`, a bare binding (lowercase), or `Variant(b0, b1)` /
    // bare `Variant` (capitalized). `_` discards a payload slot.
    fn parse_pattern(&mut self) -> Result<Pattern, ParserError> {
        // Integer/char literals (optionally negated) match a scalar scrutinee by
        // value; everything else is an identifier-led variant or binding pattern.
        if let Some(lit) = self.try_parse_literal_pattern()? {
            return Ok(Pattern::Literal(lit));
        }

        let name = self.expect_ident()?;
        if name == "_" {
            return Ok(Pattern::Wildcard);
        }

        // `Enum.Variant` qualifies the arm; a bare variant resolves against the
        // scrutinee's enum instead.
        let (enum_name, name) = if self.enum_names.contains(&name) && self.match_dot() {
            (Some(name), self.expect_ident()?)
        } else {
            (None, name)
        };

        let starts_upper = name.chars().next().is_some_and(char::is_uppercase);
        if !matches!(self.peek(), Some(Token::LParen)) {
            return Ok(if starts_upper || enum_name.is_some() {
                Pattern::Variant {
                    enum_name,
                    variant: name,
                    bindings: Vec::new(),
                }
            } else {
                Pattern::Binding(name)
            });
        }

        self.expect_lparen()?;
        let mut bindings = Vec::new();
        if !self.check_rparen() {
            loop {
                bindings.push(self.expect_ident()?);
                if self.match_comma() {
                    if self.check_rparen() {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect_rparen()?;
        Ok(Pattern::Variant {
            enum_name,
            variant: name,
            bindings,
        })
    }

    // Parse a leading int/char literal pattern (optional `-`), else None so the
    // caller falls through to identifier-led patterns. Char reuses ascii-byte form.
    fn try_parse_literal_pattern(&mut self) -> Result<Option<Literal>, ParserError> {
        match self.peek() {
            Some(Token::Integer(_)) => Ok(Some(Literal::Integer(self.expect_integer()?))),
            Some(Token::HexInteger(_)) => Ok(Some(Literal::HexInteger(self.expect_hex_integer()?))),
            Some(Token::BinInteger(_)) => Ok(Some(Literal::BinInteger(self.expect_bin_integer()?))),
            Some(Token::Char(value)) => {
                let v = *value as i64;
                self.advance();
                Ok(Some(Literal::Integer(v)))
            }
            Some(Token::Minus) => {
                self.advance();
                match self.peek() {
                    Some(Token::Integer(_)) => Ok(Some(Literal::Integer(-self.expect_integer()?))),
                    Some(Token::HexInteger(_)) => {
                        Ok(Some(Literal::HexInteger(-self.expect_hex_integer()?)))
                    }
                    Some(Token::BinInteger(_)) => {
                        Ok(Some(Literal::BinInteger(-self.expect_bin_integer()?)))
                    }
                    _ => Err(self.error("expected integer literal after `-` in match pattern")),
                }
            }
            _ => Ok(None),
        }
    }

    // Canonical field init is `.name = expr` so `:` always introduces a type. A
    // trailing `..` zero-fills the fields the literal leaves unnamed.
    fn parse_struct_literal_fields(&mut self) -> Result<(Vec<FieldInit>, bool), ParserError> {
        self.expect_lbrace()?;
        let mut fields = Vec::new();
        let mut rest = false;
        self.consume_terminators();
        while !self.check_rbrace() {
            if self.match_dot_dot() {
                rest = true;
                self.consume_terminators();
                if !self.check_rbrace() {
                    return Err(self.error("`..` must be the last entry in a struct literal"));
                }
                break;
            }
            if !self.match_dot() {
                return Err(self.error("struct literal fields use `.name = expr`"));
            }
            let name = self.expect_ident()?;
            self.expect_assign()?;
            let expr = self.parse_expression()?;
            fields.push(FieldInit {
                name,
                ty: None,
                expr,
            });

            self.consume_terminators();
            if self.match_comma() {
                self.consume_terminators();
                continue;
            }
            if !self.check_rbrace() {
                return Err(self.error("expected `,` between struct literal fields"));
            }
        }
        self.expect_rbrace()?;
        Ok((fields, rest))
    }

    fn match_rparen(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::RParen))
    }

    fn match_lbrace(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::LBrace))
    }

    fn match_rbrace(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::RBrace))
    }

    fn match_lbracket(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::LBracket))
    }

    fn match_rbracket(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::RBracket))
    }

    fn match_comma(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Comma))
    }

    fn match_dot(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Dot))
    }

    fn match_plus(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Plus))
    }

    fn match_minus(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Minus))
    }

    fn match_arrow(&mut self) -> bool {
        self.match_minus() && self.match_gt()
    }

    fn match_star(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Star))
    }

    fn match_slash(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Slash))
    }

    fn match_percent(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Percent))
    }

    fn match_or(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Or))
    }

    fn match_and(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::And))
    }

    fn match_eq(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Eq))
    }

    fn match_neq(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Neq))
    }

    fn match_lt(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Lt))
    }

    fn match_lte(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Lte))
    }

    fn match_shl(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Shl))
    }

    fn match_shr(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Shr))
    }

    fn match_gt(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Gt))
    }

    fn match_gte(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Gte))
    }

    fn match_not(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Not))
    }

    fn match_ampersand(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Ampersand))
    }

    fn match_tilde(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Tilde))
    }

    fn match_bitwise_and(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Ampersand))
    }

    fn match_bitwise_or(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Pipe))
    }

    fn match_bitwise_xor(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Caret))
    }

    fn match_at(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::At))
    }

    fn match_if(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::If))
    }

    fn match_else(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Else))
    }

    fn match_while(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::While))
    }

    fn match_for(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::For))
    }

    fn match_in(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::In))
    }

    fn match_dot_dot(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::DotDot))
    }

    fn match_dot_dot_eq(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::DotDotEq))
    }

    fn match_return(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Return))
    }

    fn match_defer(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Defer))
    }

    fn match_statement_terminator(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::StatementTerminator))
    }

    fn expect_string_literal(&mut self) -> Result<String, ParserError> {
        match self.peek() {
            Some(Token::String(text)) => {
                if text.len() < 2 || !text.starts_with('"') || !text.ends_with('"') {
                    return Err(self.error("invalid string literal"));
                }
                let content = text[1..text.len() - 1].to_owned();
                self.advance();
                Ok(content)
            }
            Some(tok) => Err(self.error_with_token("expected string literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn match_variant<F>(&mut self, predicate: F) -> bool
    where
        F: FnOnce(&Token<'a>) -> bool,
    {
        if let Some(tok) = self.peek()
            && predicate(tok)
        {
            self.advance();
            return true;
        }
        false
    }

    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn peek_n(&self, n: usize) -> Option<&Token<'a>> {
        self.tokens.get(self.pos + n)
    }

    fn advance(&mut self) -> Option<Token<'a>> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek(), Some(Token::Eof) | None)
    }

    fn current_span(&self) -> Span {
        let idx = self.pos.min(self.spans.len().saturating_sub(1));
        self.spans.get(idx).cloned().unwrap_or_default()
    }

    fn error(&self, message: &str) -> ParserError {
        ParserError {
            message: message.to_owned(),
            span: self.current_span(),
        }
    }

    fn error_with_token(&self, message: &str, token: &Token<'a>) -> ParserError {
        ParserError {
            message: format!("{message}: {token:?}"),
            span: self.current_span(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::ast::{
        ArrayLen, AssignTarget, DeclNode, Expression, ForIter, Literal, Path, Pattern, PrimaryExpr,
        ReturnType, Statement, Type,
    };
    use crate::lexer::Lexer;
    use crate::token::Token;

    #[test]
    fn lexer_emits_colon_equal_as_one_token() {
        let tokens: Vec<_> = Lexer::tokenize("value := 42")
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("value"),
                Token::ColonEqual,
                Token::Integer("42"),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn parses_inferred_binding_as_declaration() {
        let tokens = vec![
            Token::Ident("value"),
            Token::ColonEqual,
            Token::Integer("42"),
            Token::Eof,
        ];
        let mut parser = Parser::new(tokens);
        match parser.parse_statement().unwrap() {
            Statement::InferredVariableDecl { name, init } => {
                assert_eq!(name, "value");
                assert!(matches!(
                    init,
                    Expression::Primary(crate::ast::PrimaryExpr::Literal(
                        crate::ast::Literal::Integer(42)
                    ))
                ));
            }
            other => panic!("expected inferred declaration, got {other:?}"),
        }
    }

    #[test]
    fn lexer_tokenizes_range_operators_not_as_floats() {
        let tokens: Vec<_> = Lexer::tokenize("0..5 1..=4")
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Token::Integer("0"),
                Token::DotDot,
                Token::Integer("5"),
                Token::Integer("1"),
                Token::DotDotEq,
                Token::Integer("4"),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lexer_still_tokenizes_floats() {
        let tokens: Vec<_> = Lexer::tokenize("3.14 0.5")
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        assert_eq!(
            tokens,
            vec![Token::Float("3.14"), Token::Float("0.5"), Token::Eof],
        );
    }

    #[test]
    fn parses_for_range_loop() {
        let tokens = Lexer::tokenize("for i in 0..5 {\nx = x + i\n}")
            .into_iter()
            .map(|(token, _)| token)
            .collect::<Vec<_>>();
        let mut parser = Parser::new(tokens);
        match parser.parse_statement().unwrap() {
            Statement::For { var, iter, body } => {
                assert_eq!(var, "i");
                assert!(matches!(
                    iter,
                    ForIter::Range {
                        inclusive: false,
                        ..
                    }
                ));
                assert_eq!(body.statements.len(), 1);
            }
            other => panic!("expected for loop, got {other:?}"),
        }
    }

    #[test]
    fn parses_inclusive_for_range_loop() {
        let tokens = Lexer::tokenize("for i in 1..=4 {\n}")
            .into_iter()
            .map(|(token, _)| token)
            .collect::<Vec<_>>();
        let mut parser = Parser::new(tokens);
        match parser.parse_statement().unwrap() {
            Statement::For { iter, .. } => assert!(matches!(
                iter,
                ForIter::Range {
                    inclusive: true,
                    ..
                }
            )),
            other => panic!("expected inclusive for loop, got {other:?}"),
        }
    }

    #[test]
    fn parses_range_slice_and_plain_index() {
        // `arr[a..b]` parses to a Slice node; `arr[i]` stays an ArrayIndex.
        let tokens = Lexer::tokenize("y = arr[1..4]\nz = arr[2]")
            .into_iter()
            .map(|(token, _)| token)
            .collect::<Vec<_>>();
        let mut parser = Parser::new(tokens);
        let Statement::Expression(Expression::Assignment { rvalue: slice, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected assignment");
        };
        assert!(matches!(
            *slice,
            Expression::Primary(PrimaryExpr::Slice {
                start: Some(_),
                end: Some(_),
                inclusive: false,
                ..
            })
        ));
        let Statement::Expression(Expression::Assignment { rvalue: index, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected assignment");
        };
        assert!(matches!(
            *index,
            Expression::Primary(PrimaryExpr::ArrayIndex { .. })
        ));
    }

    #[test]
    fn parses_open_ended_range_slices() {
        // Open start and open end both parse with the missing endpoint as None.
        for (src, has_start, has_end) in [
            ("y = arr[..2]", false, true),
            ("y = arr[3..]", true, false),
            ("y = arr[..]", false, false),
        ] {
            let tokens = Lexer::tokenize(src)
                .into_iter()
                .map(|(token, _)| token)
                .collect::<Vec<_>>();
            let mut parser = Parser::new(tokens);
            let Statement::Expression(Expression::Assignment { rvalue, .. }) =
                parser.parse_statement().unwrap()
            else {
                panic!("expected assignment for `{src}`");
            };
            match *rvalue {
                Expression::Primary(PrimaryExpr::Slice { start, end, .. }) => {
                    assert_eq!(start.is_some(), has_start, "start for `{src}`");
                    assert_eq!(end.is_some(), has_end, "end for `{src}`");
                }
                other => panic!("expected slice for `{src}`, got {other:?}"),
            }
        }
    }

    #[test]
    fn parses_for_each_loop() {
        let tokens = Lexer::tokenize("for x in items {\n}")
            .into_iter()
            .map(|(token, _)| token)
            .collect::<Vec<_>>();
        let mut parser = Parser::new(tokens);
        match parser.parse_statement().unwrap() {
            Statement::For { var, iter, .. } => {
                assert_eq!(var, "x");
                assert!(matches!(iter, ForIter::Each(_)));
            }
            other => panic!("expected for-each loop, got {other:?}"),
        }
    }

    #[test]
    fn rejects_explicit_void_return_type() {
        let tokens = vec![
            Token::Ident("noop"),
            Token::Colon,
            Token::LParen,
            Token::RParen,
            Token::Minus,
            Token::Gt,
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ];
        let error = Parser::new(tokens).parse_program().unwrap_err();
        assert!(error.message.contains("omit `->`"));
    }

    #[test]
    fn parses_named_struct_literal() {
        let source = "struct Point { x: i32 }\nvalue := Point { .x = 1 }";
        let mut parser = Parser::new_with_spans(Lexer::tokenize(source));
        let program = parser.parse_program().unwrap();
        match &program.declarations[1].decl {
            DeclNode::InferredVariable { init, .. } => assert!(matches!(
                init,
                Expression::Primary(crate::ast::PrimaryExpr::NamedStructLiteral {
                    name,
                    fields,
                    ..
                }) if name == "Point" && fields.len() == 1 && fields[0].ty.is_none()
            )),
            other => panic!("expected inferred named literal, got {other:?}"),
        }
    }

    #[test]
    fn parses_pointer_cast_syntax() {
        let tokens = vec![
            Token::I8,
            Token::Star,
            Token::LParen,
            Token::Ident("int_ptr"),
            Token::RParen,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        match parser.parse_expression().unwrap() {
            Expression::Cast { target_ty, expr } => {
                assert!(
                    matches!(&target_ty, Type::Pointer(inner) if matches!(inner.as_ref(), Type::Primitive(name) if name == "i8"))
                );
                match expr.as_ref() {
                    Expression::Primary(crate::ast::PrimaryExpr::Identifier(name)) => {
                        assert_eq!(name, "int_ptr");
                    }
                    other => panic!("expected identifier in cast, got: {other:?}"),
                }
            }
            other => panic!("expected Cast expression, got: {other:?}"),
        }
    }

    #[test]
    fn compound_assign_desugars_to_binary() {
        use crate::ast::{BinaryOp, Literal, PrimaryExpr};
        use crate::token::CompoundOp;
        // `x += 1` parses as `x = x + 1`.
        let tokens = vec![
            Token::Ident("x"),
            Token::CompoundAssign(CompoundOp::Add),
            Token::Integer("1"),
            Token::Eof,
        ];
        let mut parser = Parser::new(tokens);
        match parser.parse_expression().unwrap() {
            Expression::Assignment { target, rvalue } => {
                assert!(matches!(target.as_ref(), AssignTarget::Identifier(n) if n == "x"));
                match rvalue.as_ref() {
                    Expression::Binary { op, left, right } => {
                        assert!(matches!(op, BinaryOp::Add));
                        assert!(matches!(left.as_ref(),
                            Expression::Primary(PrimaryExpr::Identifier(n)) if n == "x"));
                        assert!(matches!(
                            right.as_ref(),
                            Expression::Primary(PrimaryExpr::Literal(Literal::Integer(_)))
                        ));
                    }
                    other => panic!("expected Binary rvalue, got: {other:?}"),
                }
            }
            other => panic!("expected Assignment, got: {other:?}"),
        }
    }

    #[test]
    fn parses_variable_declaration() {
        let tokens = vec![
            Token::Ident("x"),
            Token::Colon,
            Token::I32,
            Token::Assign,
            Token::Integer("42"),
            Token::StatementTerminator,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        assert_eq!(program.declarations.len(), 1);
        match &program.declarations[0].decl {
            DeclNode::Variable { name, .. } => assert_eq!(name, "x"),
            other => panic!("unexpected declaration: {other:?}"),
        }
    }

    #[test]
    fn parses_function_declaration() {
        let tokens = vec![
            Token::Ident("main"),
            Token::Colon,
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::Return,
            Token::StatementTerminator,
            Token::RBrace,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        assert_eq!(program.declarations.len(), 1);
        match &program.declarations[0].decl {
            DeclNode::Function { name, body, .. } => {
                assert_eq!(name, "main");
                assert!(body.is_some());
            }
            other => panic!("unexpected declaration: {other:?}"),
        }
    }

    #[test]
    fn parses_external_function_declaration() {
        let tokens = vec![
            Token::External,
            Token::Ident("print"),
            Token::Colon,
            Token::LParen,
            Token::Ident("value"),
            Token::Colon,
            Token::I32,
            Token::RParen,
            Token::Minus,
            Token::Gt,
            Token::I32,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        match &program.declarations[0].decl {
            DeclNode::Function {
                is_extern, body, ..
            } => {
                assert!(*is_extern);
                assert!(body.is_none());
            }
            other => panic!("unexpected declaration: {other:?}"),
        }
    }

    #[test]
    fn parses_struct_return_function_signature() {
        let tokens = vec![
            Token::Ident("divide"),
            Token::Colon,
            Token::LParen,
            Token::Ident("a"),
            Token::Colon,
            Token::I32,
            Token::Comma,
            Token::Ident("b"),
            Token::Colon,
            Token::I32,
            Token::RParen,
            Token::Minus,
            Token::Gt,
            Token::LBrace,
            Token::Ident("quotient"),
            Token::Colon,
            Token::I32,
            Token::Comma,
            Token::Ident("remainder"),
            Token::Colon,
            Token::I32,
            Token::RBrace,
            Token::LBrace,
            Token::Return,
            Token::LBrace,
            Token::Dot,
            Token::Ident("quotient"),
            Token::Assign,
            Token::Ident("a"),
            Token::Slash,
            Token::Ident("b"),
            Token::Comma,
            Token::Dot,
            Token::Ident("remainder"),
            Token::Assign,
            Token::Ident("a"),
            Token::Percent,
            Token::Ident("b"),
            Token::RBrace,
            Token::StatementTerminator,
            Token::RBrace,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        match &program.declarations[0].decl {
            DeclNode::Function { return_type, .. } => match return_type {
                Some(ReturnType::Single(Type::Struct(fields))) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "quotient");
                    assert_eq!(fields[1].name, "remainder");
                }
                other => panic!("unexpected return type: {other:?}"),
            },
            other => panic!("unexpected declaration: {other:?}"),
        }
    }

    #[test]
    fn parses_if_else_and_while_statement() {
        let if_tokens = vec![
            Token::If,
            Token::True,
            Token::LBrace,
            Token::Break,
            Token::RBrace,
            Token::Else,
            Token::LBrace,
            Token::Continue,
            Token::RBrace,
            Token::Eof,
        ];
        let mut if_parser = Parser::new(if_tokens);
        match if_parser.parse_statement().unwrap() {
            Statement::If { else_branch, .. } => match else_branch {
                Some(branch) => assert!(matches!(*branch, Statement::Block(_))),
                None => panic!("expected else branch"),
            },
            other => panic!("unexpected statement: {other:?}"),
        }

        let while_tokens = vec![
            Token::While,
            Token::False,
            Token::LBrace,
            Token::Return,
            Token::StatementTerminator,
            Token::RBrace,
            Token::Eof,
        ];
        let mut while_parser = Parser::new(while_tokens);
        match while_parser.parse_statement().unwrap() {
            Statement::While { body, .. } => assert_eq!(body.statements.len(), 1),
            other => panic!("unexpected statement: {other:?}"),
        }
    }

    #[test]
    fn parses_struct_destructuring_assignment() {
        let tokens = vec![
            Token::LBrace,
            Token::Ident("q"),
            Token::Colon,
            Token::I32,
            Token::Comma,
            Token::Ident("r"),
            Token::Colon,
            Token::I32,
            Token::RBrace,
            Token::Assign,
            Token::Ident("divide"),
            Token::LParen,
            Token::Integer("10"),
            Token::Comma,
            Token::Integer("3"),
            Token::RParen,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        match parser.parse_expression().unwrap() {
            Expression::Assignment { target, .. } => match *target {
                AssignTarget::StructDestructure(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, Some("q".to_string()));
                    assert_eq!(fields[1].name, Some("r".to_string()));
                    assert!(matches!(&fields[0].ty, Some(Type::Primitive(n)) if n == "i32"));
                    assert!(matches!(&fields[1].ty, Some(Type::Primitive(n)) if n == "i32"));
                }
                other => panic!("unexpected assignment target: {other:?}"),
            },
            other => panic!("unexpected expression: {other:?}"),
        }
    }

    #[test]
    fn parses_name_only_destructuring_binding() {
        let mut parser = Parser::new(tokens_of("{ quo, rem = remainder } := pair"));
        let statement = parser
            .parse_statement()
            .expect("binding pattern should parse");
        let Statement::DestructureBinding { fields, .. } = statement else {
            panic!("expected destructuring binding");
        };
        assert_eq!(
            fields,
            vec![
                crate::ast::BindingField {
                    source: "quo".into(),
                    binding: "quo".into()
                },
                crate::ast::BindingField {
                    source: "remainder".into(),
                    binding: "rem".into()
                },
            ]
        );
    }

    #[test]
    fn rejects_duplicate_destructuring_bindings() {
        let mut parser = Parser::new(tokens_of("{ value = left, value = right } := pair"));
        let error = parser
            .parse_statement()
            .expect_err("duplicate binding must fail");
        assert!(error.message.contains("duplicate binding `value`"));
    }

    #[test]
    fn parses_as_cast_operator() {
        use crate::token::Token;
        let tokens = vec![Token::Ident("x"), Token::As, Token::I32, Token::Eof];
        let mut parser = Parser::new(tokens);
        match parser.parse_expression().unwrap() {
            Expression::Cast { target_ty, expr } => {
                assert!(matches!(target_ty, Type::Primitive(n) if n == "i32"));
                match expr.as_ref() {
                    Expression::Primary(crate::ast::PrimaryExpr::Identifier(n)) => {
                        assert_eq!(n, "x");
                    }
                    other => panic!("expected identifier, got: {other:?}"),
                }
            }
            other => panic!("expected Cast, got: {other:?}"),
        }
    }

    // Parse `source` as a single expression through the real lexer.
    fn expression_of(source: &str) -> Expression {
        let mut parser = Parser::new_with_spans(Lexer::tokenize(source));
        parser
            .parse_expression()
            .unwrap_or_else(|e| panic!("failed to parse `{source}`: {e}"))
    }

    #[test]
    fn bitwise_operators_bind_tighter_than_comparisons() {
        use crate::ast::BinaryOp;
        // `flags & MASK == 0` is `(flags & MASK) == 0`, not C's `flags & (MASK == 0)`.
        let Expression::Binary { op, left, .. } = expression_of("flags & MASK == 0") else {
            panic!("expected a binary expression")
        };
        assert_eq!(op, BinaryOp::Eq);
        assert!(matches!(
            left.as_ref(),
            Expression::Binary {
                op: BinaryOp::BitwiseAnd,
                ..
            }
        ));

        // `|` stays looser than `^`, which stays looser than `&`.
        let Expression::Binary { op, right, .. } = expression_of("a | b ^ c & d") else {
            panic!("expected a binary expression")
        };
        assert_eq!(op, BinaryOp::BitwiseOr);
        let Expression::Binary { op, right, .. } = right.as_ref() else {
            panic!("expected `^` under `|`")
        };
        assert_eq!(*op, BinaryOp::BitwiseXor);
        assert!(matches!(
            right.as_ref(),
            Expression::Binary {
                op: BinaryOp::BitwiseAnd,
                ..
            }
        ));
    }

    #[test]
    fn shifts_bind_tighter_than_bitwise_and_comparisons() {
        use crate::ast::BinaryOp;
        let Expression::Binary { op, left, .. } = expression_of("a < b << 2") else {
            panic!("expected a binary expression")
        };
        assert_eq!(op, BinaryOp::Lt);
        assert!(matches!(
            left.as_ref(),
            Expression::Primary(PrimaryExpr::Identifier(_))
        ));
    }

    #[test]
    fn parses_radix_literals_and_bitwise_not() {
        use crate::ast::UnaryOp;
        assert_eq!(
            expression_of("0b1010_1010"),
            Expression::Primary(PrimaryExpr::Literal(Literal::BinInteger(0xAA)))
        );
        assert_eq!(
            expression_of("0x8000_0000"),
            Expression::Primary(PrimaryExpr::Literal(Literal::HexInteger(0x8000_0000)))
        );
        assert_eq!(
            expression_of("1_000_000"),
            Expression::Primary(PrimaryExpr::Literal(Literal::Integer(1_000_000)))
        );
        let Expression::Unary { op, .. } = expression_of("~mask") else {
            panic!("expected a unary expression")
        };
        assert_eq!(op, UnaryOp::BitNot);
    }

    #[test]
    fn rejects_legacy_import_declaration() {
        use crate::token::Token;
        let tokens = vec![Token::Import, Token::String(r#""core/io""#), Token::Eof];
        let mut parser = Parser::new(tokens);
        let err = parser
            .parse_program()
            .expect_err("legacy import string syntax must be rejected");
        assert!(
            err.to_string().contains("top-level module binding"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_module_import_binding() {
        use crate::token::Token;
        let tokens = vec![
            Token::Ident("math"),
            Token::ColonEqual,
            Token::Import,
            Token::LParen,
            Token::String(r#""./math.hll""#),
            Token::RParen,
            Token::Eof,
        ];
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        assert_eq!(program.declarations.len(), 1);
        match &program.declarations[0].decl {
            DeclNode::ModuleImport { alias, path } => {
                assert_eq!(alias, "math");
                assert_eq!(path, "./math.hll");
            }
            other => panic!("expected ModuleImport, got: {other:?}"),
        }
    }

    #[test]
    fn parses_const_module_import_binding() {
        use crate::token::Token;
        let tokens = vec![
            Token::Const,
            Token::Ident("http"),
            Token::Assign,
            Token::Import,
            Token::LParen,
            Token::String(r#""http""#),
            Token::RParen,
            Token::Eof,
        ];
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        match &program.declarations[0].decl {
            DeclNode::ModuleImport { alias, path } => {
                assert_eq!(alias, "http");
                assert_eq!(path, "http");
            }
            other => panic!("expected ModuleImport, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_import_call_inside_function_body() {
        use crate::token::Token;
        // fn body: `m := import("x")` must be rejected as a body statement.
        let tokens = vec![
            Token::Ident("f"),
            Token::Colon,
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::Ident("m"),
            Token::ColonEqual,
            Token::Import,
            Token::LParen,
            Token::String(r#""x""#),
            Token::RParen,
            Token::RBrace,
            Token::Eof,
        ];
        let mut parser = Parser::new(tokens);
        let err = parser
            .parse_program()
            .expect_err("import(...) in a body must be rejected");
        assert!(
            err.to_string().contains("top-level module binding"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_generic_type_declaration() {
        let tokens = vec![
            Token::Type,
            Token::Ident("Vector"),
            Token::Lt,
            Token::Ident("T"),
            Token::Gt,
            Token::Assign,
            Token::Ident("Vector"),
            Token::Lt,
            Token::Ident("T"),
            Token::Gt,
            Token::Eof,
        ];

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();

        match &program.declarations[0].decl {
            DeclNode::Type {
                name, generics, ty, ..
            } => {
                assert_eq!(name, "Vector");
                assert_eq!(generics, &vec!["T".to_string()]);
                assert!(
                    matches!(ty, Type::Named { name, args } if name == "Vector" && args.len() == 1)
                );
            }
            other => panic!("unexpected declaration: {other:?}"),
        }
    }

    #[test]
    fn parses_slice_and_array_type_suffixes() {
        // `i32[]` is a slice; `i32[3]` is a fixed array.
        let i32_ty = || Type::Named {
            name: "i32".into(),
            args: vec![],
        };

        let mut slice = Parser::new(vec![Token::Ident("i32"), Token::LBracket, Token::RBracket]);
        assert_eq!(slice.parse_type().unwrap(), Type::Slice(Box::new(i32_ty())));

        let mut array = Parser::new(vec![
            Token::Ident("i32"),
            Token::LBracket,
            Token::Integer("3"),
            Token::RBracket,
        ]);
        assert_eq!(
            array.parse_type().unwrap(),
            Type::Array(ArrayLen::Fixed(3), Box::new(i32_ty()))
        );
    }

    #[test]
    fn parses_constant_array_length_as_expression() {
        let mut parser = Parser::new(vec![
            Token::Ident("u8"),
            Token::LBracket,
            Token::Ident("CAP"),
            Token::RBracket,
        ]);
        let Type::Array(ArrayLen::Const(len), _) = parser.parse_type().unwrap() else {
            panic!("expected an unresolved constant length");
        };
        assert_eq!(
            *len,
            Expression::Primary(PrimaryExpr::Identifier("CAP".to_owned()))
        );
    }

    #[test]
    fn parses_function_pointer_type() {
        let mut parser = Parser::new(vec![
            Token::LParen,
            Token::I32,
            Token::Comma,
            Token::I32,
            Token::RParen,
            Token::Minus,
            Token::Gt,
            Token::I32,
            Token::Eof,
        ]);

        assert_eq!(
            parser.parse_type().unwrap(),
            Type::Function {
                params: vec![
                    Type::Primitive("i32".to_owned()),
                    Type::Primitive("i32".to_owned())
                ],
                return_type: Some(Box::new(Type::Primitive("i32".to_owned()))),
            }
        );
    }

    #[test]
    fn parses_dynamic_reference_as_a_wide_value() {
        let mut parser = Parser::new(tokens_of("dyn geometry.Measurable*"));
        assert_eq!(
            parser.parse_type().unwrap(),
            Type::Pointer(Box::new(Type::DynamicReference(
                crate::ast::Path::from_segments(vec![
                    "geometry".to_owned(),
                    "Measurable".to_owned(),
                ]),
            )))
        );

        let mut parser = Parser::new(tokens_of("dyn Measurable"));
        assert_eq!(
            parser.parse_type().unwrap(),
            Type::DynamicReference(crate::ast::Path::single("Measurable"))
        );
    }

    #[test]
    fn parses_associated_items_and_generic_owner_methods() {
        let src = r#"
struct Matrix<T> { cell: T }
impl Matrix<T> {
    const LANES = 4
    type Row = T[4]
    scale: <U>(self: Matrix*, factor: U) -> i32 { return 0 }
}
"#;
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        let DeclNode::Function { name, generics, .. } = &program.declarations[1].decl else {
            panic!("expected desugared method");
        };
        assert_eq!(name, "Matrix_scale");
        // Owner generics precede the member's own generics.
        assert_eq!(generics, &["T", "U"]);
        let DeclNode::AssociatedItems {
            owner,
            generics,
            consts,
            aliases,
        } = &program.declarations[2].decl
        else {
            panic!("expected associated items");
        };
        assert_eq!(owner, "Matrix");
        assert_eq!(generics, &["T"]);
        assert_eq!(consts.len(), 1);
        assert_eq!(consts[0].0, "LANES");
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].0, "Row");
    }

    #[test]
    fn parses_associated_projections_in_type_and_expression_position() {
        let mut parser = Parser::new(tokens_of("Matrix<i32>.Row*"));
        let ty = parser.parse_type().expect("projection type should parse");
        let Type::Pointer(inner) = ty else {
            panic!("expected pointer suffix to apply");
        };
        let Type::Associated { owner, member, .. } = *inner else {
            panic!("expected associated type");
        };
        assert_eq!(owner, "Matrix");
        assert_eq!(member, "Row");

        let mut parser = Parser::new(tokens_of("return Matrix<i32>.LANES"));
        let statement = parser.parse_statement().expect("projection should parse");
        let Statement::Return(Some(Expression::Primary(PrimaryExpr::AssociatedConst {
            owner,
            member,
            ..
        }))) = statement
        else {
            panic!("expected associated constant expression");
        };
        assert_eq!(owner, "Matrix");
        assert_eq!(member, "LANES");
    }

    #[test]
    fn parses_arbitrarily_deep_compile_time_paths() {
        let mut parser = Parser::new(tokens_of("alpha.beta.Container<gamma.delta.Item>"));
        let Type::Named { name, args } = parser.parse_type().expect("qualified type should parse")
        else {
            panic!("expected named type");
        };
        assert_eq!(name.segments, ["alpha", "beta", "Container"]);
        let Type::Named { name: argument, .. } = &args[0] else {
            panic!("expected named generic argument");
        };
        assert_eq!(argument.segments, ["gamma", "delta", "Item"]);
    }

    #[test]
    fn nested_generic_pointer_suffix_applies_to_outer_type() {
        let tokens = Lexer::tokenize("Box<Pair<i32>>*")
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        let mut parser = Parser::new(tokens);
        let ty = parser
            .parse_type()
            .expect("nested generic type should parse");
        let Type::Pointer(inner) = ty else {
            panic!("pointer suffix must apply to Box");
        };
        assert!(matches!(
            *inner,
            Type::Named { ref name, ref args }
                if name == "Box"
                    && matches!(&args[..], [Type::Named { name, args }] if name == "Pair" && args.len() == 1)
        ));
    }

    fn tokens_of(src: &str) -> Vec<Token<'_>> {
        Lexer::tokenize(src)
            .into_iter()
            .map(|(token, _)| token)
            .collect()
    }

    #[test]
    fn parses_enum_with_unit_and_payload_variants() {
        let src = "enum Shape {\nCircle(f64)\nRect(f64, f64)\nEmpty\n}";
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        match &program.declarations[0].decl {
            DeclNode::Enum {
                name,
                generics,
                variants,
                ..
            } => {
                assert_eq!(name, "Shape");
                assert!(generics.is_empty());
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].name, "Circle");
                assert_eq!(variants[0].payload.len(), 1);
                assert_eq!(variants[1].payload.len(), 2);
                assert!(variants[2].payload.is_empty());
            }
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn parses_generic_enum() {
        let src = "enum Option<T> {\nSome(T)\nNone\n}";
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        match &program.declarations[0].decl {
            DeclNode::Enum {
                name,
                generics,
                variants,
                ..
            } => {
                assert_eq!(name, "Option");
                assert_eq!(generics, &vec!["T".to_string()]);
                assert_eq!(variants.len(), 2);
            }
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn parses_bounds_on_all_generic_declaration_kinds() {
        let src = r#"
struct Pair<T: First + Second, U> { first: T, second: U }
enum Maybe<T: First> { Value(T), Missing }
type Callback<T: Second> = (T*) -> i32
"#;
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        let expected = vec![("T".to_owned(), Path::single("First"))];
        match &program.declarations[0].decl {
            DeclNode::Struct {
                generics, bounds, ..
            } => {
                assert_eq!(generics, &["T", "U"]);
                assert_eq!(
                    bounds,
                    &[
                        ("T".to_owned(), Path::single("First")),
                        ("T".to_owned(), Path::single("Second"))
                    ]
                );
            }
            other => panic!("expected struct, got {other:?}"),
        }
        match &program.declarations[1].decl {
            DeclNode::Enum { bounds, .. } => assert_eq!(bounds, &expected),
            other => panic!("expected enum, got {other:?}"),
        }
        match &program.declarations[2].decl {
            DeclNode::Type { bounds, .. } => {
                assert_eq!(bounds, &[("T".to_owned(), Path::single("Second"))]);
            }
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn rejects_duplicate_generic_parameters_and_bounds() {
        for (source, expected) in [
            (
                "struct Pair<T, T> { value: T }",
                "duplicate generic parameter `T`",
            ),
            (
                "enum Maybe<T: First + First> { Missing }",
                "duplicate bound `T: First`",
            ),
        ] {
            let mut parser = Parser::new(tokens_of(source));
            let error = parser
                .parse_program()
                .expect_err("declaration must be rejected");
            assert!(
                error.to_string().contains(expected),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn parses_qualified_interface_bounds() {
        let mut parser = Parser::new(tokens_of(
            "struct Holder<T: geometry.Measurable> { value: T }",
        ));
        let program = parser.parse_program().unwrap();
        let DeclNode::Struct { bounds, .. } = &program.declarations[0].decl else {
            panic!("expected struct");
        };
        assert_eq!(
            bounds,
            &[(
                "T".to_owned(),
                Path::from_segments(vec!["geometry".to_owned(), "Measurable".to_owned()])
            )]
        );
    }

    #[test]
    fn parses_match_with_variant_and_wildcard_patterns() {
        let src = "x = match s {\nCircle(r) -> {\nreturn r\n}\n_ -> {\nreturn 0\n}\n}";
        let mut parser = Parser::new(tokens_of(src));
        let Statement::Expression(Expression::Assignment { rvalue, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected assignment");
        };
        let Expression::Match { scrutinee, arms } = *rvalue else {
            panic!("expected match expression");
        };
        assert!(matches!(
            *scrutinee,
            Expression::Primary(PrimaryExpr::Identifier(ref n)) if n == "s"
        ));
        assert_eq!(arms.len(), 2);
        match &arms[0].pattern {
            Pattern::Variant {
                variant, bindings, ..
            } => {
                assert_eq!(variant, "Circle");
                assert_eq!(bindings, &vec!["r".to_string()]);
            }
            other => panic!("expected variant pattern, got {other:?}"),
        }
        assert!(matches!(arms[1].pattern, Pattern::Wildcard));
    }

    #[test]
    fn parses_struct_literal_rest_marker() {
        let mut parser = Parser::new(tokens_of("p = { .x = 1, .. }"));
        let Statement::Expression(Expression::Assignment { rvalue, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected assignment");
        };
        let Expression::Primary(PrimaryExpr::StructLiteral { fields, rest }) = *rvalue else {
            panic!("expected struct literal");
        };
        assert_eq!(fields.len(), 1);
        assert!(rest);
    }

    #[test]
    fn rejects_rest_marker_before_a_field() {
        let mut parser = Parser::new(tokens_of("p = { .., .x = 1 }"));
        let error = parser
            .parse_statement()
            .expect_err("`..` must come last in a struct literal");
        assert!(
            error.to_string().contains("must be the last entry"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parses_qualified_enum_variant_construction() {
        let src = "enum Shape {\nCircle(f64)\nEmpty\n}\nmain: () -> i32 {\ns := Shape.Circle(2.0)\ne := Shape.Empty\nreturn 0\n}";
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected function");
        };
        let Statement::InferredVariableDecl { init, .. } = &body.statements[0] else {
            panic!("expected inferred binding");
        };
        let Expression::Primary(PrimaryExpr::FunctionCall { name, .. }) = init else {
            panic!("expected call, got {init:?}");
        };
        assert_eq!(name, "Shape.Circle");
        let Statement::InferredVariableDecl { init, .. } = &body.statements[1] else {
            panic!("expected inferred binding");
        };
        assert!(matches!(
            init,
            Expression::Primary(PrimaryExpr::Identifier(name)) if name == "Shape.Empty"
        ));
    }

    #[test]
    fn parses_qualified_enum_variant_pattern() {
        let src = "enum Shape {\nCircle(f64)\nEmpty\n}\nmain: () -> i32 {\nmatch s {\nShape.Circle(r) -> {\nreturn 1\n}\nShape.Empty -> {\nreturn 0\n}\n}\nreturn 0\n}";
        let mut parser = Parser::new(tokens_of(src));
        let program = parser.parse_program().unwrap();
        let DeclNode::Function {
            body: Some(body), ..
        } = &program.declarations[1].decl
        else {
            panic!("expected function");
        };
        let Statement::Expression(Expression::Match { arms, .. }) = &body.statements[0] else {
            panic!("expected match statement");
        };
        let Pattern::Variant {
            enum_name,
            variant,
            bindings,
        } = &arms[0].pattern
        else {
            panic!("expected variant pattern");
        };
        assert_eq!(enum_name.as_deref(), Some("Shape"));
        assert_eq!(variant, "Circle");
        assert_eq!(bindings, &vec!["r".to_owned()]);
        let Pattern::Variant {
            enum_name, variant, ..
        } = &arms[1].pattern
        else {
            panic!("expected variant pattern");
        };
        assert_eq!(enum_name.as_deref(), Some("Shape"));
        assert_eq!(variant, "Empty");
    }

    #[test]
    fn parses_value_match_arms() {
        let src = "n := match s {\nCircle(r) -> r * r\n_ -> 0\n}";
        let mut parser = Parser::new(tokens_of(src));
        let Statement::InferredVariableDecl { init, .. } = parser.parse_statement().unwrap() else {
            panic!("expected inferred binding");
        };
        let Expression::Match { arms, .. } = init else {
            panic!("expected match expression");
        };
        assert_eq!(arms.len(), 2);
        // Value arms carry an expression and an empty statement body.
        assert!(arms[0].value.is_some());
        assert!(arms[0].body.statements.is_empty());
        assert!(arms[1].value.is_some());
    }

    #[test]
    fn parses_literal_match_patterns() {
        // Block-body arms avoid the newline-insensitive value-arm ambiguity with a
        // following `-N` pattern (a bare value arm would read `2` then `- 3`).
        let src = "match c {\n10 -> {\nx = 1\n}\n'A' -> {\nx = 2\n}\n-3 -> {\nx = 4\n}\n_ -> {\nx = 0\n}\n}";
        let mut parser = Parser::new(tokens_of(src));
        let Statement::Expression(Expression::Match { arms, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected match expression");
        };
        assert_eq!(arms.len(), 4);
        assert!(matches!(
            arms[0].pattern,
            Pattern::Literal(Literal::Integer(10))
        ));
        // A char literal lowers to its ascii byte (`'A'` == 65).
        assert!(matches!(
            arms[1].pattern,
            Pattern::Literal(Literal::Integer(65))
        ));
        assert!(matches!(
            arms[2].pattern,
            Pattern::Literal(Literal::Integer(-3))
        ));
        assert!(matches!(arms[3].pattern, Pattern::Wildcard));
    }

    #[test]
    fn parses_postfix_question_as_try() {
        let src = "n = parse(text)?";
        let mut parser = Parser::new(tokens_of(src));
        let Statement::Expression(Expression::Assignment { rvalue, .. }) =
            parser.parse_statement().unwrap()
        else {
            panic!("expected assignment");
        };
        assert!(matches!(*rvalue, Expression::Try(_)));
    }
}
