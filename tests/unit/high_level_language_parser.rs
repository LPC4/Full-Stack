use full_stack::high_level_language::ast::{
    AssignTarget, DeclNode, Expression, ReturnType, Statement, Type,
};
use full_stack::high_level_language::parser::Parser;
use full_stack::high_level_language::token::Token;

#[test]
fn parses_pointer_cast_syntax() {
    // Test parsing of i8*(ptr) as a pointer-type cast, not multiplication
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
            // Verify it's a pointer type i8*
            assert!(matches!(&target_ty, Type::Pointer(inner) if matches!(inner.as_ref(), Type::Primitive(name) if name == "i8")));
            
            // Verify the expression being cast is an identifier
            match expr.as_ref() {
                Expression::Primary(full_stack::high_level_language::ast::PrimaryExpr::Identifier(name)) => {
                    assert_eq!(name, "int_ptr");
                }
                other => panic!("expected identifier in cast, got: {other:?}"),
            }
        }
        other => panic!("expected Cast expression, got: {other:?}"),
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
        Token::Ident("quotient"),
        Token::Colon,
        Token::I32,
        Token::Assign,
        Token::Ident("a"),
        Token::Slash,
        Token::Ident("b"),
        Token::Comma,
        Token::Ident("remainder"),
        Token::Colon,
        Token::I32,
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
                assert!(matches!(&fields[0].ty, Some(Type::Primitive(name)) if name == "i32"));
                assert!(matches!(&fields[1].ty, Some(Type::Primitive(name)) if name == "i32"));
            }
            other => panic!("unexpected assignment target: {other:?}"),
        },
        other => panic!("unexpected expression: {other:?}"),
    }
}

#[test]
fn parses_struct_destructuring_with_types() {
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
                assert!(matches!(&fields[0].ty, Some(Type::Primitive(name)) if name == "i32"));
                assert_eq!(fields[1].name, Some("r".to_string()));
                assert!(matches!(&fields[1].ty, Some(Type::Primitive(name)) if name == "i32"));
            }
            other => panic!("unexpected assignment target: {other:?}"),
        },
        other => panic!("unexpected expression: {other:?}"),
    }
}

#[test]
fn parses_struct_destructuring_with_named_types() {
    let tokens = vec![
        Token::LBrace,
        Token::Ident("file"),
        Token::Colon,
        Token::Ident("FileHandle"),
        Token::Comma,
        Token::Ident("error"),
        Token::Colon,
        Token::Ident("i32"),
        Token::RBrace,
        Token::Assign,
        Token::Ident("open_file"),
        Token::LParen,
        Token::Ident("path"),
        Token::RParen,
        Token::Eof,
    ];

    let mut parser = Parser::new(tokens);
    match parser.parse_expression().unwrap() {
        Expression::Assignment { target, .. } => match *target {
            AssignTarget::StructDestructure(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, Some("file".to_string()));
                assert!(fields[0].ty.is_some());
                assert_eq!(fields[1].name, Some("error".to_string()));
                assert!(fields[1].ty.is_some());
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
