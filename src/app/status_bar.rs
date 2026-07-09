use super::{FullStackApp, metrics};
use crate::ui::theme;
use eframe::egui;

impl FullStackApp {
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        // The dot mirrors the machine state; the label names the selected config.
        let dot_color = match &self.live {
            Some(live) => super::machine_dock::state_style(&live.state).1,
            None => theme::TEXT_FAINT,
        };
        let config_label = self
            .selected_config()
            .map(|config| {
                format!(
                    "{} \u{00B7} {}",
                    super::tool_window::kind_tag(config.kind),
                    config.name
                )
            })
            .unwrap_or_else(|| "no run config".to_owned());
        let (errors, warnings) = self
            .stage_cache
            .as_ref()
            .map(|cache| super::diagnostics::count_levels(&cache.result.diagnostics))
            .unwrap_or((0, 0));
        ui.horizontal_centered(|ui| {
            ui.add_space(metrics::PANEL_TOP_PADDING);
            ui.label(theme::mono("\u{25CF}", metrics::STATUS_DOT_SIZE, dot_color));
            ui.label(theme::label(&config_label));
            ui.label(theme::mono(
                "\u{00B7}",
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
            ui.label(theme::label(&format!(
                "{} {}",
                self.layer.tag(),
                self.layer.name()
            )));
            if let Some(message) = &self.status_message {
                ui.label(theme::mono(
                    "\u{00B7}",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
                ui.label(theme::label(message));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(metrics::PANEL_INSET_X);
                let color = if self.terminal_open {
                    theme::ELECTRIC
                } else {
                    theme::TEXT_DIM
                };
                let switch = ui.add(
                    egui::Button::new(theme::mono(">_ terminal", metrics::STATUS_TEXT_SIZE, color))
                        .frame(false),
                );
                if switch.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if switch.clicked() {
                    self.terminal_open = !self.terminal_open;
                }

                if let Some((line, col)) = self.cursor_pos {
                    ui.label(theme::mono(
                        format!("{line}:{col}"),
                        metrics::STATUS_TEXT_SIZE,
                        theme::TEXT_DIM,
                    ));
                }

                self.diagnostics_chip(ui, errors, warnings);

                // Unsaved-count chip; clicking it saves everything (Cmd+Shift+S).
                let dirty = self.workspace.dirty_documents().count();
                if dirty > 0 {
                    let chip = ui.add(
                        egui::Button::new(theme::mono(
                            format!("\u{25CF} {dirty} unsaved"),
                            metrics::STATUS_TEXT_SIZE,
                            theme::WARNING,
                        ))
                        .frame(false),
                    );
                    if chip.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if chip
                        .on_hover_text(theme::mono(
                            "save all",
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT,
                        ))
                        .clicked()
                    {
                        self.save_all_documents();
                    }
                }
            });
        });
    }

    /// Diagnostic tallies for the active build target; click opens Output.
    fn diagnostics_chip(&mut self, ui: &mut egui::Ui, errors: usize, warnings: usize) {
        if errors + warnings == 0 {
            return;
        }
        let text = match (errors, warnings) {
            (0, w) => format!(
                "\u{25B2} {}",
                super::diagnostics::tally(w as u64, "warning")
            ),
            (e, 0) => format!("\u{25B2} {}", super::diagnostics::tally(e as u64, "error")),
            (e, w) => format!(
                "\u{25B2} {} \u{00B7} {}",
                super::diagnostics::tally(e as u64, "error"),
                super::diagnostics::tally(w as u64, "warning")
            ),
        };
        let color = if errors > 0 {
            theme::DANGER
        } else {
            theme::WARNING
        };
        let chip = ui.add(
            egui::Button::new(theme::mono(text, metrics::STATUS_TEXT_SIZE, color)).frame(false),
        );
        if chip.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if chip
            .on_hover_text(theme::mono(
                "show build output",
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ))
            .clicked()
        {
            self.terminal_open = true;
            self.terminal.set_active_output();
        }
    }
}
