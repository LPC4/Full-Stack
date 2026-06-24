/// Source location attached to a token.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column (byte offset within the line).
    pub col: u32,
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
    Float(&'a str),
    String(&'a str),
    // A `'c'` literal, lexed to its ascii byte value.
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
    // `?` postfix Result/Option propagation (V2).
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
    At,
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
    As,
    Import,
    Export,
}
