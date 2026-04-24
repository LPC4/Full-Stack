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
                    return Token::Error("Unterminated string literal".to_owned());
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
