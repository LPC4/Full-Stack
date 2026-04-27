use crate::view::{CompilerView, CompilationState, ProgramCatalog};
use crate::view::highlight_assembly;

pub struct AssemblyView;

impl CompilerView for AssemblyView {
    fn title(&self) -> &'static str { "Assembly" }
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, state: &mut CompilationState, _catalog: &mut ProgramCatalog) {
        let mut job = highlight_assembly(ui.style(), &state.asm);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.label(galley);
            });
    }
}