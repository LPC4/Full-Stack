pub mod highlighter;
pub mod layout;
pub mod common;
pub mod program_catalog;
pub mod viewtrait;
pub mod compilation_state;
pub mod source_view;
pub mod tokens_view;
pub mod ast_view;
pub mod ir_view;
pub mod assembly_view;

pub use highlighter::{highlight_code, highlight_ast, highlight_ir, highlight_assembly};
pub use layout::{auto_grid_columns, split_rect_into_grid, estimated_monospace_char_width};
pub use common::ViewType;
pub use program_catalog::{ProgramFile, ProgramKind, ProgramCatalog};
pub use viewtrait::CompilerView;
pub use compilation_state::CompilationState;
pub use source_view::SourceView;
pub use tokens_view::TokensView;
pub use ast_view::AstView;
pub use ir_view::IrView;
pub use assembly_view::AssemblyView;

pub(crate) fn blank_custom_program_source() -> String {
    "; Write your program here\n".to_string()
}