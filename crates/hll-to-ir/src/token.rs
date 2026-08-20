/// Source location attached to a token.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column (byte offset within the line).
    pub col: u32,
    /// Byte length of the spanned token; 0 means "point, do not underline".
    pub len: u32,
    /// The full source line (without the trailing newline).
    pub source_line: String,
}

impl Span {
    /// Format as `"line N, col M"`.
    pub fn location(&self) -> String {
        format!("line {}, col {}", self.line, self.col)
    }

    /// Format as `"line N | source text"`.
    pub fn display(&self) -> String {
        format!("line {} | {}", self.line, self.source_line)
    }

    /// This span narrowed to the first occurrence of `token` on its line, for
    /// a token-precise caret; unchanged when the token is not on the line.
    pub fn narrowed_to(&self, token: &str) -> Self {
        if token.is_empty() {
            return self.clone();
        }
        match self.source_line.find(token) {
            Some(idx) => Self {
                line: self.line,
                col: idx as u32 + 1,
                len: token.len() as u32,
                source_line: self.source_line.clone(),
            },
            None => self.clone(),
        }
    }

    /// Number of caret columns to draw under the token: the character count of
    /// the spanned bytes on its line, at least 1.
    pub fn caret_width(&self) -> usize {
        if self.len == 0 {
            return 1;
        }
        let start = (self.col as usize).saturating_sub(1);
        let end = start + self.len as usize;
        self.source_line
            .get(start..end)
            .map_or(1, |slice| slice.chars().count().max(1))
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, col {}", self.line, self.col)
    }
}

/// The arithmetic/bitwise operation in a compound assignment (`+=`, `<<=`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Ident(&'a str),
    Integer(&'a str),
    HexInteger(&'a str),
    BinInteger(&'a str),
    Float(&'a str),
    String(&'a str),
    Char(u8),

    // Keywords
    External,
    If,
    Else,
    While,
    For,
    In,
    Enum,
    Match,
    Break,
    Continue,
    Return,
    Defer,
    New,
    Asm,
    And,
    Or,
    True,
    False,
    Null,
    Fn,

    // Primitive Types
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,

    // Punctuation and Operators
    Colon,
    ColonEqual,
    Comma,
    Dot,
    DotDot,
    DotDotEq,
    Assign,
    // `?` postfix Result/Option propagation.
    Question,
    CompoundAssign(CompoundOp),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // Error
    Error(String),

    // Comparison
    Eq,
    Neq,
    Lt,
    Lte,
    Shl,
    Shr,
    Gt,
    Gte,
    Not,

    // Operators
    Ampersand,
    Caret,
    Pipe,
    Tilde,
    At,
    Dollar,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    StatementTerminator, // Mapped to \n

    Eof,
    Const,
    Type,
    Struct,
    Impl,
    Interface,
    Dyn,
    As,
    Import,
    Export,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stmt_span() -> Span {
        Span {
            line: 3,
            col: 5,
            len: 12,
            source_line: "    x := bogus".to_owned(),
        }
    }

    #[test]
    fn narrowed_to_points_at_the_token() {
        let narrowed = stmt_span().narrowed_to("bogus");
        assert_eq!(narrowed.col, 10);
        assert_eq!(narrowed.len, 5);
        assert_eq!(narrowed.line, 3);
    }

    #[test]
    fn narrowed_to_keeps_the_span_when_token_is_absent_or_empty() {
        assert_eq!(stmt_span().narrowed_to("missing"), stmt_span());
        assert_eq!(stmt_span().narrowed_to(""), stmt_span());
    }
}
