// Common types and constants shared across views

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewType {
    Source,
    Tokens,
    AST,
    IR,
    Assembly,
}

impl ViewType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "Source Code",
            Self::Tokens => "Lexer Tokens",
            Self::AST => "Parser AST",
            Self::IR => "Intermediate Repr.",
            Self::Assembly => "Assembly Code",
        }
    }
}
