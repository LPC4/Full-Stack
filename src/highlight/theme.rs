use egui::Color32;

#[expect(
    dead_code,
    reason = "the shared palette also contains IDE surface tokens"
)]
mod palette {
    use egui::Color32;

    include!(concat!(env!("OUT_DIR"), "/theme_palette.rs"));
}

use palette::{
    SYNTAX_BRACKET, SYNTAX_BUILTIN_TYPE, SYNTAX_COMMENT, SYNTAX_DIRECTIVE, SYNTAX_FUNCTION,
    SYNTAX_GENERIC, SYNTAX_IDENTIFIER, SYNTAX_KEYWORD, SYNTAX_LABEL, SYNTAX_NUMBER,
    SYNTAX_REGISTER, SYNTAX_STRING, SYNTAX_TYPE, SYNTAX_VARIANT,
};

/// Token colors used by the syntax highlighters. A future editor can swap this
/// for a user theme; for now a single dark palette is provided.
#[derive(Clone, Copy, Debug)]
pub struct SyntaxPalette {
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub label: Color32,
    pub register: Color32,
    pub bracket: Color32,
    pub directive: Color32,
    pub identifier: Color32,
    pub builtin_type: Color32,
    pub ty: Color32,
    pub generic: Color32,
    pub variant: Color32,
    pub function: Color32,
}

impl SyntaxPalette {
    pub fn dark() -> Self {
        Self {
            keyword: SYNTAX_KEYWORD,
            string: SYNTAX_STRING,
            number: SYNTAX_NUMBER,
            comment: SYNTAX_COMMENT,
            label: SYNTAX_LABEL,
            register: SYNTAX_REGISTER,
            bracket: SYNTAX_BRACKET,
            directive: SYNTAX_DIRECTIVE,
            identifier: SYNTAX_IDENTIFIER,
            builtin_type: SYNTAX_BUILTIN_TYPE,
            ty: SYNTAX_TYPE,
            generic: SYNTAX_GENERIC,
            variant: SYNTAX_VARIANT,
            function: SYNTAX_FUNCTION,
        }
    }
}

impl Default for SyntaxPalette {
    fn default() -> Self {
        Self::dark()
    }
}
