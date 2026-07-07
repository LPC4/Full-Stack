use super::{FullStackApp, metrics};
use crate::ui::theme;
use eframe::egui;

const TOP_BAR_CONTROL_HEIGHT: f32 = 24.0;

impl FullStackApp {
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.horizontal_centered(|ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if action_button(ui, "open folder")
                    .on_hover_text("Open a local workspace folder")
                    .clicked()
                    && !self.block_open_folder_for_dirty_documents()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.open_native_workspace_folder(&folder);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().interact_size.y = TOP_BAR_CONTROL_HEIGHT;
                ui.add_space(metrics::SECTION_GAP);
                if action_button(ui, "\u{25B6} run")
                    .on_hover_text("Run (Ctrl+R / F5)")
                    .clicked()
                {
                    self.run_selected_config(now);
                }
                if action_button(ui, "\u{25CE} debug")
                    .on_hover_text("Run paused at entry (Shift+F5)")
                    .clicked()
                {
                    self.debug_selected_config(now);
                }
                if action_button(ui, "\u{2699} build")
                    .on_hover_text("Build (Ctrl+B)")
                    .clicked()
                {
                    self.build_selected_config(now);
                }
                self.config_selector(ui);
            });
        });
    }

    /// The run-config dropdown: choose which program Build/Run act on.
    fn config_selector(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = self.configs().iter().map(|c| c.name.clone()).collect();
        if names.is_empty() {
            ui.label(theme::label("no run configs"));
            return;
        }
        let selected = self
            .selected_config()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "select".to_owned());
        let mut chosen = None;
        egui::ComboBox::from_id_salt("run_config")
            .selected_text(theme::mono(
                selected,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ))
            .show_ui(ui, |ui| {
                for (idx, name) in names.iter().enumerate() {
                    let active = idx == self.selected_config;
                    if ui
                        .selectable_label(
                            active,
                            theme::mono(name, metrics::STATUS_TEXT_SIZE, theme::TEXT),
                        )
                        .clicked()
                    {
                        chosen = Some(idx);
                    }
                }
            });
        if let Some(idx) = chosen {
            self.select_config(idx);
        }
    }
}

fn action_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(theme::mono(text, metrics::STATUS_TEXT_SIZE, theme::TEXT))
            .min_size(egui::vec2(0.0, TOP_BAR_CONTROL_HEIGHT)),
    )
}
