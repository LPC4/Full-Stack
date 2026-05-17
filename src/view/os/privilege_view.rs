// Privilege Level Timeline view - shows M/S/U privilege mode at every captured cycle

use crate::view::{CompilerView, CompilationState, ProgramCatalog, centered_placeholder};
use egui::Context;

#[derive(Clone)]
pub struct PrivilegeView;

impl CompilerView for PrivilegeView {
    fn title(&self) -> &'static str {
        "Privilege Timeline"
    }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &Context, _state: &mut CompilationState, _catalog: &mut ProgramCatalog) {
        centered_placeholder(ui, "Coming soon");
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
