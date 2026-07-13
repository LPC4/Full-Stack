//! User-level IDE settings: appearance, editor, terminal, machine, and startup
//! behavior. App-global (follows the user via eframe storage), not per-workspace.

use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use serde::{Deserialize, Serialize};

/// Key under which settings persist in eframe storage.
pub(crate) const APP_KEY: &str = "full_stack_settings";

/// UI scale presets mapped to egui zoom factors.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ZoomLevel {
    Small,
    #[default]
    Medium,
    Large,
    ExtraLarge,
}

impl ZoomLevel {
    pub(crate) const ALL: [Self; 4] = [Self::Small, Self::Medium, Self::Large, Self::ExtraLarge];

    /// The egui `zoom_factor` this preset applies. Medium is 1.0 (no change).
    pub(crate) fn factor(self) -> f32 {
        match self {
            Self::Small => 0.9,
            Self::Medium => 1.0,
            Self::Large => 1.15,
            Self::ExtraLarge => 1.3,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Small => "S",
            Self::Medium => "M",
            Self::Large => "L",
            Self::ExtraLarge => "XL",
        }
    }
}

/// egui's default `TextStyle::Monospace` size; the editor-font default matches it
/// so a fresh profile renders identically to no settings at all.
pub(crate) const DEFAULT_MONO_SIZE: f32 = 12.0;

/// The settings reading column: capped so rows stay scannable in a wide editor.
const SETTINGS_MAX_WIDTH: f32 = 620.0;
const SETTINGS_INSET_X: f32 = 28.0;

/// Every field is optional via `#[serde(default)]`, so older and newer profiles
/// load forward-compatibly. Defaults mirror the previously hardcoded behavior.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub(crate) struct AppSettings {
    // Appearance
    pub(crate) ui_zoom: ZoomLevel,
    pub(crate) editor_font_size: f32,
    pub(crate) current_line_highlight: bool,
    // Editor
    pub(crate) check_on_save: bool,
    pub(crate) check_on_idle: bool,
    pub(crate) save_all_before_run: bool,
    pub(crate) confirm_discard_on_close: bool,
    // Terminal
    pub(crate) open_terminal_on_run: bool,
    pub(crate) focus_machine_console_on_run: bool,
    // Machine / debugger
    pub(crate) default_steps_per_frame: u64,
    pub(crate) frame_budget_ms: u64,
    pub(crate) restart_reuses_build: bool,
    pub(crate) persist_breakpoints: bool,
    // Startup (native only)
    pub(crate) reopen_last_workspace: bool,
    pub(crate) last_workspace: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ui_zoom: ZoomLevel::Medium,
            editor_font_size: DEFAULT_MONO_SIZE,
            current_line_highlight: true,
            check_on_save: true,
            check_on_idle: true,
            save_all_before_run: true,
            confirm_discard_on_close: false,
            open_terminal_on_run: true,
            focus_machine_console_on_run: true,
            default_steps_per_frame: 200_000,
            frame_budget_ms: 6,
            restart_reuses_build: true,
            persist_breakpoints: true,
            reopen_last_workspace: false,
            last_workspace: None,
        }
    }
}

impl AppSettings {
    /// Apply appearance settings (zoom, mono font size) on top of `theme::apply`.
    /// Idempotent; call only when settings change to avoid re-layout churn.
    pub(crate) fn apply(&self, ctx: &egui::Context) {
        ctx.set_zoom_factor(self.ui_zoom.factor());
        let mut style = (*ctx.global_style()).clone();
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Monospace) {
            font.size = self.editor_font_size.clamp(8.0, 24.0);
        }
        ctx.set_global_style(style);
    }
}

impl FullStackApp {
    /// The full-page settings view, shown over the editor when `settings_open`.
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Settings");
        let mut s = self.settings.clone();
        let defaults = AppSettings::default();
        egui::ScrollArea::vertical()
            .id_salt("settings_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // A capped, centered reading column; full-width rows are unscannable.
                let column = SETTINGS_MAX_WIDTH.min(ui.available_width() - 2.0 * SETTINGS_INSET_X);
                let inset = SETTINGS_INSET_X.max((ui.available_width() - column) / 2.0);
                ui.horizontal(|ui| {
                    ui.add_space(inset);
                    ui.vertical(|ui| {
                        ui.set_max_width(column.max(0.0));
                        if section_title(ui, "Appearance") {
                            s.ui_zoom = defaults.ui_zoom;
                            s.editor_font_size = defaults.editor_font_size;
                            s.current_line_highlight = defaults.current_line_highlight;
                        }
                        selector_row(ui, "UI scale", "Overall interface zoom.", &mut s.ui_zoom);
                        number_row(
                            ui,
                            "Editor font size",
                            "Monospace size across code, terminal, and machine views.",
                            &mut s.editor_font_size,
                        );
                        toggle_row(
                            ui,
                            "Current-line highlight",
                            "Shade the line the cursor sits on.",
                            &mut s.current_line_highlight,
                        );

                        if section_title(ui, "Editor") {
                            s.check_on_save = defaults.check_on_save;
                            s.check_on_idle = defaults.check_on_idle;
                            s.save_all_before_run = defaults.save_all_before_run;
                        }
                        toggle_row(
                            ui,
                            "Check on save",
                            "Recompile and show diagnostics each time you save.",
                            &mut s.check_on_save,
                        );
                        toggle_row(
                            ui,
                            "Check while typing",
                            "Recompile after a pause in typing, before you save.",
                            &mut s.check_on_idle,
                        );
                        toggle_row(
                            ui,
                            "Save all before run",
                            "Write unsaved edits before a build or run.",
                            &mut s.save_all_before_run,
                        );
                        toggle_row(
                            ui,
                            "Confirm dirty tab close",
                            "Ask before discarding unsaved changes from a tab.",
                            &mut s.confirm_discard_on_close,
                        );

                        if section_title(ui, "Terminal") {
                            s.open_terminal_on_run = defaults.open_terminal_on_run;
                            s.focus_machine_console_on_run = defaults.focus_machine_console_on_run;
                        }
                        toggle_row(
                            ui,
                            "Open terminal on run",
                            "Show the bottom terminal when a run starts.",
                            &mut s.open_terminal_on_run,
                        );
                        toggle_row(
                            ui,
                            "Focus machine console",
                            "Route keyboard input to the guest console after launch.",
                            &mut s.focus_machine_console_on_run,
                        );

                        if section_title(ui, "Machine") {
                            s.default_steps_per_frame = defaults.default_steps_per_frame;
                            s.frame_budget_ms = defaults.frame_budget_ms;
                            s.restart_reuses_build = defaults.restart_reuses_build;
                            s.persist_breakpoints = defaults.persist_breakpoints;
                        }
                        u64_row(
                            ui,
                            "Default steps per frame",
                            "Fallback step budget when the run config does not override it.",
                            &mut s.default_steps_per_frame,
                            1..=5_000_000,
                            10_000.0,
                            "",
                        );
                        u64_row(
                            ui,
                            "Frame budget",
                            "Maximum VM drive time per UI frame.",
                            &mut s.frame_budget_ms,
                            1..=32,
                            1.0,
                            " ms",
                        );
                        toggle_row(
                            ui,
                            "Reuse build on restart",
                            "Restart from cached artifacts while sources are unchanged.",
                            &mut s.restart_reuses_build,
                        );
                        toggle_row(
                            ui,
                            "Persist breakpoints",
                            "Write debugger breakpoints back to the selected run config.",
                            &mut s.persist_breakpoints,
                        );

                        // Reopening a folder is native-only; wasm keeps the bundle.
                        if !cfg!(target_arch = "wasm32") {
                            if section_title(ui, "Startup") {
                                s.reopen_last_workspace = defaults.reopen_last_workspace;
                            }
                            let hint = match &s.last_workspace {
                                Some(path) => format!("Last opened: {path}"),
                                None => "No folder opened yet.".to_owned(),
                            };
                            toggle_row(
                                ui,
                                "Reopen last workspace",
                                &hint,
                                &mut s.reopen_last_workspace,
                            );
                        }

                        section_reference(ui, "Shortcuts");
                        for (keys, desc) in SHORTCUTS {
                            shortcut_row(ui, keys, desc);
                        }
                        ui.add_space(metrics::SECTION_GAP);
                    });
                });
            });

        if s != self.settings {
            let appearance_changed = s.ui_zoom != self.settings.ui_zoom
                || s.editor_font_size != self.settings.editor_font_size;
            self.settings = s;
            if appearance_changed {
                self.settings.apply(ui.ctx());
            }
        }
    }
}

/// Keyboard shortcuts, the single source for the read-only reference table.
/// `command` maps to Ctrl on Windows/Linux and Cmd on macOS.
const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+S", "Save the active file"),
    ("Ctrl+Shift+S", "Save all files"),
    ("Ctrl+B", "Build the selected config"),
    ("Ctrl+R", "Run the selected config"),
    ("F5", "Resume, or run when idle"),
    ("Shift+F5", "Stop the session; debug launch when idle"),
    ("Ctrl+F5", "Restart the session"),
    ("F6", "Pause"),
    ("F9", "Toggle a breakpoint on the caret line"),
    ("F10", "Step (line/instruction/cycle per the dock unit)"),
    ("F11", "Step into"),
    ("Shift+F11", "Step out"),
    ("Ctrl+P", "Quick open a file"),
    ("Ctrl+F", "Find in the active file"),
    ("Ctrl+Shift+F", "Find across the workspace"),
    ("Ctrl+J", "Toggle the terminal"),
    ("Ctrl+W", "Close the active tab"),
    ("Ctrl+,", "Open settings"),
];

/// Vertical air above each section eyebrow so groups read as groups.
const SECTION_TOP_GAP: f32 = 22.0;

/// A plain section eyebrow with no reset control, for read-only reference lists.
fn section_reference(ui: &mut egui::Ui, title: &str) {
    ui.add_space(SECTION_TOP_GAP);
    ui.label(theme::label(title));
    ui.add_space(metrics::SECTION_GAP - 2.0);
    chrome::hairline_soft(ui);
}

/// A read-only shortcut row: the key combo on the left, its action on the right.
fn shortcut_row(ui: &mut egui::Ui, keys: &str, desc: &str) {
    ui.add_space(metrics::SECTION_GAP);
    ui.horizontal(|ui| {
        ui.label(theme::mono(keys, metrics::LABEL_TEXT_SIZE, theme::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(theme::mono(
                desc,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
        });
    });
}

/// A section eyebrow with a right-aligned "reset" control; true when reset is clicked.
fn section_title(ui: &mut egui::Ui, title: &str) -> bool {
    ui.add_space(SECTION_TOP_GAP);
    let mut reset = false;
    ui.horizontal(|ui| {
        ui.label(theme::label(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(theme::mono(
                        "reset",
                        metrics::STATUS_TEXT_SIZE,
                        theme::TEXT_FAINT,
                    ))
                    .frame(false),
                )
                .on_hover_text("restore this section to defaults")
                .clicked()
            {
                reset = true;
            }
        });
    });
    ui.add_space(metrics::SECTION_GAP - 2.0);
    chrome::hairline_soft(ui);
    reset
}

/// A settings row: label over a dim hint on the left, a control on the right.
fn setting_row(ui: &mut egui::Ui, label: &str, hint: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(metrics::DOCK_SECTION_GAP);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(theme::mono(label, metrics::LABEL_TEXT_SIZE, theme::TEXT));
            ui.label(theme::mono(
                hint,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

fn toggle_row(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut bool) {
    setting_row(ui, label, hint, |ui| {
        if chrome::segmented(ui, &["off", "on"], usize::from(*value)).is_some() {
            *value = !*value;
        }
    });
}

fn number_row(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut f32) {
    setting_row(ui, label, hint, |ui| {
        ui.add(
            egui::DragValue::new(value)
                .range(8.0..=24.0)
                .speed(0.25)
                .suffix(" px"),
        );
    });
}

fn u64_row(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    speed: f64,
    suffix: &str,
) {
    setting_row(ui, label, hint, |ui| {
        ui.add(
            egui::DragValue::new(value)
                .range(range)
                .speed(speed)
                .suffix(suffix),
        );
    });
}

fn selector_row(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut ZoomLevel) {
    setting_row(ui, label, hint, |ui| {
        let labels: Vec<&str> = ZoomLevel::ALL.iter().map(|z| z.label()).collect();
        let selected = ZoomLevel::ALL.iter().position(|z| z == value).unwrap_or(1);
        if let Some(next) = chrome::segmented(ui, &labels, selected) {
            *value = ZoomLevel::ALL[next];
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_todays_behavior() {
        let s = AppSettings::default();
        assert_eq!(s.ui_zoom, ZoomLevel::Medium);
        assert_eq!(s.ui_zoom.factor(), 1.0);
        assert_eq!(s.editor_font_size, DEFAULT_MONO_SIZE);
        assert!(s.current_line_highlight);
        assert!(s.check_on_save);
        assert!(s.check_on_idle);
        assert!(s.save_all_before_run);
        assert!(!s.confirm_discard_on_close);
        assert!(s.open_terminal_on_run);
        assert_eq!(s.default_steps_per_frame, 200_000);
        assert_eq!(s.frame_budget_ms, 6);
        assert!(s.restart_reuses_build);
        assert!(s.persist_breakpoints);
        assert!(!s.reopen_last_workspace);
    }

    #[test]
    fn serde_round_trips() {
        let s = AppSettings {
            ui_zoom: ZoomLevel::Large,
            check_on_save: false,
            frame_budget_ms: 12,
            ..AppSettings::default()
        };
        let text = serde_json::to_string(&s).expect("serialize");
        let back: AppSettings = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn missing_fields_default_and_unknown_ignored() {
        // Only one field set; everything else must fall back to Default, and an
        // unknown key must be ignored rather than failing the load.
        let json = r#"{ "check_on_save": false, "future_setting": 42 }"#;
        let s: AppSettings = serde_json::from_str(json).expect("deserialize partial");
        assert!(!s.check_on_save);
        assert_eq!(s.ui_zoom, ZoomLevel::Medium);
        assert_eq!(s.frame_budget_ms, 6);
    }
}
