use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{RichText, ScrollArea};

#[derive(Default, Clone)]
pub struct ExecutionView;

impl CompilerView for ExecutionView {
    fn title(&self) -> &'static str {
        "Execution (WSL)"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        if state.execution_output.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Run the program to see WSL output.").weak());
            });
            return;
        }

        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            ui.label(RichText::new(&state.execution_output).monospace());
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
