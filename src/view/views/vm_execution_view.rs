use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{RichText, ScrollArea};

#[derive(Default, Clone)]
pub struct VmExecutionView;

impl CompilerView for VmExecutionView {
    fn title(&self) -> &'static str {
        "VM Output"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        if state.vm_output.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Compile a program to see VM output.").weak());
            });
            return;
        }

        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            ui.label(RichText::new(&state.vm_output).monospace());
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
