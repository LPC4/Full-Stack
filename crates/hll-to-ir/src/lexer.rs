use crate::token::{CompoundOp, Span, Token};
use std::iter::Peekable;
use std::str::Chars;

pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
    line: u32,
    line_start: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    /// Tokenize the entire input, returning tokens paired with their source spans.
    pub fn tokenize(input: &'a str) -> Vec<(Token<'a>, Span)> {
        let mut lexer = Self::new(input);
        let lines: Vec<&str> = input.lines().collect();
        let mut out = Vec::new();
        loop {
            let span_start_pos = lexer.pos;
            let span_line = lexer.line;
            let span_col = (span_start_pos - lexer.line_start + 1) as u32;
            let tok = lexer.next_token();
            let source_line = lines
                .get((span_line as usize).saturating_sub(1))
                .unwrap_or(&"")
                .to_string();
            let span = Span {
                line: span_line,
                col: span_col,
                source_line,
            };
            let is_eof = matches!(tok, Token::Eof);
            out.push((tok, span));
            if is_eof {
                break;
            }
        }
        out
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
            '\n' => {
                self.line += 1;
                self.line_start = self.pos;
                Token::StatementTerminator
            }

            // Comments
            ';' => {
                self.skip_comment();
                self.next_token()
            }

            // Punctuation & Operators
            ':' => {
                if self.peek_is('=') {
                    self.advance();
                    Token::ColonEqual
                } else {
                    Token::Colon
                }
            }
            ',' => Token::Comma,
            '.' => {
                if self.peek_is('.') {
                    self.advance();
                    if self.peek_is('=') {
                        self.advance();
                        Token::DotDotEq
                    } else {
                        Token::DotDot
                    }
                } else {
                    Token::Dot
                }
            }
            '+' => self.op_or_compound(CompoundOp::Add, Token::Plus),
            // `-=` compound; `->` stays Minus then Gt (handled by the parser).
            '-' => self.op_or_compound(CompoundOp::Sub, Token::Minus),
            '*' => self.op_or_compound(CompoundOp::Mul, Token::Star),
            '/' => self.op_or_compound(CompoundOp::Div, Token::Slash),
            '%' => self.op_or_compound(CompoundOp::Mod, Token::Percent),
            '@' => Token::At,
            '?' => Token::Question,
            '&' => {
                if self.peek_is('&') {
                    self.advance();
                    Token::And
                } else {
                    self.op_or_compound(CompoundOp::BitAnd, Token::Ampersand)
                }
            }
            '|' => {
                if self.peek_is('|') {
                    self.advance();
                    Token::Or
                } else {
                    self.op_or_compound(CompoundOp::BitOr, Token::Pipe)
                }
            }
            '^' => self.op_or_compound(CompoundOp::BitXor, Token::Caret),
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
                if self.peek_is('<') {
                    self.advance();
                    self.op_or_compound(CompoundOp::Shl, Token::Shl)
                } else if self.peek_is('=') {
                    self.advance();
                    Token::Lte
                } else {
                    Token::Lt
                }
            }
            '>' => {
                if self.peek_is('>') {
                    self.advance();
                    self.op_or_compound(CompoundOp::Shr, Token::Shr)
                } else if self.peek_is('=') {
                    self.advance();
                    Token::Gte
                } else {
                    Token::Gt
                }
            }

            // Literals & Keywords
            '"' => self.read_string(start),
            '\'' => self.read_char(),
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
            "struct" => Token::Struct,
            "const" => Token::Const,
            "external" => Token::External,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "in" => Token::In,
            "enum" => Token::Enum,
            "match" => Token::Match,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "return" => Token::Return,
            "defer" => Token::Defer,
            "new" => Token::New,
            "asm" => Token::Asm,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "as" => Token::As,
            "import" => Token::Import,
            "export" => Token::Export,
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
                    // A `.` followed by another `.` is the range operator, not a
                    // decimal point: leave it for `0..5` to tokenize as `0 .. 5`.
                    let mut ahead = self.chars.clone();
                    ahead.next();
                    if ahead.peek() == Some(&'.') {
                        break;
                    }
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

    // Read a `'c'` literal (opening quote already consumed) into its ascii byte.
    // Supports the common escapes; rejects empty, multi-char, and non-ascii.
    fn read_char(&mut self) -> Token<'a> {
        let value = match self.chars.next() {
            Some('\\') => {
                self.pos += 1;
                match self.chars.next() {
                    Some(e) => {
                        self.pos += e.len_utf8();
                        match e {
                            'n' => b'\n',
                            't' => b'\t',
                            'r' => b'\r',
                            'b' => 8,
                            '0' => 0,
                            '\\' => b'\\',
                            '\'' => b'\'',
                            '"' => b'"',
                            _ => return Token::Error(format!("unknown char escape: \\{}", e)),
                        }
                    }
                    None => return Token::Error("unterminated char literal".to_owned()),
                }
            }
            Some('\'') => return Token::Error("empty char literal".to_owned()),
            Some(c) if c.is_ascii() => {
                self.pos += c.len_utf8();
                c as u8
            }
            Some(c) => {
                self.pos += c.len_utf8();
                return Token::Error(format!("non-ascii char literal: {}", c));
            }
            None => return Token::Error("unterminated char literal".to_owned()),
        };
        match self.chars.next() {
            Some('\'') => {
                self.pos += 1;
                Token::Char(value)
            }
            _ => Token::Error("unterminated or multi-character char literal".to_owned()),
        }
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

    // Return the compound-assign token if the operator is followed by `=`,
    // otherwise the plain operator token.
    fn op_or_compound(&mut self, op: CompoundOp, plain: Token<'a>) -> Token<'a> {
        if self.peek_is('=') {
            self.advance();
            Token::CompoundAssign(op)
        } else {
            plain
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Lexer;
    use crate::token::Token;

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
        // Single | is now the bitwise OR operator (Token::Pipe)
        assert_eq!(lexer.next_token(), Token::Pipe);
        assert_eq!(lexer.next_token(), Token::Ident("e"));
    }

    #[test]
    fn test_single_pipe_is_bitwise_or() {
        let mut lexer = Lexer::new("|");
        // Single | is now the bitwise OR operator (Token::Pipe)
        assert_eq!(lexer.next_token(), Token::Pipe);
    }

    #[test]
    fn test_char_literals() {
        let input = "'A' '0' '\\n' '\\'' '\\\\'";
        let mut lexer = Lexer::new(input);
        assert_eq!(lexer.next_token(), Token::Char(65));
        assert_eq!(lexer.next_token(), Token::Char(48));
        assert_eq!(lexer.next_token(), Token::Char(10));
        assert_eq!(lexer.next_token(), Token::Char(39));
        assert_eq!(lexer.next_token(), Token::Char(92));
        assert_eq!(lexer.next_token(), Token::Eof);
    }

    #[test]
    fn test_new_keywords_v150() {
        let input = "as import export";
        let mut lexer = Lexer::new(input);
        assert_eq!(lexer.next_token(), Token::As);
        assert_eq!(lexer.next_token(), Token::Import);
        assert_eq!(lexer.next_token(), Token::Export);
        assert_eq!(lexer.next_token(), Token::Eof);
    }
}
