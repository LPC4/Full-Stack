pub mod highlighter;
pub mod theme;
pub mod widgets;

pub use theme::*;
pub use widgets::*;
pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
