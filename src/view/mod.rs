pub mod assembly_view;
pub mod ast_view;
pub mod common;
pub mod compilation_state;
pub mod highlighter;
pub mod ir_view;
pub mod layout;
pub mod program_catalog;
pub mod source_view;
pub mod tokens_view;
pub mod viewtrait;

pub use assembly_view::AssemblyView;
pub use ast_view::AstView;
pub use common::ViewType;
pub use compilation_state::CompilationState;
pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
pub use ir_view::IrView;
pub use layout::{auto_grid_columns, estimated_monospace_char_width, split_rect_into_grid};
pub use program_catalog::{ProgramCatalog, ProgramFile, ProgramKind};
pub use source_view::SourceView;
pub use tokens_view::TokensView;
pub use viewtrait::CompilerView;

pub(crate) fn blank_custom_program_source() -> String {
    "; Write your program here\n".to_string()
}
