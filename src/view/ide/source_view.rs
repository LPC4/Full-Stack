use crate::view::highlight_code;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Frame, Key, TextEdit, TextStyle};

#[derive(Default, Clone)]
pub struct SourceView;

impl CompilerView for SourceView {
    fn title(&self) -> &'static str {
        "Source"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        catalog: &mut ProgramCatalog,
    ) {
        let mut source_code = catalog.get_selected_source();

        let undo_shortcut = ui.input(|i| i.modifiers.command && i.key_pressed(Key::Z));
        let redo_shortcut = ui.input(|i| {
            (i.modifiers.command && i.key_pressed(Key::Y))
                || (i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Z))
        });

        if undo_shortcut && catalog.undo_selected_source() {
            source_code = catalog.get_selected_source();
        }

        if redo_shortcut && catalog.redo_selected_source() {
            source_code = catalog.get_selected_source();
        }

        let mut layouter = |ui: &egui::Ui, string: &dyn egui::TextBuffer, _wrap: f32| {
            let mut job = highlight_code(ui.style(), string.as_str());
            job.wrap.max_width = f32::INFINITY;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        // Reserve space for error panel if there's an error.
        let has_error = state.error.is_some();
        let error_panel_height = if has_error { 140.0 } else { 0.0 };
        let available = ui.available_size();
        let editor_height = (available.y - error_panel_height).max(50.0);

        let frame = Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4.0);

        let panel_id = ui.id();
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let can_undo = catalog.can_undo_selected_source();
                if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                    if catalog.undo_selected_source() {
                        source_code = catalog.get_selected_source();
                    }
                }

                let can_redo = catalog.can_redo_selected_source();
                if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                    if catalog.redo_selected_source() {
                        source_code = catalog.get_selected_source();
                    }
                }
            });

            egui::ScrollArea::both()
                .id_salt(panel_id.with("source_editor_scroll"))
                .auto_shrink([false; 2])
                .max_height(editor_height)
                .show(ui, |ui| {
                    let response = ui.add(
                        TextEdit::multiline(&mut source_code)
                            .font(TextStyle::Monospace)
                            .frame(Frame::NONE)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .min_size(egui::vec2(available.x, editor_height))
                            .layouter(&mut layouter),
                    );

                    if response.changed() {
                        catalog.replace_selected_source_with_history(source_code);
                    }
                });
        });

        if let Some(error_text) = &state.error {
            let error_text = error_text.clone();
            ui.add_space(2.0);

            let error_frame = Frame::NONE
                .fill(egui::Color32::from_rgb(40, 15, 15))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(160, 50, 50)))
                .inner_margin(8.0)
                .corner_radius(4.0);

            error_frame.show(ui, |ui| {
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt(panel_id.with("error_scroll"))
                    .max_height(error_panel_height - 30.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for line in error_text.lines() {
                            if line.trim().is_empty() {
                                ui.add_space(2.0);
                            } else if line.trim_start().starts_with("- ")
                                || line.trim_start().starts_with("  -")
                            {
                                // Bullet error entries
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        egui::Color32::from_rgb(255, 120, 100),
                                        egui::RichText::new(line.trim()).monospace(),
                                    );
                                });
                            } else if line.trim_start().starts_with("  |") {
                                // Source snippet line
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        egui::Color32::from_rgb(150, 150, 200),
                                        egui::RichText::new(line).monospace(),
                                    );
                                });
                            } else {
                                // Header / main message
                                ui.colored_label(
                                    egui::Color32::from_rgb(240, 100, 80),
                                    egui::RichText::new(line).monospace().strong(),
                                );
                            }
                        }
                    });
            });
        }
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
