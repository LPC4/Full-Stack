pub mod common;
pub mod compilation_state;
pub mod highlighter;
pub mod layout;
pub mod program_catalog;
pub mod viewtrait;

pub use common::ViewType;
pub use compilation_state::CompilationState;
pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
pub use layout::{auto_grid_columns, estimated_monospace_char_width, split_rect_into_grid};
pub use program_catalog::{ProgramCatalog, ProgramFile, ProgramKind};
pub use viewtrait::CompilerView;

pub mod views;
pub use crate::view::views::assembly_view::AssemblyView;
pub use crate::view::views::ast_view::AstView;
pub use crate::view::views::execution_view::ExecutionView;
pub use crate::view::views::ir_view::IrView;
pub use crate::view::views::source_view::SourceView;
pub use crate::view::views::stack_view::StackView;
pub use crate::view::views::tokens_view::TokensView;

pub(crate) fn blank_custom_program_source() -> String {
    "; Write your program here\n".to_owned()
}
