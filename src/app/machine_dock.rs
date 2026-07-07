use super::{FullStackApp, chrome, metrics};
use crate::ui::{Layer, theme};
use chrome::ActionTone;
use eframe::egui;
use full_stack::session::MachineState;

/// A transport action requested from the dock controls this frame.
#[derive(Clone)]
enum Transport {
    Pause,
    Resume,
    StepOver,
    StepIn,
    StepOut,
    StepLine,
    StepN(u64),
    Stop,
    Restart,
    /// Jump the machine rung to the disassembly view (fault triage).
    ShowDisasm,
    /// Open a machine surface (console/screen/disasm/memory) on the L4 rung.
    ShowView(super::debug_view::MachineView),
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
    pub(super) fn machine_dock(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Machine");
        if self.live.is_some() {
            self.live_dock(ui);
        } else {
            self.idle_dock(ui);
        }
    }

    /// The resident session: state, counters, key registers, and the transport.
    fn live_dock(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let mut action = None;
        let mut step_unit = self.step_unit;
        let mut step_count = self.step_count;
        let mut bp_entry = std::mem::take(&mut self.breakpoint_entry);
        let mut time_scrub = self.time_scrub;
        let restart_debug = self.current_launch_debug;
        let machine_view = self.machine_view;
        let on_vm_layer = self.layer == Layer::Vm;
        let has_screen = self.config_has_screen();
        let process_count = self
            .inspect_cache
            .as_ref()
            .map(super::inspect_panel::InspectCache::process_count)
            .unwrap_or(0);
        if let Some(live) = &self.live {
            chrome::indented(ui, |ui| {
                if let Some(act) = state_section(ui, live, process_count) {
                    action = Some(act);
                }
                chrome::section_eyebrow(ui, "registers");
                let regs = live.registers();
                stat_row(ui, "pc", format!("{:016x}", regs.pc), theme::TEXT_DIM);
                stat_row(ui, "sp", format!("{:016x}", regs.x[2]), theme::TEXT_DIM);
                stat_row(ui, "ra", format!("{:016x}", regs.x[1]), theme::TEXT_DIM);
                stat_row(ui, "a0", format!("{:016x}", regs.x[10]), theme::TEXT_DIM);
                chrome::section_eyebrow(ui, "session");
                if let Some(transport) = transport_section(
                    ui,
                    &live.state,
                    restart_debug,
                    &mut step_unit,
                    &mut step_count,
                ) {
                    action = Some(transport);
                }
                chrome::section_eyebrow(ui, "view");
                if let Some(view) = view_row(ui, machine_view, on_vm_layer, has_screen) {
                    action = Some(Transport::ShowView(view));
                }
                if let Some(bp_action) = breakpoints_section(ui, live, &mut bp_entry) {
                    action = Some(bp_action);
                }
                if let Some(tt_action) = time_travel_section(ui, live, &mut time_scrub) {
                    action = Some(tt_action);
                }
            });
        }
        self.step_unit = step_unit;
        self.step_count = step_count;
        self.breakpoint_entry = bp_entry;
        self.time_scrub = time_scrub;
        if let Some(action) = action {
            self.apply_transport(action, now);
        }
    }

    /// Apply a dock control to the session after the render borrow ends.
    fn apply_transport(&mut self, action: Transport, now: f64) {
        let mut persist_breakpoints = false;
        match action {
            Transport::Stop => self.stop_live_machine(),
            Transport::Restart => self.restart_selected_config(now),
            Transport::ShowDisasm => {
                self.machine_view = super::debug_view::MachineView::Disasm;
                self.layer = Layer::Vm;
            }
            Transport::ShowView(view) => {
                self.machine_view = view;
                self.layer = Layer::Vm;
                self.settings_open = false;
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
                    }
                }
            }
            Transport::ScrubTo(index) => {
                if let Some(live) = &mut self.live {
                    live.restore_timepoint(index);
                    self.time_scrub = index;
                }
                // Refresh machine-derived views against the restored state.
                self.inspect_cache = None;
                self.cache_grid = None;
                self.last_followed_pc = None;
            }
            other => {
                let step_unit = self.step_unit;
                let Some(live) = &mut self.live else {
                    return;
                };
                match other {
                    Transport::Pause => live.pause(),
                    Transport::Resume => live.resume(),
                    Transport::StepOver => live.step_over(),
                    Transport::StepIn => live.step_in(),
                    Transport::StepOut => live.step_out(),
                    Transport::StepLine => live.step_line(),
                    Transport::StepN(count) => match step_unit {
                        StepUnit::Line => {
                            for _ in 0..count {
                                live.step_line();
                            }
                        }
                        StepUnit::Instruction => live.step_instructions(count),
                        StepUnit::Cycle => live.step_cycles(count),
                    },
                    Transport::RemoveBreakpoint(addr) => {
                        live.breakpoints.remove(&addr);
                        persist_breakpoints = true;
                    }
                    Transport::ClearBreakpoints => {
                        live.breakpoints.clear();
                        persist_breakpoints = true;
                    }
                    _ => {}
                }
            }
        }
        if persist_breakpoints {
            self.persist_live_breakpoints();
        }
    }

    /// No session yet: show the selected config and how to start one.
    fn idle_dock(&self, ui: &mut egui::Ui) {
        chrome::indented(ui, |ui| {
            state_line(ui, "idle", theme::TEXT_FAINT);
            if let Some(config) = self.selected_config() {
                chrome::section_eyebrow(ui, "config");
                stat_row(ui, "name", config.name.clone(), theme::TEXT_DIM);
                stat_row(
                    ui,
                    "kind",
                    super::tool_window::kind_tag(config.kind).to_owned(),
                    theme::TEXT_FAINT,
                );
            }
            ui.add_space(metrics::DOCK_SECTION_GAP);
            ui.label(theme::mono(
                "Run (Ctrl+R) to execute\nin the VM.",
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
        });
    }
}

/// The state dot + label, counters, and (on fault) the fault details.
fn state_section(
    ui: &mut egui::Ui,
    live: &full_stack::session::LiveMachine,
    process_count: usize,
) -> Option<Transport> {
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
    ui.add_space(metrics::SECTION_GAP);
    stat_row(ui, "steps", live.steps.to_string(), theme::TEXT_DIM);
    if live.state == MachineState::Running && live.steps_per_sec > 0.0 {
        stat_row(
            ui,
            "rate",
            format!("{}/s", human_rate(live.steps_per_sec)),
            theme::TEXT_FAINT,
        );
    }
    if process_count > 0 {
        stat_row(
            ui,
            "processes",
            process_count.to_string(),
            theme::TEXT_FAINT,
        );
    }
    action
}

/// Session lifecycle controls for the current state, plus the paused stepping
/// group. One equal-width row; the state-advancing verb reads as primary.
fn transport_section(
    ui: &mut egui::Ui,
    state: &MachineState,
    restart_debug: bool,
    step_unit: &mut StepUnit,
    step_count: &mut u64,
) -> Option<Transport> {
    let mut action = None;
    let restart_label = if restart_debug {
        "restart debug"
    } else {
        "restart"
    };
    match state {
        MachineState::Running => {
            let clicked = chrome::action_row(
                ui,
                &[
                    ("pause", ActionTone::Primary),
                    (restart_label, ActionTone::Normal),
                    ("stop", ActionTone::Danger),
                ],
            );
            action = match clicked {
                Some(0) => Some(Transport::Pause),
                Some(1) => Some(Transport::Restart),
                Some(2) => Some(Transport::Stop),
                _ => None,
            };
        }
        MachineState::Paused => {
            let clicked = chrome::action_row(
                ui,
                &[
                    ("resume", ActionTone::Primary),
                    (restart_label, ActionTone::Normal),
                    ("stop", ActionTone::Danger),
                ],
            );
            action = match clicked {
                Some(0) => Some(Transport::Resume),
                Some(1) => Some(Transport::Restart),
                Some(2) => Some(Transport::Stop),
                _ => None,
            };
        }
        MachineState::Exited(_) | MachineState::Stopped | MachineState::Faulted(_) => {
            if chrome::action_row(ui, &[(restart_label, ActionTone::Primary)]) == Some(0) {
                action = Some(Transport::Restart);
            }
        }
    }
    if *state == MachineState::Paused
        && let Some(step) = stepping_section(ui, step_unit, step_count)
    {
        action = Some(step);
    }
    action
}

/// The paused-state stepping group: a unit selector, single-step buttons (step
/// over/in/out for instructions, one tick for cycles), and a bulk step-N row.
fn stepping_section(
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
    ui.add_space(metrics::SECTION_GAP);
    match step_unit {
        StepUnit::Line => {
            if chrome::action_row(ui, &[("next line", ActionTone::Normal)]) == Some(0) {
                action = Some(Transport::StepLine);
            }
        }
        StepUnit::Instruction => {
            let clicked = chrome::action_row(
                ui,
                &[
                    ("over", ActionTone::Normal),
                    ("in", ActionTone::Normal),
                    ("out", ActionTone::Normal),
                ],
            );
            action = match clicked {
                Some(0) => Some(Transport::StepOver),
                Some(1) => Some(Transport::StepIn),
                Some(2) => Some(Transport::StepOut),
                _ => None,
            };
        }
        StepUnit::Cycle => {
            if chrome::action_row(ui, &[("tick", ActionTone::Normal)]) == Some(0) {
                action = Some(Transport::StepN(1));
            }
        }
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
    *scrub = (*scrub).min(last);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Right to left: the now verb hugs the edge, the scrubber fills.
        let now_button = dock_text_button(ui, "now", theme::TEXT_DIM);
        ui.spacing_mut().slider_width = ui.available_width();
        if ui
            .add(egui::Slider::new(scrub, 0..=last).show_value(false))
            .changed()
        {
            action = Some(Transport::ScrubTo(*scrub));
        }
        if now_button.clicked() {
            *scrub = last;
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

/// The machine-surface switch: mirrors the L4 view tabs so the debugger is
/// reachable from the dock no matter which file is active.
fn view_row(
    ui: &mut egui::Ui,
    current: super::debug_view::MachineView,
    on_vm_layer: bool,
    has_screen: bool,
) -> Option<super::debug_view::MachineView> {
    use super::debug_view::MachineView;
    let mut views: Vec<(&str, MachineView)> = vec![("console", MachineView::Console)];
    if has_screen {
        views.push(("screen", MachineView::Screen));
    }
    views.push(("disasm", MachineView::Disasm));
    views.push(("memory", MachineView::Memory));
    let labels: Vec<&str> = views.iter().map(|(label, _)| *label).collect();
    // Off the machine rung nothing reads selected, so any click jumps there.
    let selected = if on_vm_layer {
        views
            .iter()
            .position(|(_, view)| *view == current)
            .unwrap_or(views.len())
    } else {
        views.len()
    };
    chrome::segmented(ui, &labels, selected).map(|i| views[i].1)
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
        MachineState::Exited(0) => ("exit 0".to_owned(), Layer::Vm.color()),
        MachineState::Exited(code) => (format!("exit {code}"), theme::TEXT),
        MachineState::Stopped => ("stopped".to_owned(), theme::TEXT_DIM),
        MachineState::Faulted(_) => ("faulted".to_owned(), theme::DANGER),
    }
}

/// Steps/second with a compact k/M suffix.
fn human_rate(rate: f64) -> String {
    if rate >= 1_000_000.0 {
        format!("{:.1}M", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.0}k", rate / 1_000.0)
    } else {
        format!("{rate:.0}")
    }
}
