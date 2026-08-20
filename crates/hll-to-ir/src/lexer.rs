use crate::token::{CompoundOp, Span, Token};
use std::iter::Peekable;
use std::str::Chars;

pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
    line: u32,
    line_start: usize,
    // The last token's start, captured after whitespace so spans point at the
    // token itself, not the run of spaces before it.
    tok_start: usize,
    tok_line: u32,
    tok_line_start: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            pos: 0,
            line: 1,
            line_start: 0,
            tok_start: 0,
            tok_line: 1,
            tok_line_start: 0,
        }
    }

    /// Tokenize the entire input, returning tokens paired with their source spans.
    pub fn tokenize(input: &'a str) -> Vec<(Token<'a>, Span)> {
        let mut lexer = Self::new(input);
        let lines: Vec<&str> = input.lines().collect();
        let mut out = Vec::new();
        loop {
            let tok = lexer.next_token();
            // Spans read from the recorded token start (post-whitespace) and the
            // current position, which sits just past the token just produced.
            let span_line = lexer.tok_line;
            let span_col = (lexer.tok_start - lexer.tok_line_start + 1) as u32;
            let span_len = (lexer.pos - lexer.tok_start) as u32;
            let source_line = lines
                .get((span_line as usize).saturating_sub(1))
                .unwrap_or(&"")
                .to_string();
            let span = Span {
                line: span_line,
                col: span_col,
                len: span_len,
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
        // Record the real token start so `tokenize` reads col/len from here even
        // across the comment recursion below (which re-enters `next_token`).
        self.tok_start = start;
        self.tok_line = self.line;
        self.tok_line_start = self.line_start;
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
            '$' => Token::Dollar,
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
            '~' => Token::Tilde,
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
            "impl" => Token::Impl,
            "interface" => Token::Interface,
            "dyn" => Token::Dyn,
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
            "fn" => Token::Fn,
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
        let mut is_float = false;

        // `0x`/`0b` prefixed literal: scan digits of that radix only.
        if self.input[start..].starts_with('0')
            && let Some(radix) = self.chars.peek().and_then(|c| match c {
                'x' | 'X' => Some(16),
                'b' | 'B' => Some(2),
                _ => None,
            })
        {
            self.advance(); // consume the radix letter
            let digits_start = self.pos;
            while let Some(&c) = self.chars.peek() {
                if c == '_' || c.is_digit(radix) {
                    self.advance();
                } else {
                    break;
                }
            }
            let digits = &self.input[digits_start..self.pos];
            let text = &self.input[start..self.pos];
            if digits.is_empty() {
                return Token::Error(format!("numeric literal `{text}` has no digits"));
            }
            if let Err(message) = check_digit_separators(digits, text) {
                return Token::Error(message);
            }
            return if radix == 16 {
                Token::HexInteger(text)
            } else {
                Token::BinInteger(text)
            };
        }

        // Standard integer or float
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() || c == '_' {
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

        let text = &self.input[start..self.pos];
        for run in text.split('.') {
            if let Err(message) = check_digit_separators(run, text) {
                return Token::Error(message);
            }
        }
        if is_float {
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
                            _ => return Token::Error(format!("unknown char escape: \\{e}")),
                        }
                    }
                    None => return Token::Error("unterminated char literal".to_owned()),
                }
            }
            Some('\'') => {
                self.pos += 1;
                return Token::Error("empty char literal".to_owned());
            }
            Some(c) if c.is_ascii() => {
                self.pos += c.len_utf8();
                c as u8
            }
            Some(c) => {
                self.pos += c.len_utf8();
                return Token::Error(format!("non-ascii char literal: {c}"));
            }
            None => return Token::Error("unterminated char literal".to_owned()),
        };
        match self.chars.next() {
            Some('\'') => {
                self.pos += 1;
                Token::Char(value)
            }
            Some(c) => {
                self.pos += c.len_utf8();
                Token::Error("unterminated or multi-character char literal".to_owned())
            }
            None => Token::Error("unterminated or multi-character char literal".to_owned()),
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

// Digit separators are insignificant but must sit between digits: a leading,
// trailing, or doubled `_` is rejected. An empty run is the caller's concern.
fn check_digit_separators(digits: &str, literal: &str) -> Result<(), String> {
    if digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return Err(format!(
            "digit separator `_` in `{literal}` must sit between digits"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Lexer;
    use crate::token::Token;

    #[test]
    fn spans_point_at_the_token_with_its_width() {
        // Leading indentation must not bleed into the token's col; each span's
        // len is the token's byte length.
        let spans = Lexer::tokenize("  return bogus\n");
        let (tok, ret) = &spans[0];
        assert_eq!(*tok, Token::Return);
        assert_eq!((ret.line, ret.col, ret.len), (1, 3, 6));
        let (tok, ident) = &spans[1];
        assert_eq!(*tok, Token::Ident("bogus"));
        assert_eq!((ident.line, ident.col, ident.len), (1, 10, 5));
        assert_eq!(ident.source_line, "  return bogus");
    }

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
        assert_eq!(lexer.next_token(), Token::Pipe);
        assert_eq!(lexer.next_token(), Token::Ident("e"));
    }

    #[test]
    fn test_single_pipe_is_bitwise_or() {
        let mut lexer = Lexer::new("|");
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
    fn test_binary_literals_and_digit_separators() {
        let mut lexer = Lexer::new("0b1010 0B11 1_000_000 0x8000_0000 3.141_59");
        assert_eq!(lexer.next_token(), Token::BinInteger("0b1010"));
        assert_eq!(lexer.next_token(), Token::BinInteger("0B11"));
        assert_eq!(lexer.next_token(), Token::Integer("1_000_000"));
        assert_eq!(lexer.next_token(), Token::HexInteger("0x8000_0000"));
        assert_eq!(lexer.next_token(), Token::Float("3.141_59"));
        assert_eq!(lexer.next_token(), Token::Eof);
    }

    #[test]
    fn test_misplaced_digit_separators_are_errors() {
        for input in ["1_", "1__0", "0x_FF", "0b", "1_.5"] {
            let mut lexer = Lexer::new(input);
            assert!(
                matches!(lexer.next_token(), Token::Error(_)),
                "expected `{input}` to be rejected"
            );
        }
    }

    #[test]
    fn test_tilde_is_bitwise_not() {
        let mut lexer = Lexer::new("~mask");
        assert_eq!(lexer.next_token(), Token::Tilde);
        assert_eq!(lexer.next_token(), Token::Ident("mask"));
    }

    #[test]
    fn test_new_keywords() {
        let input = "as import export";
        let mut lexer = Lexer::new(input);
        assert_eq!(lexer.next_token(), Token::As);
        assert_eq!(lexer.next_token(), Token::Import);
        assert_eq!(lexer.next_token(), Token::Export);
        assert_eq!(lexer.next_token(), Token::Eof);
    }
}
