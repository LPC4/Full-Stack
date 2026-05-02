use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::ScrollArea;

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
            ui.vertical(|ui| {
                ui.add_space(20.0);
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("VM Execution Panel").heading());
                });
                ui.add_space(10.0);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Click \"Run in VM\" button above to execute").weak(),
                    );
                });
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("on the custom RISC-V virtual machine.").weak());
                });
            });
            return;
        }

        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            ui.label(egui::RichText::new(&state.vm_output).monospace());
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
