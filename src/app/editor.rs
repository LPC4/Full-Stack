use super::diagnostics::DiagMark;
use super::find::FindHighlight;
use super::{FullStackApp, SourceKind, chrome, metrics};
use crate::ui::{Layer, theme};
use eframe::egui;
use egui::text::{CCursor, LayoutJob, LayoutSection};
use egui::{FontId, Sense, TextStyle, pos2, vec2};
use full_stack::compilation_pipeline::assembled_to_elf_file;
use full_stack::session::MachineState;

const FB_WIDTH: usize = 320;
const FB_HEIGHT: usize = 240;

type Highlighter = fn(&egui::Style, &str) -> LayoutJob;

/// Source-level debug overlay for the active editor file: the paused line and
/// the file's breakpoint lines (both 1-based). Clicks toggle via the caller.
#[derive(Default)]
struct SourceDebug {
    current_line: Option<usize>,
    breakpoints: Vec<usize>,
}

impl FullStackApp {
    /// Map the live session's breakpoints onto `file`'s gutter lines. The pc
    /// is only surfaced by the machine Code view; editor tabs never mark it.
    fn source_debug_for(&self, file: &str) -> SourceDebug {
        let Some(live) = &self.live else {
            return SourceDebug::default();
        };
        if !live.has_line_table() {
            return SourceDebug::default();
        }
        let mut breakpoints: Vec<usize> = live
            .breakpoints
            .iter()
            .filter_map(|&addr| live.source_for_pc(addr))
            .filter(|(bp_file, _)| *bp_file == file)
            .map(|(_, line)| line as usize)
            .collect();
        breakpoints.sort_unstable();
        breakpoints.dedup();
        SourceDebug {
            current_line: None,
            breakpoints,
        }
    }

    /// Toggle a source breakpoint on `line` of `file`; the line must carry code
    /// (map to a pc) or the click is reported as a no-op in the status bar.
    fn toggle_source_breakpoint(&mut self, file: &str, line: usize) {
        let Some(live) = &mut self.live else {
            return;
        };
        let Some(pc) = live.pc_for_source(file, line as u32) else {
            self.status_message = Some(format!("no code on line {line} to break on"));
            return;
        };
        if !live.breakpoints.remove(&pc) {
            live.breakpoints.insert(pc);
        }
        self.persist_live_breakpoints();
    }

    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
        if self.settings_open {
            self.settings_page(ui);
            return;
        }
        if !self.editor_tabs(ui) {
            self.cursor_pos = None;
            chrome::centered_hint(ui, "Open a file from the Explorer to begin.");
            return;
        }
        self.cursor_pos = None;
        // The machine surface is its own [debug] tab: when it is focused the
        // canvas shows the debugger, independent of the open documents.
        if self.layer == Layer::Vm {
            self.vm_view(ui);
            return;
        }
        // Binary documents (.elf/.o) get the decoded ELF viewer instead of text.
        if let Some(doc) = self.workspace.active_document()
            && doc.is_binary()
        {
            let name = doc.path().to_string();
            super::elf_view::show(ui, &name, doc.bytes().unwrap_or_default());
            return;
        }
        let active = self.workspace.active_document().map(|doc| {
            (
                doc.text().to_owned(),
                doc.is_read_only(),
                doc.path().to_string(),
            )
        });
        let Some((text, read_only, name)) = active else {
            self.cursor_pos = None;
            chrome::centered_hint(ui, "Open a file from the Explorer to begin.");
            return;
        };
        ui.add_space(metrics::SECTION_GAP);

        let source_kind = Self::source_kind_for_path(&name);
        if source_kind != SourceKind::Other
            && self.layer != Self::source_layer_for_kind(source_kind)
        {
            self.cursor_pos = None;
            self.stage_view(ui);
            return;
        }

        let find = self.find_bar(ui, &text);
        let highlighter = match source_kind {
            SourceKind::Hll => Some(full_stack::highlight::highlight_code as Highlighter),
            SourceKind::Ir => Some(full_stack::highlight::highlight_ir as Highlighter),
            SourceKind::Asm => Some(full_stack::highlight::highlight_assembly as Highlighter),
            SourceKind::Build | SourceKind::Elf | SourceKind::Other => None,
        };
        let marks = self.active_diagnostic_marks(&text);
        // Held (not taken) until the scroll lands, so a jump into a freshly
        // opened tab survives the scroll area's first clamped frame.
        let jump = self
            .pending_jump
            .as_ref()
            .filter(|(path, _)| path.as_str() == name)
            .map(|(_, line)| super::diagnostics::byte_offset_of_line(&text, *line));
        let source_debug = self.source_debug_for(&name);
        let output = code_view(
            ui,
            CodeViewConfig {
                id_salt: ("source", name.as_str()),
                text: &text,
                highlighter,
                interactive: !read_only,
                show_current_line: self.settings.current_line_highlight,
                find: find.as_ref(),
                marks: &marks,
                jump_to: jump,
                debug: &source_debug,
            },
        );
        if let Some(edited) = output.edited {
            self.workspace.set_active_text(edited).ok();
            self.stage_cache = None;
            self.mark_build_stale();
        }
        if let Some(line) = output.bp_toggle {
            self.toggle_source_breakpoint(&name, line);
        }
        if jump.is_some() && output.jump_landed {
            self.pending_jump = None;
        }
        self.cursor_pos = output.cursor;
    }

    fn stage_view(&mut self, ui: &mut egui::Ui) {
        match self.active_source_kind() {
            SourceKind::Build | SourceKind::Hll => {}
            // Elf never reaches here; the editor routes binary docs to elf_view.
            SourceKind::Elf | SourceKind::Other => return,
            SourceKind::Ir => {
                let text = match self.layer {
                    Layer::Asm => "IR to assembly is not wired for standalone .ir files yet.",
                    Layer::Elf => "Linking is not wired for standalone .ir files yet.",
                    Layer::Vm => "VM execution is not wired for standalone .ir files yet.",
                    Layer::Hll | Layer::Ir => "",
                };
                read_only_code(ui, self.layer.tag(), text, None);
                return;
            }
            SourceKind::Asm => {
                let text = match self.layer {
                    Layer::Elf => "Assembly/linking is not wired for standalone .s files yet.",
                    Layer::Vm => "VM execution is not wired for standalone .s files yet.",
                    Layer::Hll | Layer::Ir | Layer::Asm => "",
                };
                read_only_code(ui, self.layer.tag(), text, None);
                return;
            }
        }
        if self.layer == Layer::Vm {
            self.vm_view(ui);
            return;
        }
        self.refresh_stage_cache();

        let Some(cache) = &self.stage_cache else {
            chrome::centered_hint(ui, "Open a file from the Explorer to inspect this rung.");
            return;
        };

        if cache.result.has_errors() {
            read_only_code(ui, "diagnostics", &cache.result.format_diagnostics(), None);
            return;
        }

        let (text, highlighter) = match self.layer {
            Layer::Hll => unreachable!("HLL is rendered by the editable source view"),
            Layer::Ir => cache
                .result
                .ir
                .as_ref()
                .map(|ir| {
                    (
                        ir.display.as_str(),
                        Some(full_stack::highlight::highlight_ir as Highlighter),
                    )
                })
                .unwrap_or(("IR was not produced.", None)),
            Layer::Asm => cache
                .result
                .asm
                .as_ref()
                .map(|asm| {
                    (
                        asm.display.as_str(),
                        Some(full_stack::highlight::highlight_assembly as Highlighter),
                    )
                })
                .unwrap_or(("Assembly was not produced.", None)),
            Layer::Elf => {
                if let Some(binary) = &cache.result.binary {
                    let elf = assembled_to_elf_file(&binary.assembled);
                    super::elf_view::show(ui, "stage_elf", &elf);
                } else {
                    let text = cache
                        .result
                        .assembler_error
                        .clone()
                        .unwrap_or_else(|| "No linked binary was produced.".to_owned());
                    read_only_code(ui, "elf", &text, None);
                }
                return;
            }
            Layer::Vm => {
                self.vm_view(ui);
                return;
            }
        };
        read_only_code(ui, self.layer.tag(), text, highlighter);
    }

    /// The L4 view: for a live session, a tabbed machine surface (console,
    /// screen, disassembly, memory); otherwise the post-mortem summary.
    fn vm_view(&mut self, ui: &mut egui::Ui) {
        use super::debug_view::MachineView;
        if self.live.is_none() {
            self.vm_console_summary(ui);
            return;
        }
        let has_screen = self.config_has_screen();
        if self.machine_view == MachineView::Screen && !has_screen {
            self.machine_view = MachineView::Console;
        }
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.spacing_mut().item_spacing.x = 2.0;
            self.machine_view_tab(ui, MachineView::Console, "console");
            if has_screen {
                self.machine_view_tab(ui, MachineView::Screen, "screen");
            } else {
                chrome::view_tab_disabled(
                    ui,
                    "screen",
                    "This config presents no framebuffer.\nSet present to `framebuffer` or `both` in the Run panel.",
                );
            }
            self.machine_view_tab(ui, MachineView::Code, "code");
            self.machine_view_tab(ui, MachineView::Disasm, "disasm");
            self.machine_view_tab(ui, MachineView::Memory, "memory");
        });
        chrome::hairline(ui);
        match self.machine_view {
            MachineView::Console => self.vm_console_summary(ui),
            MachineView::Screen => self.framebuffer_view(ui),
            MachineView::Code => self.show_code_follow(ui),
            MachineView::Disasm => self.show_disasm(ui),
            MachineView::Memory => self.show_memory(ui),
        }
    }

    fn machine_view_tab(
        &mut self,
        ui: &mut egui::Ui,
        view: super::debug_view::MachineView,
        label: &str,
    ) {
        if chrome::view_tab(ui, label, self.machine_view == view).clicked() {
            self.machine_view = view;
        }
    }

    /// The machine Code view: a read-only source pane that follows the pc
    /// across files, holding the last known location over gaps in line info.
    fn show_code_follow(&mut self, ui: &mut egui::Ui) {
        let Some(live) = self.live.as_ref() else {
            chrome::centered_hint(ui, "No machine session. Press Run to start one.");
            return;
        };
        if !live.has_line_table() {
            chrome::centered_hint(ui, "This image carries no source line table.");
            return;
        }
        let paused = live.state == MachineState::Paused;
        let here = live
            .source_for_pc(live.registers().pc)
            .map(|(file, line)| (file.to_owned(), line));
        if here.is_some() {
            self.code_follow = here.clone();
        }
        let Some((file, line)) = self.code_follow.clone() else {
            chrome::centered_hint(ui, "No source location yet. Step or pause the machine.");
            return;
        };

        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::label("code"));
            ui.label(theme::mono(
                format!("{file}:{line}"),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
            if here.is_none() {
                ui.label(theme::mono(
                    "pc outside source \u{00B7} showing last line",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            } else if !paused {
                ui.label(theme::mono(
                    "pause to step",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(metrics::PANEL_INSET_X);
                ui.label(theme::mono(
                    "read-only \u{00B7} click gutter: breakpoint",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            });
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        chrome::hairline(ui);

        let Some(text) = self.debug_source_text(&file) else {
            chrome::centered_hint(ui, &format!("Source for {file} is not available."));
            return;
        };
        let mut debug = self.source_debug_for(&file);
        debug.current_line = Some(line as usize);
        // Scroll only while the followed location is off-screen, latching once
        // it lands so a manual scroll through the file is not fought.
        let moved = self.code_follow_scrolled.as_ref() != Some(&(file.clone(), line));
        let jump = moved.then(|| super::diagnostics::byte_offset_of_line(&text, line as usize));
        let highlighter = match Self::source_kind_for_path(&file) {
            SourceKind::Hll => Some(full_stack::highlight::highlight_code as Highlighter),
            SourceKind::Ir => Some(full_stack::highlight::highlight_ir as Highlighter),
            SourceKind::Asm => Some(full_stack::highlight::highlight_assembly as Highlighter),
            SourceKind::Build | SourceKind::Elf | SourceKind::Other => None,
        };
        let output = code_view(
            ui,
            CodeViewConfig {
                id_salt: ("code_follow", file.clone()),
                text: &text,
                highlighter,
                interactive: false,
                show_current_line: false,
                find: None,
                marks: &[],
                jump_to: jump,
                debug: &debug,
            },
        );
        if let Some(bp_line) = output.bp_toggle {
            self.toggle_source_breakpoint(&file, bp_line);
        }
        if moved && output.jump_landed {
            self.code_follow_scrolled = Some((file, line));
        }
    }

    /// Full text of a line-table `file`: the open buffer, then the workspace
    /// copy, then bundled program and kernel/stdlib sources.
    fn debug_source_text(&self, file: &str) -> Option<String> {
        if let Ok(path) = full_stack::workspace::VPath::parse(file) {
            match self.workspace.document(&path) {
                Some(doc) if !doc.is_binary() => return Some(doc.text().to_owned()),
                _ => {}
            }
            if let Ok(bytes) = self.workspace.fs().read_file(&path)
                && let Ok(text) = String::from_utf8(bytes)
            {
                return Some(text);
            }
        }
        if let Some(source) = full_stack::userspace::source_by_path(file) {
            return Some(source.to_owned());
        }
        let rel = file.strip_prefix("crates/os-runtime/")?;
        os_runtime::SOURCE_TREE
            .iter()
            .find(|(path, _)| *path == rel)
            .map(|(_, source)| (*source).to_owned())
    }

    fn vm_console_summary(&mut self, ui: &mut egui::Ui) {
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        match (&self.live, &self.vm_result) {
            (Some(live), _) => {
                let (state, color) = super::machine_dock::state_style(&live.state);
                let steps = live.steps;
                let sps = live.steps_per_sec;
                ui.horizontal(|ui| {
                    ui.add_space(metrics::PANEL_INSET_X);
                    ui.label(theme::mono(state, metrics::STATUS_TEXT_SIZE, color));
                    ui.label(theme::label(&format!("{steps} steps")));
                    if sps > 0.0 {
                        ui.label(theme::mono(
                            format!("{sps:.0} steps/s"),
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT_FAINT,
                        ));
                    }
                });
            }
            (None, Some(vm)) => {
                let status = match vm.exit_code {
                    Some(code) => format!("halted \u{00B7} exit {code}"),
                    None if vm.max_steps_reached => "step budget exhausted".to_owned(),
                    None => "stopped".to_owned(),
                };
                let color = match vm.exit_code {
                    Some(0) => Layer::Vm.color(),
                    Some(_) => theme::TEXT,
                    None => theme::TEXT_DIM,
                };
                let steps = vm.steps;
                ui.horizontal(|ui| {
                    ui.add_space(metrics::PANEL_INSET_X);
                    ui.label(theme::mono(status, metrics::STATUS_TEXT_SIZE, color));
                    ui.label(theme::label(&format!("{steps} steps")));
                });
            }
            (None, None) => {
                chrome::centered_hint(ui, "Press Run to execute in the VM.");
                return;
            }
        }
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        chrome::hairline(ui);
        self.machine_terminal(ui, "vm_live", false);
    }

    fn framebuffer_view(&mut self, ui: &mut egui::Ui) {
        let Some(live) = self.live.as_ref() else {
            self.vm_console_summary(ui);
            return;
        };
        let fill_count = live.framebuffer_fill_count();
        let pixels = live.framebuffer().to_vec();
        let steps = live.steps;
        let sps = live.steps_per_sec;
        let state = live.state.clone();

        self.upload_framebuffer(ui.ctx(), fill_count, &pixels);
        let Some(texture) = self.machine_texture.as_ref() else {
            chrome::centered_hint(ui, "Framebuffer texture is not ready.");
            return;
        };

        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            let (state_label, color) = super::machine_dock::state_style(&state);
            ui.label(theme::mono(state_label, metrics::STATUS_TEXT_SIZE, color));
            ui.label(theme::label("320x240"));
            ui.label(theme::mono(
                format!("fill {fill_count}"),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
            ui.label(theme::mono(
                format!("{steps} steps"),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
            if sps > 0.0 && state == MachineState::Running {
                ui.label(theme::mono(
                    format!("{sps:.0} steps/s"),
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            }
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        chrome::hairline(ui);

        let rect = ui.available_rect_before_wrap();
        if !rect.is_positive() {
            return;
        }
        let aspect = FB_WIDTH as f32 / FB_HEIGHT as f32;
        let mut size = rect.size();
        if size.x / size.y > aspect {
            size.x = size.y * aspect;
        } else {
            size.y = size.x / aspect;
        }
        let image_rect = egui::Rect::from_center_size(rect.center(), size.floor());
        let id = ui.make_persistent_id("machine_framebuffer_input");
        let response = ui.interact(rect, id, egui::Sense::click());
        if response.clicked() {
            response.request_focus();
        }
        if response.has_focus() {
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
        }
        ui.painter().rect_filled(rect, 0.0, theme::TERMINAL);
        ui.painter().rect_stroke(
            image_rect,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0, theme::HAIRLINE),
            egui::StrokeKind::Inside,
        );
        ui.put(
            image_rect,
            egui::Image::new((texture.id(), image_rect.size()))
                .texture_options(egui::TextureOptions::NEAREST),
        );
        if fill_count == 0 {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "the guest has not drawn to the framebuffer yet",
                egui::FontId::monospace(metrics::STATUS_TEXT_SIZE),
                theme::TEXT_FAINT,
            );
        } else if !response.has_focus() {
            ui.painter().text(
                image_rect.center_bottom() - egui::vec2(0.0, 10.0),
                egui::Align2::CENTER_BOTTOM,
                "click the screen to send it your keys",
                egui::FontId::monospace(metrics::STATUS_TEXT_SIZE),
                theme::TEXT_FAINT,
            );
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.has_focus() {
            self.route_framebuffer_keys(ui);
        }
        ui.advance_cursor_after_rect(rect);
    }

    fn upload_framebuffer(&mut self, ctx: &egui::Context, fill_count: u64, pixels: &[u8]) {
        if self.machine_texture.is_some() && self.machine_texture_fill_count == Some(fill_count) {
            return;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([FB_WIDTH, FB_HEIGHT], pixels);
        match self.machine_texture.as_mut() {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.machine_texture = Some(ctx.load_texture(
                    "machine_framebuffer",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        self.machine_texture_fill_count = Some(fill_count);
    }

    fn route_framebuffer_keys(&mut self, ui: &egui::Ui) {
        let events = ui.input(|i| i.events.clone());
        let Some(live) = self.live.as_mut() else {
            return;
        };
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    for ch in text.chars() {
                        if ch.is_ascii_alphabetic() || ch == ' ' {
                            continue;
                        }
                        if let Some(code) = printable_scancode(ch) {
                            live.push_key_event(code, true);
                        }
                    }
                }
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    repeat,
                    ..
                } => {
                    if repeat {
                        continue;
                    }
                    if let Some(code) = special_scancode(key) {
                        live.push_key_event(code, pressed);
                    }
                    if let Some(code) = key_scancode(key) {
                        live.push_key_event(code, pressed);
                    }
                    if pressed
                        && (modifiers.command || modifiers.ctrl)
                        && let Some(ctrl) = ctrl_scancode(key)
                    {
                        live.push_uart(ctrl as u8);
                    }
                }
                // egui-winit swallows Ctrl+C/Ctrl+X into Copy/Cut (no Key event),
                // so mirror them to the guest UART as control bytes.
                egui::Event::Copy => live.push_uart(0x03),
                egui::Event::Cut => live.push_uart(0x18),
                _ => {}
            }
        }
    }
}

fn printable_scancode(ch: char) -> Option<u16> {
    if ch.is_ascii() && !ch.is_control() {
        Some(ch as u16)
    } else {
        None
    }
}

fn special_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::ArrowUp => Some(0x80),
        egui::Key::ArrowDown => Some(0x81),
        egui::Key::ArrowLeft => Some(0x82),
        egui::Key::ArrowRight => Some(0x83),
        egui::Key::Enter => Some(b'\n' as u16),
        egui::Key::Backspace => Some(8),
        egui::Key::Tab => Some(b'\t' as u16),
        egui::Key::Escape => Some(27),
        _ => None,
    }
}

fn key_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::A => Some(b'A' as u16),
        egui::Key::B => Some(b'B' as u16),
        egui::Key::C => Some(b'C' as u16),
        egui::Key::D => Some(b'D' as u16),
        egui::Key::E => Some(b'E' as u16),
        egui::Key::F => Some(b'F' as u16),
        egui::Key::G => Some(b'G' as u16),
        egui::Key::H => Some(b'H' as u16),
        egui::Key::I => Some(b'I' as u16),
        egui::Key::J => Some(b'J' as u16),
        egui::Key::K => Some(b'K' as u16),
        egui::Key::L => Some(b'L' as u16),
        egui::Key::M => Some(b'M' as u16),
        egui::Key::N => Some(b'N' as u16),
        egui::Key::O => Some(b'O' as u16),
        egui::Key::P => Some(b'P' as u16),
        egui::Key::Q => Some(b'Q' as u16),
        egui::Key::R => Some(b'R' as u16),
        egui::Key::S => Some(b'S' as u16),
        egui::Key::T => Some(b'T' as u16),
        egui::Key::U => Some(b'U' as u16),
        egui::Key::V => Some(b'V' as u16),
        egui::Key::W => Some(b'W' as u16),
        egui::Key::X => Some(b'X' as u16),
        egui::Key::Y => Some(b'Y' as u16),
        egui::Key::Z => Some(b'Z' as u16),
        egui::Key::Space => Some(b' ' as u16),
        _ => None,
    }
}

pub(super) fn ctrl_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::A => Some(0x01),
        egui::Key::B => Some(0x02),
        egui::Key::C => Some(0x03),
        egui::Key::D => Some(0x04),
        egui::Key::E => Some(0x05),
        egui::Key::F => Some(0x06),
        egui::Key::G => Some(0x07),
        egui::Key::H => Some(0x08),
        egui::Key::I => Some(0x09),
        egui::Key::J => Some(0x0A),
        egui::Key::K => Some(0x0B),
        egui::Key::L => Some(0x0C),
        egui::Key::M => Some(0x0D),
        egui::Key::N => Some(0x0E),
        egui::Key::O => Some(0x0F),
        egui::Key::P => Some(0x10),
        egui::Key::Q => Some(0x11),
        egui::Key::R => Some(0x12),
        egui::Key::S => Some(0x13),
        egui::Key::T => Some(0x14),
        egui::Key::U => Some(0x15),
        egui::Key::V => Some(0x16),
        egui::Key::W => Some(0x17),
        egui::Key::X => Some(0x18),
        egui::Key::Y => Some(0x19),
        egui::Key::Z => Some(0x1A),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_key_mapping_matches_vm_contract() {
        assert_eq!(printable_scancode('A'), Some(0x41));
        assert_eq!(special_scancode(egui::Key::ArrowUp), Some(0x80));
        assert_eq!(special_scancode(egui::Key::ArrowDown), Some(0x81));
        assert_eq!(special_scancode(egui::Key::ArrowLeft), Some(0x82));
        assert_eq!(special_scancode(egui::Key::ArrowRight), Some(0x83));
        assert_eq!(key_scancode(egui::Key::W), Some(b'W' as u16));
        assert_eq!(key_scancode(egui::Key::A), Some(b'A' as u16));
        assert_eq!(key_scancode(egui::Key::S), Some(b'S' as u16));
        assert_eq!(key_scancode(egui::Key::D), Some(b'D' as u16));
        assert_eq!(key_scancode(egui::Key::Space), Some(b' ' as u16));
    }

    #[test]
    fn ctrl_key_mapping_mirrors_uart_control_bytes() {
        assert_eq!(ctrl_scancode(egui::Key::A), Some(0x01));
        assert_eq!(ctrl_scancode(egui::Key::C), Some(0x03));
        assert_eq!(ctrl_scancode(egui::Key::Z), Some(0x1A));
        assert_eq!(ctrl_scancode(egui::Key::ArrowUp), None);
    }
}

/// Plain monospace console output (no gutter), filling the remaining height. Sticks
/// to the bottom for live logs (the terminal); anchored at the top otherwise.
pub(super) fn console_view(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    text: &str,
    stick_to_bottom: bool,
) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let mut buf = text.to_owned();
    egui::ScrollArea::both()
        .id_salt(("console_scroll", id_salt))
        .stick_to_bottom(stick_to_bottom)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut buf)
                    .id_salt(("console_edit", id_salt))
                    .font(font)
                    .frame(egui::Frame::NONE)
                    .margin(vec2(metrics::CODE_PAD_X, 0.0))
                    .interactive(false)
                    .desired_width(f32::INFINITY),
            );
        });
}

fn read_only_code(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    text: &str,
    highlighter: Option<Highlighter>,
) {
    code_view(
        ui,
        CodeViewConfig {
            id_salt: ("stage", id_salt),
            text,
            highlighter,
            interactive: false,
            show_current_line: false,
            find: None,
            marks: &[],
            jump_to: None,
            debug: &SourceDebug::default(),
        },
    );
}

/// What one frame of the code surface produced: edits and the caret position.
#[derive(Default)]
struct CodeViewOutput {
    edited: Option<String>,
    /// 1-based line/column of the primary cursor, when the view is interactive.
    cursor: Option<(usize, usize)>,
    /// 1-based gutter line the user clicked to toggle a breakpoint, if any.
    bp_toggle: Option<usize>,
    /// True once a requested `jump_to` target sits inside the viewport, so
    /// callers can keep re-jumping until the scroll actually lands.
    jump_landed: bool,
}

struct CodeViewConfig<'a, Id> {
    id_salt: Id,
    text: &'a str,
    highlighter: Option<Highlighter>,
    interactive: bool,
    show_current_line: bool,
    find: Option<&'a FindHighlight>,
    marks: &'a [DiagMark],
    jump_to: Option<usize>,
    debug: &'a SourceDebug,
}

/// The code surface: gutter + frameless editor filling the column. Finds wash
/// behind the text, diagnostics mark gutter + underline, jumps scroll into view.
fn code_view<Id>(ui: &mut egui::Ui, config: CodeViewConfig<'_, Id>) -> CodeViewOutput
where
    Id: std::hash::Hash + Clone,
{
    let CodeViewConfig {
        id_salt,
        text,
        highlighter,
        interactive,
        show_current_line,
        find,
        marks,
        jump_to,
        debug,
    } = config;
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.fonts_mut(|f| f.row_height(&font));
    let rows = (ui.available_height() / row_height).floor().max(1.0) as usize;
    let line_count = text.matches('\n').count() + 1;

    let mut buf = text.to_owned();
    let mut out = CodeViewOutput::default();
    egui::ScrollArea::both()
        .id_salt(("code_scroll", id_salt.clone()))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal_top(|ui| {
                out.bp_toggle =
                    gutter(ui, line_count, &font, marks, row_height, &debug.breakpoints);
                // Paint last frame's bands first, so this frame's text lands on
                // top of them (egui has no per-shape z within a layer).
                let dbg_id = ui.make_persistent_id(("dbg_line", &id_salt));
                paint_debug_band(ui, dbg_id);
                let hl_id = ui.make_persistent_id(("cur_line", &id_salt));
                let show_cursor_band =
                    interactive && show_current_line && debug.current_line.is_none();
                if show_cursor_band && let Some(band) = ui.data(|d| d.get_temp::<[f32; 4]>(hl_id)) {
                    let rect =
                        egui::Rect::from_min_max(pos2(band[2], band[0]), pos2(band[3], band[1]));
                    ui.painter().rect_filled(rect, 0.0, theme::CURRENT_LINE);
                    ui.ctx().request_repaint();
                }
                let find_marks = find
                    .as_ref()
                    .map(|f| (f.ranges.clone(), f.current))
                    .unwrap_or_default();
                let diag_spans: Vec<(usize, usize, egui::Color32)> = marks
                    .iter()
                    .filter_map(|mark| mark.range.map(|(start, end)| (start, end, mark.color())))
                    .collect();
                let mut layouter = highlighted_layouter(highlighter, &font, find_marks, diag_spans);
                let output = egui::TextEdit::multiline(&mut buf)
                    .id_salt(("code_edit", id_salt.clone()))
                    .font(font.clone())
                    .frame(egui::Frame::NONE)
                    .margin(vec2(metrics::CODE_PAD_X, 0.0))
                    .interactive(interactive)
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows)
                    .layouter(&mut layouter)
                    .show(ui);
                if let Some(byte) = find.as_ref().and_then(|f| f.jump_to).or(jump_to) {
                    let chars = buf[..byte.min(buf.len())].chars().count();
                    let row = output.galley.pos_from_cursor(CCursor::new(chars));
                    let target = egui::Rect::from_min_max(
                        pos2(
                            output.galley_pos.x + row.min.x,
                            output.galley_pos.y + row.min.y,
                        ),
                        pos2(
                            output.galley_pos.x + row.max.x,
                            output.galley_pos.y + row.max.y,
                        ),
                    );
                    // Landed only once the row is inside the visible viewport;
                    // a fresh scroll area can clamp the first jump to the top.
                    let clip = ui.clip_rect();
                    out.jump_landed = target.min.y >= clip.top() && target.max.y <= clip.bottom();
                    if !out.jump_landed {
                        ui.scroll_to_rect(target.expand2(vec2(40.0, 80.0)), None);
                    }
                }
                if interactive {
                    let band = output.cursor_range.map(|range| {
                        let row = output.galley.pos_from_cursor(range.primary);
                        let clip = output.text_clip_rect;
                        [
                            output.galley_pos.y + row.min.y,
                            output.galley_pos.y + row.max.y,
                            clip.left(),
                            clip.right(),
                        ]
                    });
                    ui.data_mut(|d| match band {
                        Some(band) => {
                            d.insert_temp(hl_id, band);
                        }
                        None => {
                            d.remove::<[f32; 4]>(hl_id);
                        }
                    });
                    out.cursor = output
                        .cursor_range
                        .map(|range| line_col(&buf, range.primary.index));
                    if output.response.changed() {
                        out.edited = Some(buf.clone());
                    }
                }
                hover_diagnostic_spans(ui, &output, &buf, marks);
                // Stash the paused line's row band to paint under next frame's text.
                let dbg_band = debug
                    .current_line
                    .map(|line| line_band(&output, &buf, line));
                ui.data_mut(|d| match dbg_band {
                    Some(band) => {
                        d.insert_temp(dbg_id, band);
                    }
                    None => {
                        d.remove::<[f32; 4]>(dbg_id);
                    }
                });
            });
        });
    out
}

/// Hover tooltips on the underlined spans themselves, not just the gutter bar.
fn hover_diagnostic_spans(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    buf: &str,
    marks: &[DiagMark],
) {
    for (i, mark) in marks.iter().enumerate() {
        let Some((start, end)) = mark.range else {
            continue;
        };
        // The live buffer may be newer than the marks; skip stale ranges.
        if end > buf.len() || !buf.is_char_boundary(start) || !buf.is_char_boundary(end) {
            continue;
        }
        let from = buf[..start].chars().count();
        let to = from + buf[start..end].chars().count();
        let head = output.galley.pos_from_cursor(CCursor::new(from));
        let tail = output.galley.pos_from_cursor(CCursor::new(to));
        let rect = egui::Rect::from_min_max(
            output.galley_pos + head.min.to_vec2(),
            output.galley_pos + tail.max.to_vec2(),
        );
        ui.interact(rect, ui.id().with(("diag_span", i)), Sense::hover())
            .on_hover_text(theme::mono(
                mark.message.clone(),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ));
    }
}

/// Paint the paused source line as a filled band with an electric left bar,
/// from the coords stashed last frame, so this frame's text lands on top.
fn paint_debug_band(ui: &egui::Ui, dbg_id: egui::Id) {
    let Some(band) = ui.data(|d| d.get_temp::<[f32; 4]>(dbg_id)) else {
        return;
    };
    let rect = egui::Rect::from_min_max(pos2(band[2], band[0]), pos2(band[3], band[1]));
    ui.painter().rect_filled(rect, 0.0, theme::SURFACE);
    let bar = egui::Rect::from_min_max(rect.left_top(), pos2(band[2] + 2.0, band[1]));
    ui.painter().rect_filled(bar, 0.0, theme::ELECTRIC);
}

/// The `[top, bottom, left, right]` band for a 1-based `line`, in the code
/// view's screen space, so it can be stored and painted under next frame's text.
fn line_band(output: &egui::text_edit::TextEditOutput, buf: &str, line: usize) -> [f32; 4] {
    let byte = super::diagnostics::byte_offset_of_line(buf, line);
    let chars = buf[..byte.min(buf.len())].chars().count();
    let row = output.galley.pos_from_cursor(CCursor::new(chars));
    let clip = output.text_clip_rect;
    [
        output.galley_pos.y + row.min.y,
        output.galley_pos.y + row.max.y,
        clip.left(),
        clip.right(),
    ]
}

/// 1-based line/column for a char offset into `text`.
fn line_col(text: &str, char_idx: usize) -> (usize, usize) {
    let (mut line, mut col) = (1, 1);
    for c in text.chars().take(char_idx) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Right-aligned line numbers with diagnostic bars and breakpoint dots; a
/// gutter click toggles a breakpoint on the line under the pointer.
fn gutter(
    ui: &mut egui::Ui,
    line_count: usize,
    font: &FontId,
    marks: &[DiagMark],
    row_height: f32,
    breakpoints: &[usize],
) -> Option<usize> {
    let digits = line_count.to_string().len().max(2);
    let mut nums = String::with_capacity(line_count * (digits + 1));
    for i in 1..=line_count {
        if i > 1 {
            nums.push('\n');
        }
        nums.push_str(&format!("{i:>digits$}"));
    }
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(nums, font.clone(), theme::TEXT_FAINT));
    // Rows are pixel-snapped, so their pitch can differ from the font's nominal
    // row height; measure it or gutter marks drift below the code lines.
    let pitch = match galley.rows.get(1) {
        Some(row) => row.pos.y - galley.rows[0].pos.y,
        None => row_height,
    };
    let width = galley.size().x + metrics::CODE_PAD_X + metrics::GUTTER_GAP;
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(
        pos2(rect.left() + metrics::CODE_PAD_X, rect.top()),
        galley,
        theme::TEXT_FAINT,
    );

    let dot_x = rect.left() + 6.0;
    let line_of = |y: f32| -> Option<usize> {
        let row = (y - rect.top()) / pitch;
        if row < 0.0 {
            return None;
        }
        let line = row.floor() as usize + 1;
        (line <= line_count).then_some(line)
    };
    let dot_y = |line: usize| rect.top() + (line as f32 - 0.5) * pitch;
    for &line in breakpoints {
        if line >= 1 && line <= line_count {
            ui.painter()
                .circle_filled(pos2(dot_x, dot_y(line)), 3.0, theme::DANGER);
        }
    }
    let resp = ui.interact(rect, ui.id().with("gutter_bp"), Sense::click());
    if let Some(line) = resp.hover_pos().and_then(|p| line_of(p.y)) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if !breakpoints.contains(&line) {
            ui.painter().circle_stroke(
                pos2(dot_x, dot_y(line)),
                3.0,
                egui::Stroke::new(1.0, theme::TEXT_FAINT),
            );
        }
    }
    let toggled = if resp.clicked() {
        resp.interact_pointer_pos().and_then(|p| line_of(p.y))
    } else {
        None
    };

    for (i, mark) in marks.iter().enumerate() {
        if mark.line == 0 || mark.line > line_count {
            continue;
        }
        let top = rect.top() + (mark.line - 1) as f32 * pitch;
        let bar = egui::Rect::from_min_size(pos2(rect.left(), top + 1.0), vec2(3.0, pitch - 2.0));
        ui.painter().rect_filled(bar, 0.0, mark.color());
        let hover = egui::Rect::from_min_size(pos2(rect.left(), top), vec2(width, pitch));
        ui.interact(hover, ui.id().with(("diag_mark", i)), Sense::hover())
            .on_hover_text(theme::mono(
                mark.message.clone(),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ));
    }
    toggled
}

fn highlighted_layouter(
    highlighter: Option<Highlighter>,
    font: &FontId,
    find_marks: (Vec<(usize, usize)>, usize),
    diag_spans: Vec<(usize, usize, egui::Color32)>,
) -> impl FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> std::sync::Arc<egui::Galley> {
    let font = font.clone();
    move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = highlighter
            .map(|highlight| highlight(ui.style(), buf.as_str()))
            .unwrap_or_else(|| {
                LayoutJob::simple(buf.as_str().to_owned(), font.clone(), theme::TEXT, wrap)
            });
        apply_match_backgrounds(&mut job, &find_marks.0, find_marks.1);
        apply_diagnostic_underlines(&mut job, &diag_spans);
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    }
}

/// Re-split the job's sections at match boundaries and wash the covered spans.
fn apply_match_backgrounds(job: &mut LayoutJob, ranges: &[(usize, usize)], current: usize) {
    let ranges = valid_ranges(&job.text, ranges.iter().copied());
    restyle_ranges(job, &ranges, |idx, format| {
        format.background = if idx == current {
            theme::FIND_MATCH_CURRENT
        } else {
            theme::FIND_MATCH
        };
    });
}

/// Underline diagnostic spans in their level color (DANGER/WARNING) and wash
/// their background so the offending text reads at a glance.
fn apply_diagnostic_underlines(job: &mut LayoutJob, spans: &[(usize, usize, egui::Color32)]) {
    let ranges = valid_ranges(&job.text, spans.iter().map(|&(start, end, _)| (start, end)));
    let colors: Vec<egui::Color32> = spans.iter().map(|&(_, _, color)| color).collect();
    restyle_ranges(job, &ranges, |idx, format| {
        format.underline = egui::Stroke::new(1.0, colors[idx]);
        format.background = if colors[idx] == theme::DANGER {
            theme::DANGER_WASH
        } else {
            theme::WARNING_WASH
        };
    });
}

/// Ranges validated against the live buffer, which may be newer than them.
fn valid_ranges(text: &str, ranges: impl Iterator<Item = (usize, usize)>) -> Vec<(usize, usize)> {
    ranges
        .filter(|&(start, end)| {
            end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end)
        })
        .collect()
}

/// Re-split the job's sections at range boundaries; `restyle(idx, format)` runs
/// on every window that falls inside `ranges[idx]`.
fn restyle_ranges(
    job: &mut LayoutJob,
    ranges: &[(usize, usize)],
    mut restyle: impl FnMut(usize, &mut egui::text::TextFormat),
) {
    if ranges.is_empty() {
        return;
    }
    let mut sections = Vec::with_capacity(job.sections.len() + ranges.len() * 2);
    for section in job.sections.drain(..) {
        let (start, end) = (section.byte_range.start, section.byte_range.end);
        let mut cuts = vec![start];
        for &(range_start, range_end) in ranges {
            if range_start > start && range_start < end {
                cuts.push(range_start);
            }
            if range_end > start && range_end < end {
                cuts.push(range_end);
            }
        }
        cuts.push(end);
        cuts.sort_unstable();
        cuts.dedup();

        let mut leading_space = section.leading_space;
        for window in cuts.windows(2) {
            let (a, b) = (window[0], window[1]);
            let mut format = section.format.clone();
            // Every window is fully inside or fully outside a range, because the
            // cuts include all range boundaries that land within the section.
            if let Some(idx) = ranges
                .iter()
                .position(|&(range_start, range_end)| a >= range_start && b <= range_end)
            {
                restyle(idx, &mut format);
            }
            sections.push(LayoutSection {
                leading_space,
                byte_range: a..b,
                format,
            });
            leading_space = 0.0;
        }
    }
    job.sections = sections;
}
