// PLIC + CLINT Inspector view - shows real-time state of interrupt controllers

use crate::view::{CompilationState, CompilerView, ProgramCatalog, centered_placeholder};
use egui::Context;

#[derive(Clone)]
pub struct InterruptView;

impl CompilerView for InterruptView {
    fn title(&self) -> &'static str {
        "Interrupt Controller"
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
