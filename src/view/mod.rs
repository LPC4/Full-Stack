pub mod common;
pub mod compilation_state;
pub mod debug;
pub mod highlighter;
pub mod layout;
pub mod program_catalog;
pub mod viewtrait;

pub use common::{
    MemoryPalette, PipelinePalette, StackPalette, SyntaxPalette, UiTheme, ViewType, apply_ui_theme,
    ui_theme,
};
pub use compilation_state::CompilationState;
pub use highlighter::{highlight_assembly, highlight_ast, highlight_code, highlight_ir};
pub use layout::{
    auto_grid_columns, auto_grid_columns_with_min_width, estimated_monospace_char_width,
    split_rect_into_grid,
};
pub use program_catalog::{ProgramCatalog, ProgramFile, ProgramKind};
pub use viewtrait::CompilerView;

pub mod ide;
pub use crate::view::debug::{
    CacheView, CpuStateView, DisassemblyView, FramebufferView, IoView, MemoryView, PipelineView,
};
pub use crate::view::ide::AssemblyView;
pub use crate::view::ide::AstView;
pub use crate::view::ide::CfgView;
pub use crate::view::ide::ExecutionView;
pub use crate::view::ide::IrView;
pub use crate::view::ide::KernelView;
pub use crate::view::ide::MemoryMapView;
pub use crate::view::ide::SourceView;
pub use crate::view::ide::StackView;
pub use crate::view::ide::TokensView;
pub use crate::view::ide::VmExecutionView;

pub(crate) fn blank_custom_program_source() -> String {
    "; Write your program here\n".to_owned()
}
