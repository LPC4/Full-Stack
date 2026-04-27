use crate::view::{CompilerView, CompilationState, ProgramCatalog};
use crate::view::highlight_code;
use egui::{TextEdit, TextStyle};

pub struct SourceView {
    catalog: ProgramCatalog,
    source_code: String,
}

impl SourceView {
    pub fn new(catalog: ProgramCatalog) -> Self {
        let source_code = catalog.get_selected_source();
        Self { catalog, source_code }
    }
}

impl CompilerView for SourceView {
    fn title(&self) -> &'static str {
        "Source"
    }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, _state: &mut CompilationState) {
        let mut layouter = |ui: &egui::Ui, string: &dyn egui::TextBuffer, _wrap: f32| {
            let mut job = highlight_code(ui.style(), string.as_str());
            job.wrap.max_width = f32::INFINITY;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let response = TextEdit::multiline(&mut self.source_code)
            .font(TextStyle::Monospace)
            .code_editor()
            .lock_focus(true)
            .layouter(&mut layouter)
            .show(ui);
        if response.response.changed() {
            self.catalog.set_selected_source(self.source_code.clone());
        }
    }
}