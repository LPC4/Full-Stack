use crate::view::highlight_ast;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};

#[derive(Default, Clone)]
pub struct TokensView;

impl CompilerView for TokensView {
    fn title(&self) -> &'static str {
        "Tokens"
    }
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let mut job = highlight_ast(ui.style(), &state.tokens);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.label(galley);
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
