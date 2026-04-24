#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Ident(&'a str),
    Integer(&'a str),
    HexInteger(&'a str),
    Float(&'a str),
    String(&'a str),

    // Keywords
    TypeKeyword,
    ConstKeyword,
    External,
    If,
    Else,
    While,
    Break,
    Continue,
    Return,
    Defer,
    New,
    Free,
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
    Semicolon, // Starts comment
    Comma,
    Dot,
    Assign,
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
    Gt,
    Gte,
    Not,

    // Operators
    Ampersand,
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
}
