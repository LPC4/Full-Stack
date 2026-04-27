use crate::view::{CompilerView, CompilationState, ProgramCatalog};
use crate::view::highlight_ir;

pub struct IrView;

impl CompilerView for IrView {
    fn title(&self) -> &'static str { "IR" }
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, state: &mut CompilationState, _catalog: &mut ProgramCatalog) {
        let mut job = highlight_ir(ui.style(), &state.ir);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.label(galley);
            });
    }
}