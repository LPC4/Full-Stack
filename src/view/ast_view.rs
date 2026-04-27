use crate::view::{CompilerView, CompilationState};
use crate::view::highlight_ast;

pub struct AstView;

impl CompilerView for AstView {
    fn title(&self) -> &'static str { "AST" }
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, state: &mut CompilationState) {
        let mut job = highlight_ast(ui.style(), &state.ast);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.label(galley);
            });
    }
}