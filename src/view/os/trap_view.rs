// Trap Inspector view - shows current M-mode trap registers + scrollable history

use crate::view::{CompilationState, CompilerView, ProgramCatalog, centered_placeholder};
use egui::Context;

#[derive(Clone)]
pub struct TrapView;

impl CompilerView for TrapView {
    fn title(&self) -> &'static str {
        "Trap Inspector"
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
