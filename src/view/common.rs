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
            ViewType::Source => "Source Code",
            ViewType::Tokens => "Lexer Tokens",
            ViewType::AST => "Parser AST",
            ViewType::IR => "Intermediate Repr.",
            ViewType::Assembly => "Assembly Code",
        }
    }
}
