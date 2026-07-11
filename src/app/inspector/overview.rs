//! The universal panes: Overview (what is selected, what ran, what is
//! inspectable) and Diagnostics (latest build/check problems plus runtime faults).

use super::widgets::{section_label, stat_row};
use super::{Capabilities, FullStackApp};
use crate::app::{chrome, metrics, tool_window};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::compilation_pipeline::DiagnosticLevel;
use full_stack::session::MachineState;

impl FullStackApp {
    /// The landing pane: selected config, last build, machine state, and what
    /// the inspector can show right now. Useful before any run exists.
    pub(super) fn overview_pane(&self, ui: &mut egui::Ui, caps: Capabilities) {
        egui::ScrollArea::vertical()
            .id_salt("overview_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.overview_config_section(ui);
                self.overview_build_section(ui);
                self.overview_machine_section(ui);
                overview_capability_section(ui, caps);
            });
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

    fn overview_build_section(&self, ui: &mut egui::Ui) {
        ui.add_space(metrics::SECTION_GAP);
        section_label(ui, "last build");
        let Some(build) = &self.last_build else {
            stat_row(ui, "build", "not built yet");
            return;
        };
        stat_row(ui, "target", super::target_tag(build.target));
        stat_row(ui, "entry", &build.entry);
        stat_row(
            ui,
            "units",
            &crate::app::diagnostics::tally(build.artifacts.units.len() as u64, "unit"),
        );
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

    /// The latest compile/check diagnostics with click-to-jump, the assembler
    /// or linker error when one blocked the build, and any runtime fault.
    pub(super) fn diagnostics_pane(&mut self, ui: &mut egui::Ui) {
        let root = self
            .active_build_target
            .as_ref()
            .map(|target| target.root_path.as_str().to_owned());
        let mut jump = None;

        egui::ScrollArea::vertical()
            .id_salt("diagnostics_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut any = false;
                if let Some(cache) = &self.stage_cache {
                    for diag in &cache.result.diagnostics {
                        any = true;
                        let line = diag.span.as_ref().map(|span| span.line as usize);
                        if diagnostic_row(ui, diag.level, &diag.message, line).clicked()
                            && let (Some(root), Some(line)) = (&root, line)
                        {
                            jump = Some((root.clone(), line));
                        }
                    }
                    if let Some(err) = &cache.result.assembler_error {
                        any = true;
                        diagnostic_row(ui, DiagnosticLevel::Error, err, None);
                    }
                }
                if let Some(fault) = self.runtime_fault_text() {
                    any = true;
                    section_label(ui, "runtime");
                    diagnostic_row(ui, DiagnosticLevel::Error, &fault, None);
                }
                if !any {
                    let text = if self.stage_cache.is_some() {
                        "No problems found."
                    } else {
                        "No diagnostics captured.\nChecks run on save, idle, and build."
                    };
                    chrome::indented(ui, |ui| {
                        ui.label(theme::mono(
                            text,
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT_FAINT,
                        ));
                    });
                }
            });

        if let Some((path, line)) = jump {
            self.jump_to_source(&path, line);
        }
    }

    /// The live session's fault message with its source location, if any.
    fn runtime_fault_text(&self) -> Option<String> {
        let live = self.live.as_ref()?;
        let MachineState::Faulted(msg) = &live.state else {
            return None;
        };
        let at = live
            .source_for_pc(live.registers().pc)
            .map(|(file, line)| format!(" ({file}:{line})"))
            .unwrap_or_default();
        Some(format!("{msg}{at}"))
    }
}

/// What each pane group can show right now, so an empty-looking panel still
/// explains what a run or build would unlock.
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

/// One diagnostic row: `L<line>` eyebrow and the message, colored by level.
/// The full message rides the hover; the row is clickable when a line exists.
fn diagnostic_row(
    ui: &mut egui::Ui,
    level: DiagnosticLevel,
    message: &str,
    line: Option<usize>,
) -> egui::Response {
    let width = ui.available_width();
    let sense = if line.is_some() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), sense);
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        if line.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    let color = match level {
        DiagnosticLevel::Error => theme::DANGER,
        DiagnosticLevel::Warning => theme::WARNING,
    };
    let mut left = rect.left() + metrics::PANEL_INSET_X;
    if let Some(line) = line {
        let tag = painter.text(
            pos2(left, rect.center().y),
            Align2::LEFT_CENTER,
            format!("L{line}"),
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            color,
        );
        left = tag.right() + 8.0;
    }
    painter.text(
        pos2(left, rect.center().y),
        Align2::LEFT_CENTER,
        message.lines().next().unwrap_or_default(),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT,
    );
    resp.on_hover_text(message.to_owned())
}
