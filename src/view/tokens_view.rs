use crate::view::{CompilerView, CompilationState};
use crate::view::highlight_ast;

pub struct TokensView;

impl CompilerView for TokensView {
    fn title(&self) -> &'static str { "Tokens" }
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, state: &mut CompilationState) {
        let mut job = highlight_ast(ui.style(), &state.tokens);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.label(galley);
            });
    }
}