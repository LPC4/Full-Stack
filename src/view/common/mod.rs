pub mod highlighter;
pub mod theme;
pub mod widgets;

pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
pub use theme::*;
pub use widgets::*;
