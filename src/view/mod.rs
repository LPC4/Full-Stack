pub mod common;
pub mod compilation_state;
pub mod debug;
pub mod highlighter;
pub mod layout;
pub mod program_catalog;
pub mod viewtrait;

pub use common::ViewType;
pub use compilation_state::CompilationState;
pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
pub use layout::{
    auto_grid_columns, auto_grid_columns_with_min_width, estimated_monospace_char_width,
    split_rect_into_grid,
};
pub use program_catalog::{ProgramCatalog, ProgramFile, ProgramKind};
pub use viewtrait::CompilerView;

pub mod views;
pub use crate::view::debug::{
    CacheView, CpuStateView, FramebufferView, IoView, MemoryView, PipelineView,
};
pub use crate::view::views::AssemblyView;
pub use crate::view::views::AstView;
pub use crate::view::views::CfgView;
pub use crate::view::views::ExecutionView;
pub use crate::view::views::IrView;
pub use crate::view::views::MemoryMapView;
pub use crate::view::views::SourceView;
pub use crate::view::views::StackView;
pub use crate::view::views::TokensView;
pub use crate::view::views::VmExecutionView;

pub(crate) fn blank_custom_program_source() -> String {
    "; Write your program here\n".to_owned()
}
