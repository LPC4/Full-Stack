use super::ast::*;
use super::token::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserError {
    pub message: String,
    pub pos: usize,
}

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    pub tokens: Vec<Token<'a>>,
    pub pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParserError> {
        let mut declarations = Vec::new();
        let mut statements = Vec::new();

        self.consume_terminators();
        while !self.is_eof() {
            if self.is_declaration_start() {
                declarations.push(self.parse_declaration()?);
            } else {
                statements.push(self.parse_statement()?);
            }
            self.consume_terminators();
        }

        Ok(Program {
            declarations,
            statements,
        })
    }

    pub fn parse_declaration(&mut self) -> Result<Declaration, ParserError> {
        self.consume_terminators();

        let decl = match self.peek() {
            Some(Token::Const | Token::ConstKeyword) => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect_assign()?;
                let init = self.parse_expression()?;
                DeclNode::Const { name, init }
            }
            Some(Token::Type | Token::TypeKeyword) => {
                self.advance();
                let name = self.expect_ident()?;
                let generics = self.parse_generic_params()?;
                let ty = if self.match_assign() {
                    self.parse_type()?
                } else if self.check_lbrace() {
                    self.parse_struct_type()?
                } else {
                    return Err(self.error("expected `=` or `{` after type declaration name"));
                };
                DeclNode::Type { name, generics, ty }
            }
            Some(Token::External) => {
                self.advance();
                self.parse_function_decl(true)?
            }
            Some(Token::Ident(_)) => {
                // Look ahead to determine if this is a function or variable declaration
                // Function=    identifier : ( params ) -> return_type { }
                // Variable=    identifier : type [= expr]
                // After "identifier:", if next is LParen, it's a function
                if self.peek_n(1) == Some(&Token::Colon) && self.peek_n(2) == Some(&Token::LParen) {
                    self.parse_function_decl(false)?
                } else {
                    self.parse_variable_decl()?
                }
            }
            Some(tok) => {
                return Err(self.error_with_token("unexpected token at declaration start", tok));
            }
            None => return Err(self.error("unexpected end of input")),
        };

        Ok(Declaration { decl })
    }

    pub fn parse_block(&mut self) -> Result<Block, ParserError> {
        self.expect_lbrace()?;
        let mut statements = Vec::new();

        self.consume_terminators();
        while !self.check_rbrace() {
            statements.push(self.parse_statement()?);
            self.consume_terminators();
        }

        self.expect_rbrace()?;
        Ok(Block { statements })
    }

    pub fn parse_statement(&mut self) -> Result<Statement, ParserError> {
        self.consume_terminators();

        match self.peek() {
            Some(Token::If) => self.parse_if_statement(),
            Some(Token::While) => self.parse_while_statement(),
            Some(Token::Return) => self.parse_return_statement(),
            Some(Token::Defer) => self.parse_defer_statement(),
            Some(Token::Break) => {
                self.advance();
                Ok(Statement::Break)
            }
            Some(Token::Continue) => {
                self.advance();
                Ok(Statement::Continue)
            }
            Some(Token::LBrace) => {
                // Need to distinguish between block and tuple destructuring assignment
                // Look ahead to see if this is {ident, ident, ...} = expr
                let saved_pos = self.pos;
                self.advance(); // consume LBrace

                let is_tuple_destructure = if matches!(self.peek(), Some(Token::Ident(_))) {
                    // Check if next is comma (tuple) or colon (block with var decl) or other
                    matches!(self.peek_n(1), Some(Token::Comma))
                } else {
                    false
                };

                self.pos = saved_pos;

                if is_tuple_destructure {
                    // Parse as expression (which will handle tuple destructuring assignment)
                    Ok(Statement::Expression(self.parse_expression()?))
                } else {
                    // Parse as block
                    Ok(Statement::Block(self.parse_block()?))
                }
            }
            Some(Token::Ident(_)) if self.peek_n(1) == Some(&Token::Colon) => {
                // Could be variable declaration or function call
                // Check if it's a function by looking further ahead
                if self.peek_n(2) == Some(&Token::LParen) {
                    // This is actually a function declaration, shouldn't happen in statement context
                    // Fall through to expression parsing
                    Ok(Statement::Expression(self.parse_expression()?))
                } else {
                    let name = self.expect_ident()?;
                    self.expect_colon()?;
                    let ty = self.parse_type()?;
                    let init = if self.match_assign() {
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };

                    Ok(Statement::VariableDecl { name, ty, init })
                }
            }
            Some(_) => Ok(Statement::Expression(self.parse_expression()?)),
            None => Err(self.error("unexpected end of input while parsing statement")),
        }
    }

    pub fn parse_expression(&mut self) -> Result<Expression, ParserError> {
        self.parse_assignment()
    }

    pub fn parse_type(&mut self) -> Result<Type, ParserError> {
        if self.match_lbracket() {
            let size = self.parse_usize_literal()?;
            self.expect_rbracket()?;
            let ty = self.parse_type()?;
            return Ok(Type::Array(size, Box::new(ty)));
        }

        let mut ty = self.parse_type_atom()?;

        loop {
            if self.match_star() {
                ty = Type::Pointer(Box::new(ty));
                continue;
            }

            if self.match_lbracket() {
                let size = self.parse_usize_literal()?;
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
            None
        };

        Ok(DeclNode::Variable { name, ty, init })
    }

    fn parse_function_decl(&mut self, is_extern: bool) -> Result<DeclNode, ParserError> {
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;

        // Expect colon before parameters
        self.expect_colon()?;

        let params = self.parse_param_list()?;

        // Expect arrow for return type
        let return_type = if self.match_arrow() {
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
            params,
            return_type,
            body,
            is_extern,
        })
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
        if self.match_lparen() {
            let mut fields = Vec::new();

            if self.match_rparen() {
                return Ok(ReturnType::Tuple(fields));
            }

            loop {
                let (name, ty) = if matches!(self.peek(), Some(Token::Ident(_)))
                    && self.peek_n(1) == Some(&Token::Colon)
                {
                    let name = self.expect_ident()?;
                    self.expect_colon()?;
                    (Some(name), self.parse_type()?)
                } else {
                    (None, self.parse_type()?)
                };
                fields.push(ReturnField { name, ty });

                if self.match_comma() {
                    if self.check_rparen() {
                        break;
                    }
                    continue;
                }

                break;
            }

            self.expect_rparen()?;
            Ok(ReturnType::Tuple(fields))
        } else {
            Ok(ReturnType::Single(self.parse_type()?))
        }
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

    fn parse_return_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_return()?;
        if self.is_expression_terminator() || self.check_rbrace() || self.is_eof() {
            Ok(Statement::Return(None))
        } else {
            // Parse a single expression (which can be a tuple literal with braces)
            let expr = self.parse_expression()?;
            Ok(Statement::Return(Some(expr)))
        }
    }

    fn parse_defer_statement(&mut self) -> Result<Statement, ParserError> {
        self.expect_defer()?;
        Ok(Statement::Defer(self.parse_expression()?))
    }

    fn parse_assignment(&mut self) -> Result<Expression, ParserError> {
        // Only parenthesis-delimited tuple assignments are supported
        if matches!(self.peek(), Some(Token::LParen)) {
            let saved_pos = self.pos;
            let mut trial = self.clone();
            
            // Try to parse as tuple destructuring
            if let Ok(target) = trial.parse_tuple_assign_target() {
                // Check if followed by assignment operator
                if trial.match_assign() {
                    // Commit to this parse
                    *self = trial;
                    let rvalue = self.parse_assignment()?;
                    return Ok(Expression::Assignment {
                        target: Box::new(target),
                        rvalue: Box::new(rvalue),
                    });
                }
            }
            
            // Not a tuple destructuring, restore position and continue with normal parsing
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
        let mut expr = self.parse_additive()?;
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
        for op in ops.into_iter().rev() {
            expr = Expression::Unary {
                op,
                expr: Box::new(expr),
            };
        }

        self.parse_postfix(expr)
    }

    fn parse_postfix(&mut self, mut expr: Expression) -> Result<Expression, ParserError> {
        loop {
            if self.match_dot() {
                let field = self.expect_ident()?;
                expr = Expression::Primary(PrimaryExpr::FieldAccess {
                    expr: Box::new(expr),
                    field,
                });
                continue;
            }

            if self.match_lbracket() {
                let index = self.parse_expression()?;
                self.expect_rbracket()?;
                expr = Expression::Primary(PrimaryExpr::ArrayIndex {
                    expr: Box::new(expr),
                    index: Box::new(index),
                });
                continue;
            }

            if self.match_lparen() {
                let name = match expr {
                    Expression::Primary(PrimaryExpr::Identifier(name)) => name,
                    _ => return Err(self.error("function calls must target an identifier")),
                };

                let arguments = self.parse_argument_list_after_open_paren()?;
                expr = Expression::Primary(PrimaryExpr::FunctionCall { name, arguments });
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParserError> {
        let expr = match self.peek() {
            Some(Token::Integer(_)) => Expression::Primary(PrimaryExpr::Literal(Literal::Integer(
                self.expect_integer()?,
            ))),
            Some(Token::HexInteger(_)) => Expression::Primary(PrimaryExpr::Literal(
                Literal::HexInteger(self.expect_hex_integer()?),
            )),
            Some(Token::Float(_)) => {
                Expression::Primary(PrimaryExpr::Literal(Literal::Float(self.expect_float()?)))
            }
            Some(Token::StringLit(_)) => Expression::Primary(PrimaryExpr::Literal(
                Literal::StringLit(self.expect_string()?),
            )),
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
            Some(Token::Ident(name)) => {
                let id = name.to_string();
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier(id))
            }
            Some(Token::Free) => {
                self.advance();
                Expression::Primary(PrimaryExpr::Identifier("free".to_string()))
            }
            Some(Token::LParen) => {
                self.advance();
                let expr = self.parse_expression()?;
                if self.match_comma() {
                    let mut elements = vec![expr];
                    while !self.check_rparen() {
                        elements.push(self.parse_expression()?);
                        if !self.match_comma() {
                            break;
                        }
                    }
                    self.expect_rparen()?;
                    Expression::Primary(PrimaryExpr::TupleLiteral(elements))
                } else {
                    self.expect_rparen()?;
                    expr
                }
            }
            Some(Token::LBracket) => {
                self.advance();
                let mut elements = Vec::new();
                if !self.check_rbracket() {
                    loop {
                        elements.push(self.parse_expression()?);
                        if self.match_comma() {
                            if self.check_rbracket() {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.expect_rbracket()?;
                Expression::Primary(PrimaryExpr::ArrayLiteral(elements))
            }
            Some(Token::LBrace) => {
                // Save position for potential backtracking
                let saved_parser = self.clone();

                // Try to parse as tuple literal first (no field names)
                self.advance();
                let mut elements = Vec::new();
                let mut parse_failed = false;

                if !self.check_rbrace() {
                    loop {
                        match self.parse_expression() {
                            Ok(expr) => {
                                elements.push(expr);
                                if self.match_comma() {
                                    if self.check_rbrace() {
                                        break;
                                    }
                                    continue;
                                }
                                break;
                            }
                            Err(_) => {
                                parse_failed = true;
                                break;
                            }
                        }
                    }
                }

                // If we successfully parsed at least one element and found closing brace, it's a tuple
                if !parse_failed && !elements.is_empty() && self.check_rbrace() {
                    self.expect_rbrace()?;
                    return Ok(Expression::Primary(PrimaryExpr::TupleLiteral(elements)));
                }

                // Otherwise, backtrack and try as struct literal
                *self = saved_parser;
                self.advance(); // consume LBrace again
                let mut fields = Vec::new();

                if !self.check_rbrace() {
                    loop {
                        // A field init can be `name: expr` or just `expr`
                        let has_name = matches!(self.peek(), Some(Token::Ident(_)))
                            && self.peek_n(1) == Some(&Token::Colon);

                        let name = if has_name {
                            let n = self.expect_ident()?;
                            self.expect_colon()?;
                            Some(n)
                        } else {
                            None
                        };

                        let expr = self.parse_expression()?;
                        fields.push(FieldInit { name, expr });

                        if self.match_comma() {
                            if self.check_rbrace() {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }

                self.expect_rbrace()?;
                Expression::Primary(PrimaryExpr::StructLiteral(fields))
            }
            Some(Token::New) => {
                self.advance();
                self.expect_lparen()?;
                let ty = self.parse_type()?;
                let mut args = Vec::new();
                if self.match_comma() {
                    if !self.check_rparen() {
                        loop {
                            args.push(self.parse_expression()?);
                            if self.match_comma() {
                                if self.check_rparen() {
                                    break;
                                }
                                continue;
                            }
                            break;
                        }
                    }
                }
                self.expect_rparen()?;
                Expression::Primary(PrimaryExpr::New { ty, args })
            }
            Some(tok) => {
                return Err(self.error_with_token("unexpected token in primary expression", tok));
            }
            None => return Err(self.error("unexpected end of input")),
        };
        Ok(expr)
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
            Some(Token::Str) => {
                self.advance();
                Ok(Type::Primitive("Str".into()))
            }
            Some(Token::Ident(_)) => {
                let name = self.expect_ident()?;
                let args = if self.match_lt() {
                    let mut args = Vec::new();
                    if !self.check_gt() {
                        loop {
                            args.push(self.parse_type()?);
                            if self.match_comma() {
                                if self.check_gt() {
                                    break;
                                }
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect_gt()?;
                    args
                } else {
                    Vec::new()
                };

                Ok(Type::Named { name, args })
            }
            Some(Token::LBrace) => self.parse_struct_type(),
            Some(tok) => Err(self.error_with_token("expected type", tok)),
            None => Err(self.error("unexpected end of input while parsing type")),
        }
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
                continue;
            }
        }

        self.expect_rbrace()?;
        Ok(Type::Struct(fields))
    }

    fn parse_generic_params(&mut self) -> Result<Vec<String>, ParserError> {
        if !self.match_lt() {
            return Ok(Vec::new());
        }

        let mut params = Vec::new();
        if !self.check_gt() {
            loop {
                params.push(self.expect_ident()?);
                if self.match_comma() {
                    if self.check_gt() {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        self.expect_gt()?;
        Ok(params)
    }

    fn parse_argument_list_after_open_paren(&mut self) -> Result<Vec<Expression>, ParserError> {
        let mut args = Vec::new();
        if self.match_rparen() {
            return Ok(args);
        }

        loop {
            args.push(self.parse_expression()?);
            if self.match_comma() {
                if self.check_rparen() {
                    break;
                }
                continue;
            }
            break;
        }

        self.expect_rparen()?;
        Ok(args)
    }

    fn parse_tuple_assign_target(&mut self) -> Result<AssignTarget, ParserError> {
        self.expect_lparen()?;
        let mut fields = Vec::new();

        if self.check_rparen() {
            return Err(self.error("empty tuple destructuring is not allowed"));
        }

        loop {
            fields.push(self.parse_tuple_destructure_field()?);
            if self.match_comma() {
                if self.check_rparen() {
                    break;
                }
                continue;
            }
            break;
        }

        self.expect_rparen()?;
        Ok(AssignTarget::Tuple(fields))
    }

    fn parse_tuple_destructure_field(&mut self) -> Result<TupleDestructureField, ParserError> {
        let name = self.expect_ident()?;
        
        // Check for optional type annotation
        let ty = if self.match_colon() {
            Some(self.parse_type()?)
        } else {
            None
        };

        Ok(TupleDestructureField { name, ty })
    }

    fn expression_to_target(&self, expr: Expression) -> Result<AssignTarget, ParserError> {
        match expr {
            Expression::Primary(PrimaryExpr::Identifier(name)) => {
                Ok(AssignTarget::Identifier(name))
            }
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

    fn parse_usize_literal(&mut self) -> Result<usize, ParserError> {
        match self.peek() {
            Some(Token::Integer(text)) => {
                let value = text
                    .parse::<usize>()
                    .map_err(|_| self.error("invalid array size"))?;
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected array size", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParserError> {
        match self.peek() {
            Some(Token::Ident(name)) => {
                let out = (*name).to_string();
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
                let value = text
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
                let trimmed = text.trim_start_matches("0x").trim_start_matches("0X");
                let value = i64::from_str_radix(trimmed, 16)
                    .map_err(|_| self.error("invalid hex literal"))?;
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected hex integer literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_float(&mut self) -> Result<f64, ParserError> {
        match self.peek() {
            Some(Token::Float(text)) => {
                let value = text
                    .parse::<f64>()
                    .map_err(|_| self.error("invalid float literal"))?;
                self.advance();
                Ok(value)
            }
            Some(tok) => Err(self.error_with_token("expected float literal", tok)),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParserError> {
        match self.peek() {
            Some(Token::StringLit(text)) => {
                let out = (*text).to_string();
                self.advance();
                Ok(out)
            }
            Some(tok) => Err(self.error_with_token("expected string literal", tok)),
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

    fn expect_defer(&mut self) -> Result<(), ParserError> {
        if self.match_defer() {
            Ok(())
        } else {
            Err(self.error("expected `defer`"))
        }
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

    fn check_gt(&self) -> bool {
        matches!(self.peek(), Some(Token::Gt))
    }

    fn check_if(&self) -> bool {
        matches!(self.peek(), Some(Token::If))
    }

    fn is_declaration_start(&self) -> bool {
        match self.peek() {
            Some(Token::Const | Token::ConstKeyword)
            | Some(Token::Type | Token::TypeKeyword)
            | Some(Token::External) => true,
            Some(Token::Ident(_)) => {
                // Check for function: identifier ":" "(" or variable: identifier ":" type
                matches!(self.peek_n(1), Some(Token::Colon))
            }
            _ => false,
        }
    }

    fn is_expression_terminator(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::StatementTerminator | Token::RBrace | Token::Eof)
        )
    }

    fn match_assign(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Assign))
    }

    fn match_colon(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Colon))
    }

    fn match_lparen(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::LParen))
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
        if self.match_minus() && self.match_gt() {
            true
        } else {
            false
        }
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

    fn match_gt(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Gt))
    }

    fn match_gte(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Gte))
    }

    fn expect_gt(&mut self) -> Result<(), ParserError> {
        if self.match_gt() {
            Ok(())
        } else {
            Err(self.error("expected `>`"))
        }
    }

    fn match_not(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Not))
    }

    fn match_ampersand(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Ampersand))
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

    fn match_return(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Return))
    }

    fn match_defer(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::Defer))
    }

    fn match_statement_terminator(&mut self) -> bool {
        self.match_variant(|t| matches!(t, Token::StatementTerminator))
    }

    fn match_variant<F>(&mut self, predicate: F) -> bool
    where
        F: FnOnce(&Token<'a>) -> bool,
    {
        if let Some(tok) = self.peek() {
            if predicate(tok) {
                self.advance();
                return true;
            }
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

    fn error(&self, message: &str) -> ParserError {
        ParserError {
            message: message.to_string(),
            pos: self.pos,
        }
    }

    fn error_with_token(&self, message: &str, token: &Token<'a>) -> ParserError {
        ParserError {
            message: format!("{message}: {token:?}"),
            pos: self.pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_tuple_return_function_signature() {
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
            Token::LParen,
            Token::Ident("quotient"),
            Token::Colon,
            Token::I32,
            Token::Comma,
            Token::Ident("remainder"),
            Token::Colon,
            Token::I32,
            Token::RParen,
            Token::LBrace,
            Token::Return,
            Token::LBrace,
            Token::Ident("a"),
            Token::Slash,
            Token::Ident("b"),
            Token::Comma,
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
                Some(ReturnType::Tuple(fields)) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name.as_deref(), Some("quotient"));
                    assert_eq!(fields[1].name.as_deref(), Some("remainder"));
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
    fn parses_tuple_destructuring_assignment() {
        let tokens = vec![
            Token::LParen,
            Token::Ident("q"),
            Token::Comma,
            Token::Ident("r"),
            Token::RParen,
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
                AssignTarget::Tuple(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "q");
                    assert_eq!(fields[1].name, "r");
                    assert!(fields[0].ty.is_none());
                    assert!(fields[1].ty.is_none());
                }
                other => panic!("unexpected assignment target: {other:?}"),
            },
            other => panic!("unexpected expression: {other:?}"),
        }
    }

    #[test]
    fn parses_tuple_destructuring_with_types() {
        let tokens = vec![
            Token::LParen,
            Token::Ident("q"),
            Token::Colon,
            Token::I32,
            Token::Comma,
            Token::Ident("r"),
            Token::Colon,
            Token::I32,
            Token::RParen,
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
                AssignTarget::Tuple(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "q");
                    assert!(matches!(&fields[0].ty, Some(Type::Primitive(name)) if name == "i32"));
                    assert_eq!(fields[1].name, "r");
                    assert!(matches!(&fields[1].ty, Some(Type::Primitive(name)) if name == "i32"));
                }
                other => panic!("unexpected assignment target: {other:?}"),
            },
            other => panic!("unexpected expression: {other:?}"),
        }
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
            DeclNode::Type { name, generics, ty } => {
                assert_eq!(name, "Vector");
                assert_eq!(generics, &vec!["T".to_string()]);
                assert!(
                    matches!(ty, Type::Named { name, args } if name == "Vector" && args.len() == 1)
                );
            }
            other => panic!("unexpected declaration: {other:?}"),
        }
    }
}
