use full_stack::high_level_language::lexer::Lexer;
use full_stack::high_level_language::token::Token;

#[test]
fn test_basic_declaration() {
    let input = "x: i32 = 10\n";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("x"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::Integer("10"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_pointer_rules() {
    let input = "ptr: i32* = &x\nval: i32 = @ptr\n";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("ptr"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Star);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::Ampersand);
    assert_eq!(lexer.next_token(), Token::Ident("x"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);

    assert_eq!(lexer.next_token(), Token::Ident("val"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::At);
    assert_eq!(lexer.next_token(), Token::Ident("ptr"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);
}

#[test]
fn test_comments_and_whitespace() {
    let input = "x: i32 = 5 ; this is a comment\ny: i32 = 10\n";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("x"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::Integer("5"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);
    assert_eq!(lexer.next_token(), Token::Ident("y"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::Integer("10"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_memory_management() {
    let input = "ptr = new i32\ndefer free ptr\n";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("ptr"));
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::New);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::StatementTerminator);

    assert_eq!(lexer.next_token(), Token::Defer);
    assert_eq!(lexer.next_token(), Token::Ident("free"));
    assert_eq!(lexer.next_token(), Token::Ident("ptr"));
    assert_eq!(lexer.next_token(), Token::StatementTerminator);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_comment_at_eof_without_trailing_newline() {
    let input = "x: i32 = 5 ; comment at eof";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("x"));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::I32);
    assert_eq!(lexer.next_token(), Token::Assign);
    assert_eq!(lexer.next_token(), Token::Integer("5"));
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_hex_and_floats() {
    let input = "0xFF 3.14159";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::HexInteger("0xFF"));
    assert_eq!(lexer.next_token(), Token::Float("3.14159"));
}

#[test]
fn test_operators_and_punctuation() {
    let input = "== != <= >= < > ( ) { } [ ] . , : %";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Eq);
    assert_eq!(lexer.next_token(), Token::Neq);
    assert_eq!(lexer.next_token(), Token::Lte);
    assert_eq!(lexer.next_token(), Token::Gte);
    assert_eq!(lexer.next_token(), Token::Lt);
    assert_eq!(lexer.next_token(), Token::Gt);
    assert_eq!(lexer.next_token(), Token::LParen);
    assert_eq!(lexer.next_token(), Token::RParen);
    assert_eq!(lexer.next_token(), Token::LBrace);
    assert_eq!(lexer.next_token(), Token::RBrace);
    assert_eq!(lexer.next_token(), Token::LBracket);
    assert_eq!(lexer.next_token(), Token::RBracket);
    assert_eq!(lexer.next_token(), Token::Dot);
    assert_eq!(lexer.next_token(), Token::Comma);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Percent);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_keywords() {
    let input = "true false null and or type const";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::True);
    assert_eq!(lexer.next_token(), Token::False);
    assert_eq!(lexer.next_token(), Token::Null);
    assert_eq!(lexer.next_token(), Token::And);
    assert_eq!(lexer.next_token(), Token::Or);
    assert_eq!(lexer.next_token(), Token::Type);
    assert_eq!(lexer.next_token(), Token::Const);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_logical_operator_symbols() {
    let input = "a && b || c & d | e";
    let mut lexer = Lexer::new(input);

    assert_eq!(lexer.next_token(), Token::Ident("a"));
    assert_eq!(lexer.next_token(), Token::And);
    assert_eq!(lexer.next_token(), Token::Ident("b"));
    assert_eq!(lexer.next_token(), Token::Or);
    assert_eq!(lexer.next_token(), Token::Ident("c"));
    assert_eq!(lexer.next_token(), Token::Ampersand);
    assert_eq!(lexer.next_token(), Token::Ident("d"));
    assert_eq!(
        lexer.next_token(),
        Token::Error("Unexpected character: | at position 17".to_string())
    );
}

#[test]
fn test_single_pipe_is_invalid() {
    let mut lexer = Lexer::new("|");
    assert_eq!(
        lexer.next_token(),
        Token::Error("Unexpected character: | at position 1".to_string())
    );
}
