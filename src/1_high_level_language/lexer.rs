use crate::high_level_language::token::Token;
use std::iter::Peekable;
use std::str::Chars;

pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            pos: 0,
        }
    }

    pub fn next_token(&mut self) -> Token<'a> {
        self.skip_whitespace_except_newline();

        let start = self.pos;
        let c = match self.chars.next() {
            Some(c) => {
                let len = c.len_utf8();
                self.pos += len;
                c
            }
            None => return Token::Eof,
        };

        match c {
            // Significant Newline
            '\n' => Token::StatementTerminator,

            // Comments
            ';' => {
                self.skip_comment();
                self.next_token()
            }

            // Punctuation & Operators
            ':' => Token::Colon,
            ',' => Token::Comma,
            '.' => Token::Dot,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '@' => Token::At,
            '&' => {
                if self.peek_is('&') {
                    self.advance();
                    Token::And
                } else {
                    Token::Ampersand
                }
            }
            '|' => {
                if self.peek_is('|') {
                    self.advance();
                    Token::Or
                } else {
                    Token::Error(format!(
                        "Unexpected character: {} at position {}",
                        c, self.pos
                    ))
                }
            }
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,

            // Lookahead Operators
            '=' => {
                if self.peek_is('=') {
                    self.advance();
                    Token::Eq
                } else {
                    Token::Assign
                }
            }
            '!' => {
                if self.peek_is('=') {
                    self.advance();
                    Token::Neq
                } else {
                    Token::Not
                }
            }
            '<' => {
                if self.peek_is('=') {
                    self.advance();
                    Token::Lte
                } else {
                    Token::Lt
                }
            }
            '>' => {
                if self.peek_is('=') {
                    self.advance();
                    Token::Gte
                } else {
                    Token::Gt
                }
            }

            // Literals & Keywords
            '"' => self.read_string(start),
            '0'..='9' => self.read_number(start),
            'a'..='z' | 'A'..='Z' | '_' => self.read_identifier(start),

            _ => Token::Error(format!(
                "Unexpected character: {} at position {}",
                c, self.pos
            )),
        }
    }

    fn read_identifier(&mut self, start: usize) -> Token<'a> {
        while let Some(&c) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.pos];
        match text {
            "type" => Token::Type,
            "const" => Token::Const,
            "external" => Token::External,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "return" => Token::Return,
            "defer" => Token::Defer,
            "new" => Token::New,
            "free" => Token::Free,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            // Types
            "i8" => Token::I8,
            "i16" => Token::I16,
            "i32" => Token::I32,
            "i64" => Token::I64,
            "u8" => Token::U8,
            "u16" => Token::U16,
            "u32" => Token::U32,
            "u64" => Token::U64,
            "f32" => Token::F32,
            "f64" => Token::F64,
            "bool" => Token::Bool,
            _ => Token::Ident(text),
        }
    }

    fn read_number(&mut self, start: usize) -> Token<'a> {
        let mut is_hex = false;
        let mut is_float = false;

        // Check for Hex prefix
        if self.input[start..].starts_with('0') && self.peek_is('x') {
            self.advance(); // consume 'x'
            is_hex = true;
            while let Some(&c) = self.chars.peek() {
                if c.is_ascii_hexdigit() {
                    self.advance();
                } else {
                    break;
                }
            }
        } else {
            // Standard integer or float
            while let Some(&c) = self.chars.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else if c == '.' && !is_float {
                    is_float = true;
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let text = &self.input[start..self.pos];
        if is_hex {
            Token::HexInteger(text)
        } else if is_float {
            Token::Float(text)
        } else {
            Token::Integer(text)
        }
    }

    fn read_string(&mut self, start: usize) -> Token<'a> {
        // Consume characters until closing quote
        loop {
            match self.chars.next() {
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    // Skip escape sequence (consume next char)
                    self.pos += 1; // for backslash
                    if let Some(c) = self.chars.next() {
                        self.pos += c.len_utf8();
                    }
                }
                Some(c) => {
                    self.pos += c.len_utf8();
                }
                None => {
                    return Token::Error("Unterminated string literal".to_string());
                }
            }
        }

        // Extract the string content (including quotes)
        let text = &self.input[start..self.pos];
        Token::String(text)
    }

    fn skip_whitespace_except_newline(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() && c != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn advance(&mut self) {
        if let Some(c) = self.chars.next() {
            self.pos += c.len_utf8();
        }
    }

    fn peek_is(&mut self, expected: char) -> bool {
        self.chars.peek() == Some(&expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // @ for deref, & for address-of, * for pointer type
        let input = "ptr: i32* = &x\nval: i32 = @ptr\n";
        let mut lexer = Lexer::new(input);

        // ptr: i32* = &x
        assert_eq!(lexer.next_token(), Token::Ident("ptr"));
        assert_eq!(lexer.next_token(), Token::Colon);
        assert_eq!(lexer.next_token(), Token::I32);
        assert_eq!(lexer.next_token(), Token::Star);
        assert_eq!(lexer.next_token(), Token::Assign);
        assert_eq!(lexer.next_token(), Token::Ampersand);
        assert_eq!(lexer.next_token(), Token::Ident("x"));
        assert_eq!(lexer.next_token(), Token::StatementTerminator);

        // val: i32 = @ptr
        assert_eq!(lexer.next_token(), Token::Ident("val"));
        assert_eq!(lexer.next_token(), Token::Colon);
        assert_eq!(lexer.next_token(), Token::I32);
        assert_eq!(lexer.next_token(), Token::Assign);
        assert_eq!(lexer.next_token(), Token::At); // Rule 2
        assert_eq!(lexer.next_token(), Token::Ident("ptr"));
        assert_eq!(lexer.next_token(), Token::StatementTerminator);
    }

    #[test]
    fn test_comments_and_whitespace() {
        // Semicolons are comments
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
        // 'new', 'free', and 'defer'
        let input = "ptr = new i32\ndefer free ptr\n";
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next_token(), Token::Ident("ptr"));
        assert_eq!(lexer.next_token(), Token::Assign);
        assert_eq!(lexer.next_token(), Token::New);
        assert_eq!(lexer.next_token(), Token::I32);
        assert_eq!(lexer.next_token(), Token::StatementTerminator);

        assert_eq!(lexer.next_token(), Token::Defer);
        assert_eq!(lexer.next_token(), Token::Free);
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
}
