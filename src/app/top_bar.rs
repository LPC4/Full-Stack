use super::{FullStackApp, WorkspacePreset, metrics};
use crate::ui::theme;
use crate::ui::theme::icons;
use eframe::egui;
use full_stack::session::MachineState;

const TOP_BAR_CONTROL_HEIGHT: f32 = 24.0;

impl FullStackApp {
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.horizontal_centered(|ui| {
            self.open_folder_menu(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().interact_size.y = TOP_BAR_CONTROL_HEIGHT;
                ui.add_space(metrics::SECTION_GAP);
                let alive = self.live.as_ref().is_some_and(|live| live.state.is_alive());
                if alive {
                    // The focused debug surface carries its own transport bar;
                    // showing the verbs here too would double them.
                    if !self.debugger_focused() {
                        self.session_transport(ui, now);
                    }
                } else {
                    self.launch_controls(ui, now);
                }
                self.config_selector(ui);
            });
        });
    }

    /// The workspace picker: the two bundled folders on every target, plus a
    /// native file dialog for arbitrary local folders.
    fn open_folder_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(
            theme::mono("open folder", metrics::STATUS_TEXT_SIZE, theme::TEXT),
            |ui| {
                if menu_item(ui, "demo workspace")
                    .on_hover_text("Kernel, stdlib, user tools, and examples")
                    .clicked()
                {
                    self.open_workspace_preset(WorkspacePreset::Demo);
                }
                if menu_item(ui, "lessons")
                    .on_hover_text("The lessons from the written guide, one folder each")
                    .clicked()
                {
                    self.open_workspace_preset(WorkspacePreset::Lessons);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if menu_item(ui, "browse...").clicked()
                    && !self.block_open_folder_for_dirty_documents()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.open_native_workspace_folder(&folder);
                }
            },
        )
        .response
        .on_hover_text("Open a workspace folder");
    }

    /// Idle chrome: launch verbs for the selected config.
    fn launch_controls(&mut self, ui: &mut egui::Ui, now: f64) {
        if action_button(ui, &format!("{} run", icons::RUN), theme::TEXT)
            .on_hover_text("Run (Ctrl+R / F5)")
            .clicked()
        {
            self.run_selected_config(now);
        }
        if action_button(ui, &format!("{} debug", icons::DEBUG), theme::TEXT)
            .on_hover_text("Run paused at entry (Shift+F5)")
            .clicked()
        {
            self.debug_selected_config(now);
        }
        if action_button(ui, &format!("{} build", icons::BUILD), theme::TEXT)
            .on_hover_text("Build (Ctrl+B)")
            .clicked()
        {
            self.build_selected_config(now);
        }
    }

    /// A session is alive: the launch verbs give way to the transport, so the
    /// machine is always stoppable no matter which panel has focus.
    fn session_transport(&mut self, ui: &mut egui::Ui, now: f64) {
        if action_button(ui, &format!("{} stop", icons::STOP), theme::DANGER)
            .on_hover_text("Stop the session (Shift+F5)")
            .clicked()
        {
            self.stop_live_machine();
        }
        if action_button(ui, &format!("{} restart", icons::RESTART), theme::TEXT)
            .on_hover_text("Restart (Ctrl+F5); reuses the build when sources are unchanged")
            .clicked()
        {
            self.restart_selected_config(now);
        }
        let paused = self
            .live
            .as_ref()
            .is_some_and(|live| live.state == MachineState::Paused);
        let (icon, verb, hint) = if paused {
            (icons::RUN, "resume", "Resume (F5)")
        } else {
            (icons::PAUSE, "pause", "Pause (F6)")
        };
        if action_button(ui, &format!("{icon} {verb}"), theme::ELECTRIC)
            .on_hover_text(hint)
            .clicked()
            && let Some(live) = &mut self.live
        {
            if paused {
                live.resume();
            } else {
                live.pause();
            }
        }
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

fn menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.button(theme::mono(label, metrics::STATUS_TEXT_SIZE, theme::TEXT))
}

fn action_button(ui: &mut egui::Ui, text: &str, color: egui::Color32) -> egui::Response {
    ui.add(
        egui::Button::new(theme::mono(text, metrics::STATUS_TEXT_SIZE, color))
            .min_size(egui::vec2(0.0, TOP_BAR_CONTROL_HEIGHT)),
    )
}
