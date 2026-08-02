use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use full_stack::session::VmExecutionResult;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConsoleStream {
    Command,
    Info,
    Success,
    Stdout,
    Warning,
    Error,
    Detail,
}

pub(super) struct ConsoleLine {
    stream: ConsoleStream,
    text: String,
    link: Option<(String, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalKind {
    Output,
    Shell,
    Machine,
    Problems,
}

pub(super) struct TerminalState {
    pub(super) active: TerminalKind,
    output_title: String,
    output_lines: Vec<ConsoleLine>,
    shell_lines: Vec<ConsoleLine>,
    shell_input: String,
    shell_history: Vec<String>,
    history_cursor: Option<usize>,
    // Set for one frame after a command runs so the input keeps focus.
    refocus_shell: bool,
    // Set when a run starts so the machine terminal grabs keyboard focus.
    refocus_machine: bool,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            active: TerminalKind::Output,
            output_title: "Output".to_owned(),
            output_lines: vec![ConsoleLine::new(
                ConsoleStream::Info,
                "Select a run configuration, then Build or Run.".to_owned(),
            )],
            shell_lines: welcome_lines(),
            shell_input: String::new(),
            shell_history: Vec::new(),
            history_cursor: None,
            refocus_shell: false,
            refocus_machine: false,
        }
    }
}

fn welcome_lines() -> Vec<ConsoleLine> {
    vec![
        ConsoleLine::new(ConsoleStream::Info, "Full-Stack workspace shell."),
        ConsoleLine::new(ConsoleStream::Info, "Type `help` for commands."),
    ]
}

impl TerminalState {
    pub(super) fn start(&mut self, title: impl Into<String>, command: impl Into<String>) {
        self.active = TerminalKind::Output;
        self.output_title = title.into();
        self.output_lines.clear();
        self.push(ConsoleStream::Command, command);
    }

    pub(super) fn push(&mut self, stream: ConsoleStream, text: impl Into<String>) {
        push_lines(&mut self.output_lines, stream, text);
    }

    pub(super) fn push_linked(
        &mut self,
        stream: ConsoleStream,
        text: impl Into<String>,
        path: String,
        line: usize,
    ) {
        let mut console_line = ConsoleLine::new(stream, text.into());
        console_line.link = Some((path, line));
        self.output_lines.push(console_line);
    }

    pub(super) fn shell_echo(&mut self, stream: ConsoleStream, text: impl Into<String>) {
        push_lines(&mut self.shell_lines, stream, text);
    }

    pub(super) fn shell_clear(&mut self) {
        self.shell_lines = welcome_lines();
    }

    pub(super) fn set_active_output(&mut self) {
        self.active = TerminalKind::Output;
    }

    pub(super) fn set_active_machine(&mut self) {
        self.active = TerminalKind::Machine;
        self.refocus_machine = true;
    }

    pub(super) fn set_active_problems(&mut self) {
        self.active = TerminalKind::Problems;
    }

    // Exists for test assertions.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(super) fn output_text(&self) -> String {
        self.output_lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn has_error(&self) -> bool {
        self.output_lines
            .iter()
            .any(|line| line.stream == ConsoleStream::Error)
    }

    fn title(&self) -> &str {
        match self.active {
            TerminalKind::Output => &self.output_title,
            TerminalKind::Shell => "Shell",
            TerminalKind::Machine => "Machine",
            TerminalKind::Problems => "Problems",
        }
    }

    fn active_lines(&self) -> &[ConsoleLine] {
        match self.active {
            TerminalKind::Output | TerminalKind::Problems => &self.output_lines,
            TerminalKind::Shell | TerminalKind::Machine => &self.shell_lines,
        }
    }

    fn record_history(&mut self, raw: &str) {
        self.history_cursor = None;
        if self.shell_history.last().map(String::as_str) != Some(raw) {
            self.shell_history.push(raw.to_owned());
        }
    }

    fn recall_history(&mut self, back: bool) {
        if self.shell_history.is_empty() {
            return;
        }
        let last = self.shell_history.len() - 1;
        self.history_cursor = match (self.history_cursor, back) {
            (None, true) => Some(last),
            (Some(0), true) => Some(0),
            (Some(i), true) => Some(i - 1),
            (Some(i), false) if i >= last => None,
            (Some(i), false) => Some(i + 1),
            (None, false) => None,
        };
        self.shell_input = match self.history_cursor {
            Some(i) => self.shell_history[i].clone(),
            None => String::new(),
        };
    }
}

fn push_lines(sink: &mut Vec<ConsoleLine>, stream: ConsoleStream, text: impl Into<String>) {
    let text = text.into();
    if text.is_empty() {
        sink.push(ConsoleLine::new(stream, ""));
        return;
    }
    for line in text.lines() {
        sink.push(ConsoleLine::new(stream, line));
    }
}

impl ConsoleLine {
    fn new(stream: ConsoleStream, text: impl Into<String>) -> Self {
        Self {
            stream,
            text: text.into(),
            link: None,
        }
    }
}

impl FullStackApp {
    pub(super) fn terminal_panel(&mut self, ui: &mut egui::Ui, now: f64) {
        self.terminal_header(ui);
        chrome::hairline(ui);
        if self.terminal.active == TerminalKind::Machine {
            self.machine_terminal(ui, "machine", true);
            return;
        }
        if self.terminal.active == TerminalKind::Problems {
            if let Some((path, line)) = self.problems_body(ui) {
                self.jump_to_source(&path, line);
            }
            return;
        }
        let (command, jump) = console_body(ui, &mut self.terminal);
        if let Some(command) = command {
            self.shell_execute(&command, now);
        }
        if let Some((path, line)) = jump {
            self.jump_to_source(&path, line);
        }
    }

    // The transcript itself is the input surface; a focus ring marks guest keyboard focus.
    pub(super) fn machine_terminal(
        &mut self,
        ui: &mut egui::Ui,
        id_salt: &'static str,
        take_autofocus: bool,
    ) {
        if self.live.is_none() && self.vm_result.is_none() {
            chrome::centered_hint(ui, "No machine session. Press Run to start one.");
            return;
        }
        let rect = ui.available_rect_before_wrap();
        if !rect.is_positive() {
            return;
        }
        let alive = self.live.as_ref().is_some_and(|live| live.state.is_alive());
        let id = ui.make_persistent_id(("machine_terminal", id_salt));
        let focused = alive && ui.memory(|mem| mem.has_focus(id));
        if focused {
            // Keep Tab/arrows on the terminal instead of moving egui focus.
            ui.memory_mut(|mem| {
                mem.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
            self.route_terminal_keys(ui);
        }

        ui.painter().rect_filled(rect, 0.0, theme::TERMINAL);
        let ring = if focused {
            theme::ELECTRIC
        } else {
            theme::HAIRLINE_SOFT
        };
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0, ring),
            egui::StrokeKind::Inside,
        );

        let mut text: String = match (&self.live, &self.vm_result) {
            (Some(live), _) => live.uart_log.clone(),
            (None, Some(vm)) => vm.uart_output.clone(),
            (None, None) => String::new(),
        };
        if text.is_empty() && !alive {
            text.push_str("(no console output)");
        }
        if focused && cursor_blink_on(ui) {
            text.push('_');
        }
        let content = rect.shrink2(egui::vec2(metrics::TERMINAL_PAD_X, metrics::TERMINAL_PAD_Y));
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("machine_body", id_salt))
                .max_rect(content),
            |ui| {
                ui.set_clip_rect(content);
                super::editor::console_view(ui, id_salt, &text, true);
            },
        );
        ui.advance_cursor_after_rect(rect);

        // The transcript itself is the input surface: click it to type.
        let response = ui.interact(rect, id, egui::Sense::click());
        if response.clicked()
            || (take_autofocus && std::mem::take(&mut self.terminal.refocus_machine) && alive)
        {
            response.request_focus();
        }
        if alive {
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            if !focused {
                hint_pill(ui, rect, "click to type into the guest");
            }
        }
    }

    // Enter is newline, Backspace 0x08, Ctrl+letter a control byte.
    fn route_terminal_keys(&mut self, ui: &egui::Ui) {
        let events = ui.input(|i| i.events.clone());
        let Some(live) = self.live.as_mut() else {
            return;
        };
        for event in events {
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => {
                    for byte in text.bytes().filter(u8::is_ascii) {
                        live.push_uart(byte);
                    }
                }
                // egui-winit converts Ctrl+C/Ctrl+X into Copy/Cut and never emits
                // the Key event, so map them to the guest's control bytes here.
                egui::Event::Copy => live.push_uart(0x03),
                egui::Event::Cut => live.push_uart(0x18),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.command || modifiers.ctrl {
                        if let Some(ctrl) = super::editor::ctrl_scancode(key) {
                            live.push_uart(ctrl as u8);
                        }
                        continue;
                    }
                    match key {
                        egui::Key::Enter => live.push_uart(b'\n'),
                        egui::Key::Backspace => live.push_uart(0x08),
                        egui::Key::Tab => live.push_uart(b'\t'),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    // The program name lives in the tab strip, nowhere else.
    fn terminal_header(&mut self, ui: &mut egui::Ui) {
        let status = if self.terminal.has_error() {
            Some(("failed".to_owned(), theme::DANGER))
        } else {
            self.vm_result.as_ref().map(status_chip)
        };

        // Reserved now, sized once the row's height is known: this strip belongs to
        // the chrome tier like the editor's tab strip, not to the console below it.
        let band = ui.painter().add(egui::Shape::Noop);
        let band_top = ui.cursor().top();

        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        let mut close = false;
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.spacing_mut().item_spacing.x = 2.0;
            terminal_mode_button(
                ui,
                &mut self.terminal.active,
                TerminalKind::Output,
                "output",
            );
            terminal_mode_button(ui, &mut self.terminal.active, TerminalKind::Shell, "shell");
            terminal_mode_button(
                ui,
                &mut self.terminal.active,
                TerminalKind::Machine,
                "machine",
            );
            terminal_mode_button(
                ui,
                &mut self.terminal.active,
                TerminalKind::Problems,
                "problems",
            );
            ui.spacing_mut().item_spacing.x = 8.0;
            if self.terminal.active == TerminalKind::Machine
                && let Some(live) = &self.live
            {
                ui.label(theme::mono(
                    "\u{00B7}",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
                let (label, color) = super::machine_dock::state_style(&live.state);
                ui.label(theme::mono(label, metrics::STATUS_TEXT_SIZE, color));
            }
            if self.terminal.active == TerminalKind::Output {
                ui.label(theme::mono(
                    "\u{00B7}",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
                ui.label(theme::label(self.terminal.title()));
                if let Some((text, color)) = status {
                    ui.label(theme::mono(text, metrics::STATUS_TEXT_SIZE, color));
                }
            }
            if self.terminal.active == TerminalKind::Problems {
                ui.label(theme::mono(
                    "\u{00B7}",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
                let (errors, warnings) = self
                    .stage_cache
                    .as_ref()
                    .map(|cache| super::diagnostics::count_levels(&cache.result.diagnostics))
                    .unwrap_or((0, 0));
                let (text, color) = match (errors, warnings) {
                    (0, 0) => ("no problems".to_owned(), theme::TEXT_FAINT),
                    (e, 0) => (super::diagnostics::tally(e as u64, "error"), theme::DANGER),
                    (0, w) => (
                        super::diagnostics::tally(w as u64, "warning"),
                        theme::WARNING,
                    ),
                    (e, w) => (
                        format!(
                            "{} \u{00B7} {}",
                            super::diagnostics::tally(e as u64, "error"),
                            super::diagnostics::tally(w as u64, "warning")
                        ),
                        theme::DANGER,
                    ),
                };
                ui.label(theme::mono(text, metrics::STATUS_TEXT_SIZE, color));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(metrics::PANEL_INSET_X);
                if header_glyph(ui, "\u{00D7}").clicked() {
                    close = true;
                }
            });
        });
        let strip = egui::Rect::from_min_max(
            egui::pos2(ui.max_rect().left(), band_top),
            egui::pos2(ui.max_rect().right(), ui.cursor().top()),
        );
        ui.painter()
            .set(band, egui::Shape::rect_filled(strip, 0.0, theme::CHROME));
        if close {
            self.terminal_open = false;
        }
    }

    fn problems_body(&self, ui: &mut egui::Ui) -> Option<(String, usize)> {
        use full_stack::compilation_pipeline::DiagnosticLevel;
        let rect = ui.available_rect_before_wrap();
        if !rect.is_positive() {
            return None;
        }
        ui.painter().rect_filled(rect, 0.0, theme::TERMINAL);
        let content_rect =
            rect.shrink2(egui::vec2(metrics::TERMINAL_PAD_X, metrics::TERMINAL_PAD_Y));
        let root = self
            .active_build_target
            .as_ref()
            .map(|target| target.root_path.as_str().to_owned());
        let diags: &[_] = self
            .stage_cache
            .as_ref()
            .map(|cache| cache.result.diagnostics.as_slice())
            .unwrap_or(&[]);
        let mut jump = None;
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("problems_body")
                .max_rect(content_rect),
            |ui| {
                ui.set_clip_rect(content_rect);
                egui::ScrollArea::vertical()
                    .id_salt("problems_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.set_min_width(content_rect.width());
                        if diags.is_empty() {
                            ui.label(theme::mono(
                                "No problems in the active build target.",
                                metrics::STATUS_TEXT_SIZE,
                                theme::TEXT_FAINT,
                            ));
                        }
                        for diag in diags {
                            let stream = match diag.level {
                                DiagnosticLevel::Error => ConsoleStream::Error,
                                DiagnosticLevel::Warning => ConsoleStream::Warning,
                            };
                            let link = match (&diag.span, &root) {
                                (Some(span), Some(root)) => {
                                    Some((root.clone(), span.line as usize))
                                }
                                _ => None,
                            };
                            let text = match (&diag.span, &root) {
                                (Some(span), Some(root)) => format!(
                                    "{root}:{}:{}  {}",
                                    span.line,
                                    span.col.max(1),
                                    diag.message
                                ),
                                _ => diag.message.clone(),
                            };
                            let mut line = ConsoleLine::new(stream, text);
                            line.link = link;
                            if let Some(target) = console_line(ui, &line) {
                                jump = Some(target);
                            }
                        }
                    });
            },
        );
        ui.advance_cursor_after_rect(rect);
        jump
    }
}

fn console_body(
    ui: &mut egui::Ui,
    terminal: &mut TerminalState,
) -> (Option<String>, Option<(String, usize)>) {
    let rect = ui.available_rect_before_wrap();
    if !rect.is_positive() {
        return (None, None);
    }

    ui.painter().rect_filled(rect, 0.0, theme::TERMINAL);

    let content_rect = rect.shrink2(egui::vec2(metrics::TERMINAL_PAD_X, metrics::TERMINAL_PAD_Y));
    let mut submitted = None;
    let mut jump = None;
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt("terminal_body")
            .max_rect(content_rect),
        |ui| {
            ui.set_clip_rect(content_rect);
            egui::ScrollArea::both()
                .id_salt("terminal_scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.set_min_width(content_rect.width());
                    for line in terminal.active_lines() {
                        if let Some(link) = console_line(ui, line) {
                            jump = Some(link);
                        }
                    }
                    if terminal.active == TerminalKind::Shell {
                        submitted = shell_input_line(ui, terminal);
                    }
                });
        },
    );
    ui.advance_cursor_after_rect(rect);
    (submitted, jump)
}

pub(super) fn console_line(ui: &mut egui::Ui, line: &ConsoleLine) -> Option<(String, usize)> {
    let (prefix, color) = stream_style(line.stream);
    let mut jump = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add(egui::Label::new(theme::mono(
            prefix,
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_FAINT,
        )));
        let Some(link) = &line.link else {
            ui.add(
                egui::Label::new(theme::mono(&line.text, metrics::STATUS_TEXT_SIZE, color))
                    .extend(),
            );
            return;
        };
        let response = ui.add(
            egui::Label::new(theme::mono(&line.text, metrics::STATUS_TEXT_SIZE, color))
                .extend()
                .sense(egui::Sense::click()),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Link, true, line.text.clone())
        });
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let underline =
                egui::Rect::from_min_max(response.rect.left_bottom(), response.rect.right_bottom());
            ui.painter().line_segment(
                [underline.min, underline.max],
                egui::Stroke::new(1.0, color),
            );
        }
        if response.clicked() {
            jump = Some(link.clone());
        }
    });
    jump
}

#[cfg(test)]
pub(super) fn linked_test_line(text: &str, path: &str, line: usize) -> ConsoleLine {
    let mut console_line = ConsoleLine::new(ConsoleStream::Error, text);
    console_line.link = Some((path.to_owned(), line));
    console_line
}

fn status_chip(vm: &VmExecutionResult) -> (String, egui::Color32) {
    match vm.exit_code {
        Some(0) => ("exit 0".to_owned(), theme::ELECTRIC),
        Some(code) => (format!("exit {code}"), theme::TEXT),
        None if vm.max_steps_reached => ("timed out".to_owned(), theme::TEXT_DIM),
        None => ("stopped".to_owned(), theme::TEXT_DIM),
    }
}

fn stream_style(stream: ConsoleStream) -> (&'static str, egui::Color32) {
    match stream {
        ConsoleStream::Command => ("$", theme::ELECTRIC),
        ConsoleStream::Info => ("i", theme::TEXT_DIM),
        ConsoleStream::Success => ("ok", theme::ELECTRIC),
        ConsoleStream::Stdout => (">", theme::TEXT),
        ConsoleStream::Warning => ("!", theme::WARNING),
        ConsoleStream::Error => ("x", theme::DANGER),
        ConsoleStream::Detail => (" ", theme::TEXT_DIM),
    }
}

fn shell_input_line(ui: &mut egui::Ui, terminal: &mut TerminalState) -> Option<String> {
    let mut submitted = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add(egui::Label::new(theme::mono(
            "$",
            metrics::STATUS_TEXT_SIZE,
            theme::ELECTRIC,
        )));
        let response = ui.add(
            egui::TextEdit::singleline(&mut terminal.shell_input)
                .id_salt("terminal_shell_input")
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY),
        );
        if terminal.refocus_shell {
            response.request_focus();
            terminal.refocus_shell = false;
        }
        if response.has_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                terminal.recall_history(true);
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                terminal.recall_history(false);
            }
        }
        // See machine_input_line: Enter already surrendered focus this frame.
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let raw = terminal.shell_input.trim().to_owned();
            terminal.shell_input.clear();
            terminal.refocus_shell = true;
            if !raw.is_empty() {
                submitted = Some(raw);
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    });
    submitted
}

fn terminal_mode_button(
    ui: &mut egui::Ui,
    active: &mut TerminalKind,
    kind: TerminalKind,
    label: &str,
) {
    if chrome::view_tab(ui, label, *active == kind).clicked() {
        *active = kind;
    }
}

fn hint_pill(ui: &egui::Ui, area: egui::Rect, text: &str) {
    let font = egui::FontId::monospace(metrics::STATUS_TEXT_SIZE);
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, theme::TEXT_DIM));
    let pad = egui::vec2(8.0, 4.0);
    let max = area.right_bottom() - egui::vec2(8.0, 8.0);
    let pill = egui::Rect::from_min_max(max - galley.size() - 2.0 * pad, max);
    let painter = ui.painter();
    painter.rect_filled(pill, 0.0, theme::SURFACE);
    painter.rect_stroke(
        pill,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, theme::HAIRLINE),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill.min + pad, galley, theme::TEXT_DIM);
}

fn cursor_blink_on(ui: &egui::Ui) -> bool {
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(250));
    let time = ui.input(|i| i.time);
    (time * 2.0) as i64 % 2 == 0
}

fn header_glyph(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    let resp = ui.add(
        egui::Button::new(theme::mono(
            glyph,
            metrics::LABEL_TEXT_SIZE,
            theme::TEXT_DIM,
        ))
        .frame(false),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

// --- Shell command dispatch ---

impl FullStackApp {
    pub(super) fn shell_execute(&mut self, raw: &str, now: f64) {
        self.terminal.record_history(raw);
        self.terminal
            .shell_echo(ConsoleStream::Command, raw.to_owned());

        let mut parts = raw.split_whitespace();
        let Some(command) = parts.next() else {
            return;
        };
        let args: Vec<&str> = parts.collect();

        match command {
            "help" | "?" => self.shell_help(),
            "clear" | "cls" => self.terminal.shell_clear(),
            "configs" => self.shell_configs(),
            "use" | "config" => self.shell_use(&args),
            "build" => self.shell_build(&args, now),
            "run" => self.shell_run(&args, now),
            "ls" | "dir" => self.shell_ls(args.first().copied()),
            "cat" => self.shell_cat(args.first().copied()),
            "open" | "edit" => self.shell_open(args.first().copied()),
            "out" => self.shell_out(),
            "pwd" => self
                .terminal
                .shell_echo(ConsoleStream::Stdout, "/ (workspace root)"),
            other => self.terminal.shell_echo(
                ConsoleStream::Error,
                format!("unknown command `{other}`; type `help`"),
            ),
        }
    }

    fn shell_help(&mut self) {
        const HELP: &str = "\
help              show this list
build [config]    compile the config
run [config]      build then execute in the VM
configs           list run configurations
use <index>       select a run configuration
ls [path]         list a workspace directory
cat <path>        print a workspace file
open <path>       open a file in the editor
out               explain artifact output
clear             clear the shell";
        self.terminal.shell_echo(ConsoleStream::Info, HELP);
    }

    fn shell_configs(&mut self) {
        if self.configs().is_empty() {
            self.terminal
                .shell_echo(ConsoleStream::Warning, "no run configurations defined");
            return;
        }
        let selected = self.selected_config;
        let lines: Vec<String> = self
            .configs()
            .iter()
            .enumerate()
            .map(|(i, config)| {
                let marker = if i == selected { "*" } else { " " };
                format!("{marker} [{i}] {} ({:?})", config.name, config.kind)
            })
            .collect();
        self.terminal
            .shell_echo(ConsoleStream::Stdout, lines.join("\n"));
    }

    fn shell_use(&mut self, args: &[&str]) {
        let Some(index) = args.first().and_then(|a| a.parse::<usize>().ok()) else {
            self.terminal
                .shell_echo(ConsoleStream::Error, "usage: use <config index>");
            return;
        };
        if index >= self.configs().len() {
            self.terminal
                .shell_echo(ConsoleStream::Error, format!("no config at index {index}"));
            return;
        }
        self.select_config(index);
        let name = self
            .selected_config()
            .map(|c| c.name.clone())
            .unwrap_or_default();
        self.terminal
            .shell_echo(ConsoleStream::Success, format!("selected `{name}`"));
    }

    fn shell_build(&mut self, args: &[&str], now: f64) {
        if !self.select_named_config(args) {
            return;
        }
        self.terminal.set_active_output();
        self.build_selected_config(now);
    }

    fn shell_run(&mut self, args: &[&str], now: f64) {
        if !self.select_named_config(args) {
            return;
        }
        self.terminal.set_active_output();
        self.run_selected_config(now);
    }

    fn select_named_config(&mut self, args: &[&str]) -> bool {
        let Some(arg) = args.first() else {
            return true;
        };
        if let Ok(index) = arg.parse::<usize>() {
            if index >= self.configs().len() {
                self.terminal
                    .shell_echo(ConsoleStream::Error, format!("no config at index {index}"));
                return false;
            }
            self.select_config(index);
            return true;
        }
        if let Some(index) = self.configs().iter().position(|c| c.name == *arg) {
            self.select_config(index);
            true
        } else {
            self.terminal
                .shell_echo(ConsoleStream::Error, format!("no config named `{arg}`"));
            false
        }
    }

    fn shell_ls(&mut self, path: Option<&str>) {
        let vpath = match parse_vpath(path) {
            Ok(vpath) => vpath,
            Err(err) => {
                self.terminal.shell_echo(ConsoleStream::Error, err);
                return;
            }
        };
        match self.workspace.fs().read_dir(&vpath) {
            Ok(entries) if entries.is_empty() => {
                self.terminal
                    .shell_echo(ConsoleStream::Info, "(empty directory)");
            }
            Ok(entries) => {
                let lines: Vec<String> = entries
                    .iter()
                    .map(|entry| match entry.meta.kind {
                        full_stack::workspace::EntryKind::Dir => format!("{}/", entry.name),
                        full_stack::workspace::EntryKind::File => entry.name.clone(),
                    })
                    .collect();
                self.terminal
                    .shell_echo(ConsoleStream::Stdout, lines.join("\n"));
            }
            Err(err) => self
                .terminal
                .shell_echo(ConsoleStream::Error, format!("ls: {err}")),
        }
    }

    fn shell_cat(&mut self, path: Option<&str>) {
        let Some(path) = path else {
            self.terminal
                .shell_echo(ConsoleStream::Error, "usage: cat <path>");
            return;
        };
        let vpath = match parse_vpath(Some(path)) {
            Ok(vpath) => vpath,
            Err(err) => {
                self.terminal.shell_echo(ConsoleStream::Error, err);
                return;
            }
        };
        match self.workspace.fs().read_file(&vpath) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    const MAX: usize = 200;
                    let total = text.lines().count();
                    let shown: String = text.lines().take(MAX).collect::<Vec<_>>().join("\n");
                    self.terminal.shell_echo(ConsoleStream::Stdout, shown);
                    if total > MAX {
                        self.terminal.shell_echo(
                            ConsoleStream::Info,
                            format!(
                                "... {} more",
                                super::diagnostics::tally((total - MAX) as u64, "line")
                            ),
                        );
                    }
                }
                Err(_) => self
                    .terminal
                    .shell_echo(ConsoleStream::Error, "cat: not valid UTF-8"),
            },
            Err(err) => self
                .terminal
                .shell_echo(ConsoleStream::Error, format!("cat: {err}")),
        }
    }

    fn shell_open(&mut self, path: Option<&str>) {
        let Some(path) = path else {
            self.terminal
                .shell_echo(ConsoleStream::Error, "usage: open <path>");
            return;
        };
        let vpath = match parse_vpath(Some(path)) {
            Ok(vpath) => vpath,
            Err(err) => {
                self.terminal.shell_echo(ConsoleStream::Error, err);
                return;
            }
        };
        match self.workspace.open_document(vpath) {
            Ok(()) => {
                self.reset_layer_for_active_file();
                self.terminal
                    .shell_echo(ConsoleStream::Success, format!("opened {path}"));
            }
            Err(err) => self
                .terminal
                .shell_echo(ConsoleStream::Error, format!("open: {err:?}")),
        }
    }

    fn shell_out(&mut self) {
        self.terminal.shell_echo(
            ConsoleStream::Info,
            "artifacts stay in memory unless the build manifest sets `out`",
        );
    }
}

pub(super) fn parse_vpath(path: Option<&str>) -> Result<full_stack::workspace::VPath, String> {
    match path {
        None => Ok(full_stack::workspace::VPath::root()),
        Some(raw) => full_stack::workspace::VPath::parse(raw).map_err(|err| err.to_string()),
    }
}
