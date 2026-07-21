//! The universal Overview pane: what is selected, what ran, current problems,
//! and what the inspector can show right now.

use super::widgets::{section_label, stat_row};
use super::{Capabilities, FullStackApp};
use crate::app::{metrics, tool_window};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::session::MachineState;

impl FullStackApp {
    pub(super) fn overview_pane(&mut self, ui: &mut egui::Ui, caps: Capabilities) {
        let mut open_problems = false;
        let mut stage_click = None;
        egui::ScrollArea::vertical()
            .id_salt("overview_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.overview_config_section(ui);
                stage_click = self.overview_build_section(ui);
                self.overview_machine_section(ui);
                open_problems = self.overview_problems_section(ui);
                overview_capability_section(ui, caps);
            });
        if open_problems {
            self.terminal_open = true;
            self.terminal.set_active_problems();
        }
        // A stage chip opens the built program's root at that pipeline stage.
        if let Some(layer) = stage_click
            && let Some(root) = self
                .selected_config()
                .cloned()
                .and_then(|config| self.config_root(&config))
        {
            self.open_path(root);
            self.layer = layer;
        }
    }

    fn overview_config_section(&self, ui: &mut egui::Ui) {
        section_label(ui, "run config");
        let Some(config) = self.selected_config() else {
            stat_row(ui, "config", "none selected");
            return;
        };
        stat_row(ui, "name", &config.name);
        stat_row(ui, "kind", tool_window::kind_tag(config.kind));
        stat_row(ui, "present", tool_window::present_tag(config.run.present));
        if let Some(root) = self.config_root(config) {
            stat_row(ui, "root", root.as_str());
        }
    }

    fn overview_build_section(&self, ui: &mut egui::Ui) -> Option<crate::ui::Layer> {
        ui.add_space(metrics::SECTION_GAP);
        section_label(ui, "last build");
        let Some(build) = &self.last_build else {
            stat_row(ui, "build", "not built yet");
            return None;
        };
        let clicked = stage_strip(ui);
        stat_row(ui, "target", super::target_tag(build.target));
        stat_row(ui, "entry", &build.entry);
        stat_row(
            ui,
            "units",
            &crate::app::diagnostics::tally(build.artifacts.units.len() as u64, "unit"),
        );
        clicked
    }

    fn overview_machine_section(&self, ui: &mut egui::Ui) {
        ui.add_space(metrics::SECTION_GAP);
        section_label(ui, "machine");
        match (&self.live, &self.vm_result) {
            (Some(live), _) => {
                stat_row(ui, "state", &machine_state_tag(&live.state));
                stat_row(
                    ui,
                    "steps",
                    &crate::app::diagnostics::tally(live.steps, "step"),
                );
                if let MachineState::Exited(code) = live.state {
                    stat_row(ui, "exit", &code.to_string());
                }
            }
            (None, Some(vm)) => {
                stat_row(ui, "state", "post-mortem");
                stat_row(
                    ui,
                    "steps",
                    &crate::app::diagnostics::tally(vm.steps, "step"),
                );
                if let Some(code) = vm.exit_code {
                    stat_row(ui, "exit", &code.to_string());
                }
            }
            (None, None) => {
                stat_row(ui, "state", "no session");
            }
        }
    }

    fn overview_problems_section(&self, ui: &mut egui::Ui) -> bool {
        ui.add_space(metrics::SECTION_GAP);
        section_label(ui, "problems");
        let (errors, warnings) = self
            .stage_cache
            .as_ref()
            .map(|cache| crate::app::diagnostics::count_levels(&cache.result.diagnostics))
            .unwrap_or((0, 0));
        let (text, color) = match (errors, warnings) {
            (0, 0) => ("none".to_owned(), theme::TEXT_FAINT),
            (e, 0) => (
                crate::app::diagnostics::tally(e as u64, "error"),
                theme::DANGER,
            ),
            (0, w) => (
                crate::app::diagnostics::tally(w as u64, "warning"),
                theme::WARNING,
            ),
            (e, w) => (
                format!(
                    "{} \u{00B7} {}",
                    crate::app::diagnostics::tally(e as u64, "error"),
                    crate::app::diagnostics::tally(w as u64, "warning")
                ),
                theme::DANGER,
            ),
        };
        clickable_stat_row(ui, "diagnostics", &text, color, "open Problems")
    }
}

// An empty-looking panel still explains what a run or build would unlock.
fn overview_capability_section(ui: &mut egui::Ui, caps: Capabilities) {
    ui.add_space(metrics::SECTION_GAP);
    section_label(ui, "inspectable");
    stat_row(
        ui,
        "build",
        if caps.build {
            "artifacts, symbols"
        } else {
            "build to inspect"
        },
    );
    stat_row(
        ui,
        "machine",
        if caps.live {
            "live session"
        } else if caps.registers {
            "post-mortem CPU"
        } else {
            "run to inspect"
        },
    );
    stat_row(
        ui,
        "kernel",
        if caps.kernel {
            "probes available"
        } else {
            "kernel runs only"
        },
    );
}

fn machine_state_tag(state: &MachineState) -> String {
    match state {
        MachineState::Running => "running".to_owned(),
        MachineState::Paused => "paused".to_owned(),
        MachineState::Exited(_) => "exited".to_owned(),
        MachineState::Stopped => "stopped".to_owned(),
        MachineState::Faulted(_) => "faulted".to_owned(),
    }
}

fn stage_strip(ui: &mut egui::Ui) -> Option<crate::ui::Layer> {
    use crate::ui::Layer;
    const STAGES: [Layer; 4] = [Layer::Hll, Layer::Ir, Layer::Asm, Layer::Elf];
    let width = ui.available_width() - 2.0 * metrics::PANEL_INSET_X;
    let chip_w = width / STAGES.len() as f32;
    let (row, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), metrics::ROW_HEIGHT + 4.0),
        Sense::hover(),
    );
    let mut clicked = None;
    for (i, layer) in STAGES.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            pos2(
                row.left() + metrics::PANEL_INSET_X + i as f32 * chip_w,
                row.top() + 2.0,
            ),
            vec2(chip_w, metrics::ROW_HEIGHT),
        );
        let resp = ui.interact(rect, ui.id().with(("stage_chip", i)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("open {} stage", layer.name()),
            )
        });
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            0.0,
            if resp.hovered() {
                theme::SURFACE_ALT
            } else {
                theme::SURFACE
            },
        );
        painter.rect_stroke(
            rect,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0, theme::HAIRLINE),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            layer.name(),
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            layer.color(),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            clicked = Some(*layer);
        }
    }
    clicked
}

fn clickable_stat_row(
    ui: &mut egui::Ui,
    key: &str,
    value: &str,
    color: egui::Color32,
    hover: &str,
) -> bool {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        key,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::TEXT_DIM,
    );
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    resp.on_hover_text(hover.to_owned()).clicked()
}
