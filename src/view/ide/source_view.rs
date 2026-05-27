use crate::view::highlight_code;
use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Frame, RichText, TextEdit, TextStyle};

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
        let theme = ui_theme();
        let mut source_code = catalog.get_selected_source();
        let is_stdlib = catalog
            .current_program()
            .map(|p| p.is_stdlib())
            .unwrap_or(false);
        let is_os = catalog
            .current_program()
            .map(|p| p.is_os())
            .unwrap_or(false);
        let is_readonly = is_stdlib || is_os;

        // Compact info chip for read-only programs
        if is_readonly {
            let (chip_label, hint) = if is_os {
                (
                    "os",
                    "read-only: select Kernel mode and open the Machine window to boot",
                )
            } else {
                (
                    "stdlib",
                    "read-only: compile to inspect tokens, AST, IR, and assembly",
                )
            };
            Frame::NONE
                .fill(theme.surface_alt)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(chip_label)
                                .small()
                                .strong()
                                .color(theme.text_dim),
                        );
                        ui.label(RichText::new("|").small().color(theme.border));
                        ui.label(RichText::new(hint).small().color(theme.text_dim));
                    });
                });
            ui.add_space(2.0);
        }

        let mut layouter = |ui: &egui::Ui, string: &dyn egui::TextBuffer, _wrap: f32| {
            let mut job = highlight_code(ui.style(), string.as_str());
            job.wrap.max_width = f32::INFINITY;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        // Reserve space for error panel if there's an error.
        let has_error = state.error.is_some();
        let available = ui.available_size();
        let error_panel_height = if has_error {
            (available.y * 0.28).clamp(80.0, 220.0)
        } else {
            0.0
        };
        let editor_height = (available.y - error_panel_height).max(50.0);

        let frame = Frame::NONE.fill(theme.panel).inner_margin(4.0);

        let panel_id = ui.id();
        frame.show(ui, |ui| {
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
                            .layouter(&mut layouter)
                            .interactive(!is_readonly),
                    );

                    if !is_readonly && response.changed() {
                        catalog.replace_selected_source_with_history(source_code);
                    }
                });
        });

        if let Some(error_text) = &state.error {
            let error_text = error_text.clone();
            ui.add_space(2.0);

            let error_frame = theme.alert_frame(theme.error.gamma_multiply(0.12), theme.error);

            error_frame.show(ui, |ui| {
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt(panel_id.with("error_scroll"))
                    .max_height((error_panel_height - 30.0).max(20.0))
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
                                        theme.error,
                                        egui::RichText::new(line.trim()).monospace(),
                                    );
                                });
                            } else if line.trim_start().starts_with("  |") {
                                // Source snippet line
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(
                                        theme.info,
                                        egui::RichText::new(line).monospace(),
                                    );
                                });
                            } else {
                                // Header / main message
                                ui.colored_label(
                                    theme.error,
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
