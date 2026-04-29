use std::fs;
use std::path::{Path, PathBuf};

use full_stack::high_level_language::compiler::HighLevelCompiler;
use full_stack::high_level_language::{lexer::Lexer, parser::Parser, token::Token};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/fixtures")
}

fn collect_hll_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut fixtures = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("failed to read fixture directory {dir:?}: {err}"));

        for entry in entries {
            let entry = entry
                .unwrap_or_else(|err| panic!("failed to read directory entry in {dir:?}: {err}"));
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("hll") {
                fixtures.push(path);
            }
        }
    }

    fixtures.sort();
    fixtures
}

fn lex_source(source: &str) -> Vec<Token<'_>> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = matches!(token, Token::Eof);
        tokens.push(token);
        if is_eof {
            break;
        }
    }

    tokens
}

fn lex_fixture(file_name: &str) -> Vec<Token<'static>> {
    let path = fixture_root().join(file_name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());
    lex_source(source)
}

fn parse_fixture(file_name: &str) -> Result<full_stack::high_level_language::ast::Program, String> {
    let path = fixture_root().join(file_name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());

    let tokens = lex_source(source);
    let mut parser = Parser::new(tokens);
    parser
        .parse_program()
        .map_err(|err| format!("{} @{}", err.message, err.pos))
}

fn contains_token<F>(tokens: &[Token<'_>], predicate: F) -> bool
where
    F: Fn(&Token<'_>) -> bool,
{
    tokens.iter().any(predicate)
}

#[test]
fn all_high_level_language_fixtures_lex_to_eof() {
    let fixtures = collect_hll_fixtures(&fixture_root());
    assert!(!fixtures.is_empty(), "expected at least one .hll fixture");

    for path in fixtures {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let tokens = lex_source(&source);

        assert!(
            matches!(tokens.last(), Some(Token::Eof)),
            "{path:?} did not end with EOF"
        );
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token, Token::StatementTerminator)),
            "{path:?} did not contain any statement terminators"
        );
    }
}

#[test]
fn test1_hll_lexes_comments_newlines_and_return() {
    let tokens = lex_fixture("lexer/01_comments_and_newlines.hll");

    assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("x"))));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("y"))));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("z"))));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
    assert!(
        tokens
            .iter()
            .filter(|t| matches!(t, Token::StatementTerminator))
            .count()
            >= 4
    );
    assert!(matches!(tokens.last(), Some(Token::Eof)));
}

#[test]
fn test2_hll_lexes_struct_and_pointer_syntax() {
    let tokens = lex_fixture("parser/02_structs_and_pointers.hll");

    assert!(contains_token(&tokens, |t| matches!(t, Token::Type)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::LBrace)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Ampersand)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Defer)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::If)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
    assert!(matches!(tokens.last(), Some(Token::Eof)));
}

#[test]
fn test3_hll_lexes_nested_access_and_control_flow() {
    let tokens = lex_fixture("parser/03_nested_access_and_control_flow.hll");

    assert!(contains_token(&tokens, |t| matches!(t, Token::Type)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::While)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Break)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Or)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::And)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Not)));
    assert!(matches!(tokens.last(), Some(Token::Eof)));
}

#[test]
fn test4_hll_lexes_multi_return_and_destructuring() {
    let tokens = lex_fixture("parser/04_tuple_returns.hll");

    assert!(contains_token(&tokens, |t| matches!(
        t,
        Token::Ident("divide")
    )));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Comma)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::LBrace)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::RBrace)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Colon)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::If)));
    assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
    assert!(matches!(tokens.last(), Some(Token::Eof)));
}

#[test]
fn test1_hll_parser_success_and_ast_validation() {
    let program =
        parse_fixture("lexer/01_comments_and_newlines.hll").expect("failed to parse test1.hll");

    // x, y, z are declarations, return is a statement
    assert_eq!(
        program.declarations.len(),
        3,
        "expected 3 declarations in test1.hll"
    );
    assert_eq!(
        program.statements.len(),
        1,
        "expected 1 statement in test1.hll"
    );

    use full_stack::high_level_language::ast::*;

    match &program.declarations[0].decl {
        DeclNode::Variable { name, .. } => assert_eq!(name, "x"),
        _ => panic!("Expected VariableDecl for x"),
    }

    match &program.declarations[1].decl {
        DeclNode::Variable { name, .. } => assert_eq!(name, "y"),
        _ => panic!("Expected VariableDecl for y"),
    }

    match &program.declarations[2].decl {
        DeclNode::Variable { name, init, .. } => {
            assert_eq!(name, "z");
            assert!(init.is_some(), "z should have an initializer");
            if let Expression::Binary { op, .. } = init.as_ref().unwrap() {
                assert_eq!(*op, BinaryOp::Add, "Expected Add operation for z");
            } else {
                panic!("Expected Binary expression for z init");
            }
        }
        _ => panic!("Expected VariableDecl for z"),
    }

    match &program.statements[0] {
        Statement::Return(Some(expr)) => {
            if let Expression::Primary(PrimaryExpr::Identifier(id)) = expr {
                assert_eq!(id, "z");
            } else {
                panic!("Expected return to yield identifier 'z'");
            }
        }
        _ => panic!("Expected Return statement"),
    }
}

#[test]
fn test2_hll_parser_success_and_ast_validation() {
    let program =
        parse_fixture("parser/02_structs_and_pointers.hll").expect("failed to parse test2.hll");

    assert_eq!(
        program.declarations.len(),
        2,
        "Expected 2 declarations (Node type, main function)"
    );

    use full_stack::high_level_language::ast::*;

    match &program.declarations[0].decl {
        DeclNode::Type { name, .. } => assert_eq!(name, "Node"),
        _ => panic!("Expected Type Node declaration"),
    }

    match &program.declarations[1].decl {
        DeclNode::Function { name, body, .. } => {
            assert_eq!(name, "main");
            assert!(body.is_some(), "main should have a block");
            let statements = &body.as_ref().unwrap().statements;
            // let ptr, x, addr, @ptr =, defer, if, return
            assert_eq!(statements.len(), 7, "Expected 7 statements in main");
        }
        _ => panic!("Expected Function main declaration"),
    }
}

#[test]
fn test3_hll_parser_success_and_ast_validation() {
    let program = parse_fixture("parser/03_nested_access_and_control_flow.hll")
        .expect("failed to parse test3.hll");

    assert_eq!(
        program.declarations.len(),
        2,
        "Expected Container, stress_test"
    );

    use full_stack::high_level_language::ast::*;

    // Verify type Container
    match &program.declarations[0].decl {
        DeclNode::Type { name, .. } => assert_eq!(name, "Container"),
        _ => panic!("Expected Type Container declaration"),
    }

    // Verify function stress_test
    match &program.declarations[1].decl {
        DeclNode::Function { name, .. } => assert_eq!(name, "stress_test"),
        _ => panic!("Expected Function stress_test declaration"),
    }
}

#[test]
fn test4_hll_parser_success_and_ast_validation() {
    let program = parse_fixture("parser/04_tuple_returns.hll").expect("failed to parse test4.hll");

    assert_eq!(program.declarations.len(), 2, "Expected divide, start");

    use full_stack::high_level_language::ast::*;

    match &program.declarations[0].decl {
        DeclNode::Function {
            name, return_type, ..
        } => {
            assert_eq!(name, "divide");
            assert!(
                matches!(return_type, Some(ReturnType::Single(Type::Struct(_)))),
                "divide should return an inline struct"
            );
        }
        _ => panic!("Expected Function divide declaration"),
    }

    match &program.declarations[1].decl {
        DeclNode::Function { name, body, .. } => {
            assert_eq!(name, "start");
            let block = body.as_ref().expect("start should have a body");
            let statements = &block.statements;
            assert_eq!(statements.len(), 2, "Expected 2 statements in start");
            // They are expression and if
            assert!(matches!(
                statements[0],
                Statement::Expression(Expression::Assignment { .. })
            ));
            assert!(matches!(statements[1], Statement::If { .. }));
        }
        _ => panic!("Expected Function start declaration"),
    }
}

#[test]
fn test5_hll_parser_reordered_and_partial_struct_destructuring() {
    let program = parse_fixture("parser/05_destructuring_order_and_partial_binding.hll")
        .expect("failed to parse test5.hll");

    assert_eq!(program.declarations.len(), 3, "Expected Pair, pair, main");

    use full_stack::high_level_language::ast::*;

    match &program.declarations[2].decl {
        DeclNode::Function { name, body, .. } => {
            assert_eq!(name, "main");
            let block = body.as_ref().expect("main should have a body");
            assert_eq!(
                block.statements.len(),
                3,
                "Expected two destructures and a return"
            );

            let first_assign = match &block.statements[0] {
                Statement::Expression(Expression::Assignment { target, .. }) => target,
                other => panic!("expected first destructuring assignment, got {other:?}"),
            };
            match first_assign.as_ref() {
                AssignTarget::StructDestructure(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, Some("second".to_string()));
                    assert_eq!(fields[1].name, Some("first".to_string()));
                }
                other => panic!("unexpected first destructuring target: {other:?}"),
            }

            let second_assign = match &block.statements[1] {
                Statement::Expression(Expression::Assignment { target, .. }) => target,
                other => panic!("expected second destructuring assignment, got {other:?}"),
            };
            match second_assign.as_ref() {
                AssignTarget::StructDestructure(fields) => {
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].name, Some("first".to_string()));
                }
                other => panic!("unexpected second destructuring target: {other:?}"),
            }
        }
        _ => panic!("Expected Function main declaration"),
    }
}

#[test]
fn test1_hll_compiles_to_ir_with_arithmetic() {
    let path = fixture_root().join("lexer/01_comments_and_newlines.hll");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());

    let tokens = lex_source(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("failed to parse test1.hll");

    let mut compiler = HighLevelCompiler::new();
    let ir_program = compiler
        .compile_program(&program)
        .expect("failed to compile test1.hll");

    let ir_text = format!("{}", ir_program);

    // test1.hll contains only variable declarations (no functions)
    // so the IR program will be minimal but should compile successfully
    // Just verify that compilation didn't panic and IR text can be generated
    println!(
        "=== test1.hll IR OUTPUT ===\n{}",
        if ir_text.is_empty() {
            "(empty - only declarations)"
        } else {
            &ir_text
        }
    );
}

#[test]
fn test2_hll_compiles_to_ir_with_pointers_and_structs() {
    let path = fixture_root().join("parser/02_structs_and_pointers.hll");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());

    let tokens = lex_source(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("failed to parse test2.hll");

    let mut compiler = HighLevelCompiler::new();
    let ir_program = compiler
        .compile_program(&program)
        .expect("failed to compile test2.hll");

    let ir_text = format!("{}", ir_program);

    // Verify IR contains the main function and pointer/struct operations
    assert!(
        ir_text.contains("define i32 main("),
        "IR should contain main function"
    );
    // The test has pointer types and field accesses, so we should see them in IR
    assert!(
        ir_text.contains("*") || ir_text.contains("Node"),
        "IR should contain pointer types or struct references"
    );

    // Snapshot the IR output
    println!("=== test2.hll IR OUTPUT ===\n{}", ir_text);
}

#[test]
fn test3_hll_compiles_to_ir_with_control_flow() {
    let path = fixture_root().join("parser/03_nested_access_and_control_flow.hll");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());

    let tokens = lex_source(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("failed to parse test3.hll");

    let mut compiler = HighLevelCompiler::new();
    let ir_program = compiler
        .compile_program(&program)
        .expect("failed to compile test3.hll");

    let ir_text = format!("{}", ir_program);

    // Verify IR contains control flow elements
    assert!(
        ir_text.contains("define i32 stress_test("),
        "IR should contain stress_test function"
    );
    // While loops are scaffolded to use Jump
    assert!(
        ir_text.contains("jump") || ir_text.contains("branch"),
        "IR should contain control flow instructions"
    );

    // Snapshot the IR output
    println!("=== test3.hll IR OUTPUT ===\n{}", ir_text);
}

#[test]
fn test4_hll_compiles_to_ir_with_multiple_returns() {
    let path = fixture_root().join("parser/04_tuple_returns.hll");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    let source: &'static str = Box::leak(source.into_boxed_str());

    let tokens = lex_source(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().expect("failed to parse test4.hll");

    let mut compiler = HighLevelCompiler::new();
    let ir_program = compiler
        .compile_program(&program)
        .expect("failed to compile test4.hll");

    let ir_text = format!("{}", ir_program);

    // Verify IR contains both functions
    assert!(
        ir_text.contains(" divide("),
        "IR should contain divide function"
    );
    assert!(
        ir_text.contains("define void start("),
        "IR should contain start function"
    );

    // Snapshot the IR output
    println!("=== test4.hll IR OUTPUT ===\n{}", ir_text);
}
