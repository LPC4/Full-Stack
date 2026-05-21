// Sv39 Page Table Walker view - shows full three-level PTE walk

use crate::view::{CompilationState, CompilerView, ProgramCatalog, centered_placeholder};
use egui::Context;

#[derive(Clone)]
pub struct PageTableView;

impl CompilerView for PageTableView {
    fn title(&self) -> &'static str {
        "Page Table Walker"
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
