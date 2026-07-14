use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use chrome::ActionTone;
use eframe::egui;
use full_stack::session::MachineState;

/// A debugger-dock action requested this frame. Lifecycle and single-step verbs
/// live on the debug surface's transport bar; the dock owns everything else.
#[derive(Clone)]
enum Transport {
    /// Bulk step by the selected unit.
    StepN(u64),
    /// Jump the debugger to the disassembly view (fault triage).
    ShowDisasm,
    /// Focus the debugger surface on a view.
    ShowView(super::debug_view::DebugView),
    /// A `symbol` or `symbol+0x10` spec typed into the dock input.
    AddBreakpoint(String),
    RemoveBreakpoint(u64),
    ClearBreakpoints,
    /// Turn the time-travel history on or off (clears it when turned off).
    ToggleTimeTravel,
    /// Restore the machine to history frame `index`.
    ScrubTo(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StepUnit {
    Line,
    Instruction,
    Cycle,
}

impl FullStackApp {
    /// The right dock: debugger controls only. Inspection data (call stack,
    /// registers, machine probes) lives in the left Inspect tool.
    pub(super) fn machine_dock(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Debugger");
        let mut action = None;
        let mut step_unit = self.step_unit;
        let mut step_count = self.step_count;
        let mut bp_entry = std::mem::take(&mut self.breakpoint_entry);
        let mut time_scrub = self.time_scrub;
        let mut time_follow = self.time_follow_now;
        let debug_view = self.debug_view;
        let debugger_focused = self.debugger_focused();
        if let Some(live) = &self.live {
            chrome::indented(ui, |ui| {
                if let Some(act) = state_section(ui, live) {
                    action = Some(act);
                }
                if !debugger_focused {
                    ui.add_space(metrics::SECTION_GAP);
                    if chrome::action_row(ui, &[("focus [debug] tab", ActionTone::Primary)])
                        == Some(0)
                    {
                        action = Some(Transport::ShowView(debug_view));
                    }
                }
                if live.state == MachineState::Paused
                    && let Some(step) = step_section(ui, &mut step_unit, &mut step_count)
                {
                    action = Some(step);
                }
                if let Some(bp_action) = breakpoints_section(ui, live, &mut bp_entry) {
                    action = Some(bp_action);
                }
                if let Some(tt_action) =
                    time_travel_section(ui, live, &mut time_scrub, &mut time_follow)
                {
                    action = Some(tt_action);
                }
            });
        }
        self.step_unit = step_unit;
        self.step_count = step_count;
        self.breakpoint_entry = bp_entry;
        self.time_scrub = time_scrub;
        self.time_follow_now = time_follow;
        if let Some(action) = action {
            self.apply_transport(action);
        }
    }

    /// Apply a dock control to the session after the render borrow ends.
    fn apply_transport(&mut self, action: Transport) {
        let mut persist_breakpoints = false;
        match action {
            Transport::ShowDisasm => {
                self.debug_view = super::debug_view::DebugView::Disasm;
                self.focus_debugger();
            }
            Transport::ShowView(view) => {
                self.debug_view = view;
                self.focus_debugger();
            }
            Transport::AddBreakpoint(spec) => {
                if let Some(live) = &mut self.live {
                    match live.resolve_breakpoint(&spec) {
                        Ok(addr) => {
                            live.breakpoints.insert(addr);
                            self.breakpoint_entry.clear();
                            persist_breakpoints = true;
                        }
                        Err(err) => {
                            self.status_message = Some(format!("breakpoint `{spec}`: {err}"));
                        }
                    }
                }
            }
            Transport::ToggleTimeTravel => {
                if let Some(live) = &mut self.live {
                    let on = !live.timeline.enabled();
                    live.timeline.set_enabled(on);
                    if !on {
                        self.time_scrub = 0;
                        self.time_follow_now = true;
                    }
                }
            }
            Transport::ScrubTo(index) => {
                if let Some(live) = &mut self.live {
                    live.restore_timepoint(index);
                    self.time_scrub = index;
                }
                // Refresh machine-derived views against the restored state.
                self.inspector.cache = None;
                self.inspector.cache_grid = None;
            }
            Transport::StepN(count) => {
                let step_unit = self.step_unit;
                if let Some(live) = &mut self.live {
                    match step_unit {
                        StepUnit::Line => {
                            for _ in 0..count {
                                live.step_line();
                            }
                        }
                        StepUnit::Instruction => live.step_instructions(count),
                        StepUnit::Cycle => live.step_cycles(count),
                    }
                }
            }
            Transport::RemoveBreakpoint(addr) => {
                if let Some(live) = &mut self.live {
                    live.breakpoints.remove(&addr);
                    persist_breakpoints = true;
                }
            }
            Transport::ClearBreakpoints => {
                if let Some(live) = &mut self.live {
                    live.breakpoints.clear();
                    persist_breakpoints = true;
                }
            }
        }
        if persist_breakpoints {
            self.persist_live_breakpoints();
        }
    }
}

/// The state dot + label, and (on fault) the fault details.
fn state_section(ui: &mut egui::Ui, live: &full_stack::session::LiveMachine) -> Option<Transport> {
    let mut action = None;
    let (label, color) = state_style(&live.state);
    state_line(ui, &label, color);
    if let MachineState::Faulted(msg) = &live.state {
        ui.add_space(metrics::SECTION_GAP);
        ui.label(theme::mono(
            msg.clone(),
            metrics::STATUS_TEXT_SIZE,
            theme::DANGER,
        ));
        if let Some((name, offset)) = live.symbol_near(live.registers().pc) {
            ui.label(theme::mono(
                format!("near {name}+{offset:#x}"),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
        }
        ui.add_space(metrics::SECTION_GAP);
        if chrome::action_row(ui, &[("view disasm", ActionTone::Normal)]) == Some(0) {
            action = Some(Transport::ShowDisasm);
        }
    }
    action
}

/// The paused-state stepping group: a unit selector and a bulk step-N row.
/// Single steps (over/in/out) live on the debug surface's transport bar.
fn step_section(
    ui: &mut egui::Ui,
    step_unit: &mut StepUnit,
    step_count: &mut u64,
) -> Option<Transport> {
    let mut action = None;
    chrome::section_eyebrow(ui, "step");
    const UNITS: [StepUnit; 3] = [StepUnit::Line, StepUnit::Instruction, StepUnit::Cycle];
    let selected = UNITS.iter().position(|u| u == step_unit).unwrap_or(0);
    if let Some(next) = chrome::segmented(ui, &["line", "instr", "cycle"], selected) {
        *step_unit = UNITS[next];
    }
    let unit = match step_unit {
        StepUnit::Line => "lines",
        StepUnit::Instruction => "instructions",
        StepUnit::Cycle => "cycles",
    };
    ui.add_space(metrics::SECTION_GAP);
    ui.horizontal(|ui| {
        if dock_text_button(ui, "step", theme::TEXT).clicked() {
            action = Some(Transport::StepN(*step_count));
        }
        ui.add(
            egui::DragValue::new(step_count)
                .range(1..=1_000_000)
                .speed(10),
        );
        ui.label(theme::mono(
            unit,
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_FAINT,
        ));
    });
    action
}

/// Address breakpoints on the session; each row clears itself via its trailing
/// glyph and the entry field adds `symbol` or `symbol+0x10` specs.
fn breakpoints_section(
    ui: &mut egui::Ui,
    live: &full_stack::session::LiveMachine,
    entry: &mut String,
) -> Option<Transport> {
    let mut action = None;
    let count = live.breakpoints.len();
    let title = if count > 0 {
        format!("breakpoints \u{00B7} {count}")
    } else {
        "breakpoints".to_owned()
    };
    section_with_control(ui, &title, |ui| {
        if count > 0 && dock_text_button(ui, "clear", theme::TEXT_DIM).clicked() {
            action = Some(Transport::ClearBreakpoints);
        }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Laid out right to left: the add verb hugs the edge, the field fills.
        let add = dock_text_button(ui, "add", theme::TEXT_DIM);
        let field = ui.add_sized(
            egui::vec2(ui.available_width(), metrics::SEGMENT_HEIGHT),
            egui::TextEdit::singleline(entry)
                .id_salt("dock_breakpoint_entry")
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .hint_text(theme::mono(
                    "symbol+0x0",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                )),
        );
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (submitted || add.clicked()) && !entry.trim().is_empty() {
            action = Some(Transport::AddBreakpoint(entry.trim().to_owned()));
        }
    });
    ui.add_space(metrics::SECTION_GAP);
    for &addr in &live.breakpoints {
        let label = match live.symbol_near(addr) {
            Some((name, 0)) => name.to_owned(),
            Some((name, offset)) => format!("{name}+{offset:#x}"),
            None => String::new(),
        };
        if breakpoint_row(ui, addr, &label) {
            action = Some(Transport::RemoveBreakpoint(addr));
        }
    }
    action
}

/// One breakpoint: address + symbol, with a trailing remove glyph. True on remove.
fn breakpoint_row(ui: &mut egui::Ui, addr: u64, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), metrics::ROW_HEIGHT),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let font = egui::FontId::monospace(metrics::STATUS_TEXT_SIZE);
    painter.text(
        egui::pos2(rect.left() + 2.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{addr:#012x}"),
        font.clone(),
        theme::TEXT_DIM,
    );
    painter.text(
        egui::pos2(rect.left() + 96.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font.clone(),
        theme::TEXT,
    );
    let glyph_color = if resp.hovered() {
        theme::DANGER
    } else {
        theme::TEXT_FAINT
    };
    painter.text(
        egui::pos2(rect.right() - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "\u{00D7}",
        font,
        glyph_color,
    );
    resp.on_hover_text("remove breakpoint").clicked()
}

/// Time-travel history: a toggle plus a scrubber over captured frames. Scrubbing
/// restores the machine to that point (paused); the newest frame is "now".
fn time_travel_section(
    ui: &mut egui::Ui,
    live: &full_stack::session::LiveMachine,
    scrub: &mut usize,
    follow: &mut bool,
) -> Option<Transport> {
    let mut action = None;
    let enabled = live.timeline.enabled();
    section_with_control(ui, "time travel", |ui| {
        if chrome::segmented(ui, &["off", "on"], usize::from(enabled)).is_some() {
            action = Some(Transport::ToggleTimeTravel);
        }
    });
    if !enabled {
        return action;
    }
    let count = live.timeline.len();
    if count == 0 {
        ui.label(theme::mono(
            "recording... run or step",
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_FAINT,
        ));
        return action;
    }
    let last = count - 1;
    // While following, the scrubber tracks the newest frame as history grows;
    // this is display-only (the machine is already at "now", no restore).
    if *follow {
        *scrub = last;
    }
    *scrub = (*scrub).min(last);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Right to left: the now verb hugs the edge, the scrubber fills.
        let now_button = dock_text_button(ui, "now", theme::TEXT_DIM);
        ui.spacing_mut().slider_width = ui.available_width();
        if ui
            .add(egui::Slider::new(scrub, 0..=last).show_value(false))
            .changed()
        {
            // Dragging back detaches; dragging to the end re-arms the follow.
            *follow = *scrub == last;
            action = Some(Transport::ScrubTo(*scrub));
        }
        if now_button.clicked() {
            *scrub = last;
            *follow = true;
            action = Some(Transport::ScrubTo(last));
        }
    });
    let retired = live.timeline.retired_at(*scrub).unwrap_or(0);
    stat_row(
        ui,
        format!("frame {}/{count}", *scrub + 1),
        format!("{retired} retired"),
        theme::TEXT_FAINT,
    );
    action
}

/// A section eyebrow whose divider row carries a right-aligned control.
fn section_with_control(ui: &mut egui::Ui, title: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(metrics::DOCK_SECTION_GAP);
    ui.horizontal(|ui| {
        ui.label(theme::label(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(metrics::SECTION_GAP - 2.0);
    chrome::hairline_soft(ui);
    ui.add_space(metrics::SECTION_GAP);
}

/// A small frameless mono verb (clear/now/step), dimmer than the action rows.
fn dock_text_button(ui: &mut egui::Ui, label: &str, color: egui::Color32) -> egui::Response {
    let resp = ui.add(egui::Button::new(theme::mono(
        label,
        metrics::STATUS_TEXT_SIZE,
        color,
    )));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The machine state as a colored dot + word, the dock's loudest line.
fn state_line(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(metrics::STATUS_DOT_SIZE, metrics::STATUS_DOT_SIZE),
            egui::Sense::hover(),
        );
        ui.painter()
            .circle_filled(rect.center(), metrics::STATUS_DOT_SIZE / 2.0, color);
        ui.label(theme::mono(
            label.to_owned(),
            metrics::LABEL_TEXT_SIZE,
            color,
        ));
    });
}

/// A label/value line: faint label left, mono value right-aligned.
fn stat_row(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    value: impl Into<String>,
    value_color: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.label(theme::mono(
            label.into(),
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_FAINT,
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(theme::mono(
                value.into(),
                metrics::STATUS_TEXT_SIZE,
                value_color,
            ));
        });
    });
}

/// Shared with the terminal header so the machine state reads the same everywhere.
pub(super) fn state_style(state: &MachineState) -> (String, egui::Color32) {
    match state {
        MachineState::Running => ("running".to_owned(), theme::ELECTRIC),
        MachineState::Paused => ("paused".to_owned(), theme::WARNING),
        MachineState::Exited(0) => ("exit 0".to_owned(), theme::ELECTRIC),
        MachineState::Exited(code) => (format!("exit {code}"), theme::TEXT),
        MachineState::Stopped => ("stopped".to_owned(), theme::TEXT_DIM),
        MachineState::Faulted(_) => ("faulted".to_owned(), theme::DANGER),
    }
}
