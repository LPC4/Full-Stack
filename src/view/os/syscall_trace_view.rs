// System Call Trace view - shows every ecall instruction with decoded syscall info

use crate::view::{CompilationState, CompilerView, ProgramCatalog, centered_placeholder};
use egui::Context;

#[derive(Clone)]
pub struct SyscallTraceView;

impl CompilerView for SyscallTraceView {
    fn title(&self) -> &'static str {
        "Syscall Trace"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &Context,
        _state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        centered_placeholder(ui, "Coming soon");
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
