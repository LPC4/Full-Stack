use crate::view::highlight_code;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Frame, TextEdit, TextStyle};

pub struct SourceView;

impl CompilerView for SourceView {
    fn title(&self) -> &'static str {
        "Source"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        _state: &mut CompilationState,
        catalog: &mut ProgramCatalog,
    ) {
        let mut source_code = catalog.get_selected_source();

        let mut layouter = |ui: &egui::Ui, string: &dyn egui::TextBuffer, _wrap: f32| {
            let mut job = highlight_code(ui.style(), string.as_str());
            job.wrap.max_width = f32::INFINITY; // Disable word wrap
            ui.fonts_mut(|f| f.layout_job(job))
        };

        let frame = Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4.0);

        frame.show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let response = ui.add(
                        TextEdit::multiline(&mut source_code)
                            .font(TextStyle::Monospace)
                            .frame(Frame::NONE)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .min_size(ui.available_size())
                            .layouter(&mut layouter),
                    );

                    if response.changed() {
                        catalog.set_selected_source(source_code);
                    }
                });
        });
    }
}
