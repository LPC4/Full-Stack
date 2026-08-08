use super::diagnostics::DiagMark;
use super::find::FindHighlight;
use super::{FullStackApp, SourceKind, chrome, metrics};
use crate::ui::{Layer, theme};
use eframe::egui;
use egui::text::{CCursor, CCursorRange, LayoutJob, LayoutSection};
use egui::{FontId, Sense, TextStyle, pos2, vec2};
use full_stack::compilation_pipeline::assembled_to_elf_file;
use full_stack::session::MachineState;

const FB_WIDTH: usize = 320;
const FB_HEIGHT: usize = 240;
const FB_DEFAULT_SCALE: f32 = 1.5;
const FB_MIN_SCALE: f32 = 0.75;

type Highlighter = fn(&egui::Style, &str) -> LayoutJob;

#[derive(Default)]
struct SourceDebug {
    current_line: Option<usize>,
    breakpoints: Vec<usize>,
}

impl FullStackApp {
    // The pc is only surfaced by the machine Code view; editor tabs never mark it.
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

    // Label, blank, and comment lines slide forward to the next line carrying code.
    pub(super) fn toggle_source_breakpoint(&mut self, file: &str, line: usize) {
        let Some(live) = &mut self.live else {
            return;
        };
        let Some(pc) = live.pc_at_or_after_source(file, line as u32) else {
            self.status_message = Some(format!("no code at or after line {line} to break on"));
            return;
        };
        if !live.breakpoints.remove(&pc) {
            live.breakpoints.insert(pc);
        }
        self.persist_live_breakpoints();
    }

    // Slides forward like a breakpoint toggle, without leaving a breakpoint behind.
    pub(super) fn run_to_source_line(&mut self, file: &str, line: usize) {
        let Some(live) = &mut self.live else {
            return;
        };
        let Some(pc) = live.pc_at_or_after_source(file, line as u32) else {
            self.status_message = Some(format!("no code at or after line {line} to run to"));
            return;
        };
        live.run_to(pc);
    }

    pub(super) fn caret_function_name(&self) -> Option<String> {
        if self.active_source_kind() != SourceKind::Hll {
            return None;
        }
        let (line, _) = self.cursor_pos?;
        let doc = self.workspace.active_document()?;
        enclosing_function(doc.text(), line)
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
        match self.main_surface {
            super::MainSurface::Debugger => {
                self.debugger_surface(ui);
                return;
            }
            super::MainSurface::Document => {}
        }
        // Binary documents (.elf/.o) get the decoded ELF viewer instead of text.
        if let Some(doc) = self.workspace.active_document()
            && doc.is_binary()
        {
            let name = doc.path().to_string();
            let raw_url = self
                .repository_paths
                .get(doc.path())
                .map(crate::web_reference::raw_file_url);
            super::elf_view::show(
                ui,
                &name,
                doc.bytes().unwrap_or_default(),
                raw_url.as_deref(),
            );
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
        if let Some(line) = output.run_to {
            self.run_to_source_line(&name, line);
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
                    Layer::Hll | Layer::Ir => "",
                };
                read_only_code(ui, self.layer.name(), text, None, None);
                return;
            }
            SourceKind::Asm => {
                let text = match self.layer {
                    Layer::Elf => "Assembly/linking is not wired for standalone .s files yet.",
                    Layer::Hll | Layer::Ir | Layer::Asm => "",
                };
                read_only_code(ui, self.layer.name(), text, None, None);
                return;
            }
        }
        self.refresh_stage_cache();

        let Some(cache) = &self.stage_cache else {
            chrome::centered_hint(ui, "Open a file from the Explorer to inspect this rung.");
            return;
        };

        if cache.result.has_errors() {
            read_only_code(
                ui,
                "diagnostics",
                &cache.result.format_diagnostics(),
                None,
                None,
            );
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
                    super::elf_view::show(ui, "stage_elf", &elf, None);
                } else {
                    let text = cache
                        .result
                        .assembler_error
                        .clone()
                        .unwrap_or_else(|| "No linked binary was produced.".to_owned());
                    read_only_code(ui, "elf", &text, None, None);
                }
                return;
            }
        };
        // Stage sync: a spine click carries the caret's enclosing function, so
        // the IR/ASM view opens scrolled to that function's lowering.
        let jump = self
            .stage_locate
            .as_deref()
            .and_then(|name| locate_in_stage(text, self.layer, name));
        let output = read_only_code(ui, self.layer.name(), text, highlighter, jump);
        if self.stage_locate.is_some() && (jump.is_none() || output.jump_landed) {
            self.stage_locate = None;
        }
    }

    // Runtime console and framebuffer deliberately live elsewhere.
    fn debugger_surface(&mut self, ui: &mut egui::Ui) {
        use super::debug_view::DebugView;
        self.debug_transport_bar(ui);
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.spacing_mut().item_spacing.x = 2.0;
            for (view, label) in [
                (DebugView::Code, "code"),
                (DebugView::Disasm, "disasm"),
                (DebugView::Memory, "memory"),
                (DebugView::Stack, "stack"),
            ] {
                if chrome::view_tab(ui, label, self.debug_view == view).clicked() {
                    self.debug_view = view;
                }
            }
        });
        chrome::hairline(ui);
        match self.debug_view {
            DebugView::Code => self.show_code_follow(ui),
            DebugView::Disasm => self.show_disasm(ui),
            DebugView::Memory => self.show_memory(ui),
            DebugView::Stack => self.show_stack(ui),
        }
    }

    // Mirrors the F5/F6/F10/F11 shortcuts.
    fn debug_transport_bar(&mut self, ui: &mut egui::Ui) {
        use crate::ui::theme::icons;
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let paused = live.state == MachineState::Paused;
        let alive = live.state.is_alive();
        let now = ui.input(|i| i.time);
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        let mut act: Option<DebugAction> = None;
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.spacing_mut().item_spacing.x = 4.0;
            if alive {
                let (glyph, tip) = if paused {
                    (icons::RUN, "Resume (F5)")
                } else {
                    (icons::PAUSE, "Pause (F6)")
                };
                if icon_button(ui, glyph, tip, true, false).clicked() {
                    act = Some(DebugAction::PauseResume);
                }
            }
            let dim = !paused;
            let step_tip = match self.step_unit {
                super::machine_dock::StepUnit::Line => "Step over source line (F10)",
                super::machine_dock::StepUnit::Instruction => "Step one instruction (F10)",
                super::machine_dock::StepUnit::Cycle => "Step one cycle (F10)",
            };
            if icon_button(ui, icons::STEP_OVER, step_tip, false, dim).clicked() && paused {
                act = Some(DebugAction::StepOver);
            }
            if icon_button(ui, icons::STEP_IN, "Step into (F11)", false, dim).clicked() && paused {
                act = Some(DebugAction::StepIn);
            }
            if icon_button(ui, icons::STEP_OUT, "Step out (Shift+F11)", false, dim).clicked()
                && paused
            {
                act = Some(DebugAction::StepOut);
            }
            ui.add_space(metrics::SECTION_GAP);
            if icon_button(ui, icons::RESTART, "Restart (Ctrl+F5)", false, false).clicked() {
                act = Some(DebugAction::Restart);
            }
            if icon_button(ui, icons::STOP, "Stop (Shift+F5)", false, !alive).clicked() && alive {
                act = Some(DebugAction::Stop);
            }
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        match act {
            Some(DebugAction::PauseResume) => {
                if let Some(live) = &mut self.live {
                    if live.state == MachineState::Paused {
                        live.resume();
                    } else {
                        live.pause();
                    }
                }
            }
            Some(DebugAction::StepOver) => self.step_selected_live_machine(),
            Some(DebugAction::StepIn) => self.step_into_selected_live_machine(),
            Some(DebugAction::StepOut) => {
                if let Some(live) = &mut self.live {
                    live.step_out();
                }
            }
            Some(DebugAction::Restart) => self.restart_selected_config(now),
            Some(DebugAction::Stop) => self.stop_live_machine(),
            None => {}
        }
    }

    // Holds the last known location over gaps in the pc-to-line info.
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
                    "read-only \u{00B7} gutter click: breakpoint \u{00B7} ctrl+click: run to",
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
        // Stable salt: the pane keeps one scroll state while the pc hops files;
        // the jump logic below repositions it on every followed move.
        let output = code_view(
            ui,
            CodeViewConfig {
                id_salt: "code_follow",
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
        if let Some(line) = output.run_to {
            self.run_to_source_line(&file, line);
        }
        if moved && output.jump_landed {
            self.code_follow_scrolled = Some((file, line));
        }
    }

    // Lookup order: open buffer, workspace copy, bundled program and kernel/stdlib sources.
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

    // The display stays inside the IDE so the terminal and framebuffer remain visible together.
    pub(super) fn framebuffer_window(&mut self, ctx: &egui::Context) {
        let show = self.live.as_ref().is_some_and(|live| live.state.is_alive())
            && self.config_has_screen();
        if !show {
            return;
        }
        let title = format!(
            "{} \u{00B7} screen",
            self.session_tab_label.as_deref().unwrap_or("machine")
        );
        let close_requested = {
            let mut open = true;
            let min_size = vec2(
                FB_WIDTH as f32 * FB_MIN_SCALE,
                FB_HEIGHT as f32 * FB_MIN_SCALE,
            );
            let max_size = (ctx.content_rect().size() - vec2(32.0, 64.0)).max(min_size);
            egui::Window::new(title)
                .id(egui::Id::new("machine_framebuffer_window"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .default_size([
                    FB_WIDTH as f32 * FB_DEFAULT_SCALE,
                    FB_HEIGHT as f32 * FB_DEFAULT_SCALE,
                ])
                .min_size(min_size)
                .max_size(max_size)
                .resizable(true)
                .collapsible(false)
                .constrain_to(ctx.content_rect())
                .open(&mut open)
                .frame(
                    egui::Frame::new()
                        .fill(theme::CHROME)
                        .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
                        .inner_margin(egui::Margin::ZERO),
                )
                .show(ctx, |ui| self.framebuffer_window_body(ui));
            !open
        };

        if close_requested {
            self.stop_live_machine();
        }
    }

    fn framebuffer_window_body(&mut self, ui: &mut egui::Ui) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        // Key the texture on the content version, not FILL: demos that draw
        // pixels directly (mandelbrot) never touch FILL but must still refresh.
        let version = live.framebuffer_version();
        let pixels = live.framebuffer().to_vec();
        self.upload_framebuffer(ui.ctx(), version, &pixels);
        let Some(texture) = self.machine_texture.as_ref() else {
            return;
        };
        let available = ui.available_size_before_wrap().max(vec2(0.0, 0.0)).floor();
        let (rect, response) = ui.allocate_exact_size(available, Sense::click());
        if !rect.is_positive() {
            return;
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "machine framebuffer")
        });
        if response.clicked() {
            response.request_focus();
        }
        let focused = response.has_focus() || response.clicked();
        if focused {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
        }

        let size = fit_framebuffer_size(rect.size());
        let image_rect = egui::Rect::from_center_size(rect.center(), size.floor());
        ui.painter().rect_filled(rect, 0.0, theme::BEZEL);
        ui.painter().rect_filled(image_rect, 0.0, theme::BEZEL);
        ui.put(
            image_rect,
            egui::Image::new((texture.id(), image_rect.size()))
                .texture_options(egui::TextureOptions::NEAREST),
        );
        if version == 0 {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "waiting for framebuffer",
                egui::FontId::monospace(metrics::STATUS_TEXT_SIZE),
                theme::TEXT_FAINT,
            );
        }
        if focused {
            ui.painter().rect_stroke(
                image_rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, theme::ELECTRIC),
                egui::StrokeKind::Inside,
            );
            self.route_framebuffer_keys(ui);
        }
        ui.ctx().request_repaint();
    }

    fn upload_framebuffer(&mut self, ctx: &egui::Context, key: u64, pixels: &[u8]) {
        if self.machine_texture.is_some() && self.machine_texture_fill_count == Some(key) {
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
        self.machine_texture_fill_count = Some(key);
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

fn fit_framebuffer_size(available: egui::Vec2) -> egui::Vec2 {
    if available.x <= 0.0 || available.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    let aspect = FB_WIDTH as f32 / FB_HEIGHT as f32;
    if available.x / available.y > aspect {
        vec2(available.y * aspect, available.y)
    } else {
        vec2(available.x, available.x / aspect)
    }
}

#[derive(Clone, Copy)]
enum DebugAction {
    PauseResume,
    StepOver,
    StepIn,
    StepOut,
    Restart,
    Stop,
}

fn icon_button(
    ui: &mut egui::Ui,
    glyph: &str,
    tip: &str,
    primary: bool,
    dim: bool,
) -> egui::Response {
    let size = egui::vec2(26.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let hovered = resp.hovered() && !dim;
    let color = if dim {
        theme::TEXT_FAINT
    } else if primary {
        theme::ELECTRIC
    } else if hovered {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    let painter = ui.painter_at(rect);
    if hovered {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(15.0),
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !dim, tip));
    if dim {
        resp.on_hover_text(format!("{tip} \u{00B7} pause to use"))
    } else {
        resp.on_hover_text(tip)
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
    use egui_kittest::{Harness, kittest::Queryable as _};

    #[test]
    fn enclosing_function_finds_the_header_above_the_caret() {
        let src = "PI: i32 = 3\n\
                   main: () -> i32 {\n    return helper(1)\n}\n\
                   helper<T>: (x: T) -> T {\n    return x\n}\n";
        assert_eq!(enclosing_function(src, 3).as_deref(), Some("main"));
        assert_eq!(enclosing_function(src, 6).as_deref(), Some("helper"));
        assert_eq!(enclosing_function(src, 1), None, "consts are not functions");
        assert_eq!(enclosing_function("", 5), None);
    }

    #[test]
    fn locate_in_stage_finds_ir_defines_and_asm_labels() {
        let ir = "export define main() -> i32 {\nentry:\n}\n\ndefine helper($x: i64) -> i64 {\n";
        assert_eq!(locate_in_stage(ir, Layer::Ir, "main"), Some(0));
        let helper = locate_in_stage(ir, Layer::Ir, "helper").expect("helper found");
        assert!(ir[helper..].starts_with("define helper"));
        assert_eq!(locate_in_stage(ir, Layer::Ir, "absent"), None);

        let asm = ".globl main\nmain:\n    addi sp, sp, -16\nhelper:\n    ret\n";
        let main = locate_in_stage(asm, Layer::Asm, "main").expect("label found");
        assert!(asm[main..].starts_with("main:"));
        assert_eq!(locate_in_stage(asm, Layer::Asm, "elper"), None);
    }

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
    fn framebuffer_fit_preserves_aspect_inside_any_window() {
        for available in [vec2(1000.0, 600.0), vec2(800.0, 1000.0), vec2(240.0, 180.0)] {
            let fitted = fit_framebuffer_size(available);
            assert!(fitted.x <= available.x);
            assert!(fitted.y <= available.y);
            assert!((fitted.x / fitted.y - 4.0 / 3.0).abs() < f32::EPSILON);
        }
        assert_eq!(fit_framebuffer_size(vec2(0.0, 180.0)), egui::Vec2::ZERO);
    }

    #[test]
    fn ctrl_key_mapping_mirrors_uart_control_bytes() {
        assert_eq!(ctrl_scancode(egui::Key::A), Some(0x01));
        assert_eq!(ctrl_scancode(egui::Key::C), Some(0x03));
        assert_eq!(ctrl_scancode(egui::Key::Z), Some(0x1A));
        assert_eq!(ctrl_scancode(egui::Key::ArrowUp), None);
    }

    // Key alone, no Text event: that is what the web backend sends for Tab, and
    // what a TextEdit without lock_focus filters away before it can indent.
    #[test]
    fn tab_indents_the_code_editor_rather_than_moving_focus() {
        let mut harness = code_tab_harness("ret\n");
        harness
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        harness.run();
        for _ in 0..4 {
            harness.key_press(egui::Key::ArrowLeft); // cursor starts at the end
        }
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();

        assert_eq!(
            harness.state(),
            "    ret\n",
            "Tab should indent with spaces, not move focus: {:?}",
            harness.state()
        );
        assert!(
            !harness.state().contains('\t'),
            "Tab must insert spaces, not tab characters: {:?}",
            harness.state()
        );
    }

    #[test]
    fn tab_aligns_the_cursor_to_the_next_multiple_of_four() {
        let mut harness = code_tab_harness(" ret\n");
        harness
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        harness.run();
        for _ in 0..4 {
            harness.key_press(egui::Key::ArrowLeft); // cursor after the space
        }
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();

        assert_eq!(
            harness.state(),
            "    ret\n",
            "Tab should pad only up to the next multiple of four: {:?}",
            harness.state()
        );
    }

    #[test]
    fn shift_tab_dedents_to_the_previous_multiple_of_four() {
        let mut harness = code_tab_harness("     ret\n"); // five leading spaces
        harness
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        harness.run();
        for _ in 0..9 {
            harness.key_press(egui::Key::ArrowLeft); // cursor at the line start
        }
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
        harness.run();

        assert_eq!(
            harness.state(),
            "    ret\n",
            "Shift+Tab should drop five spaces back to four: {:?}",
            harness.state()
        );
    }

    #[test]
    fn tab_and_shift_tab_apply_to_every_line_in_a_selection() {
        let mut harness = code_tab_harness("a\nb\nc\n");
        harness
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        harness.run();
        // Select the last two lines, then indent and dedent them as a block.
        for _ in 0..4 {
            harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
        }
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert_eq!(
            harness.state(),
            "a\n    b\n    c\n",
            "Tab should indent every selected line: {:?}",
            harness.state()
        );
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
        harness.run();
        assert_eq!(
            harness.state(),
            "a\nb\nc\n",
            "Shift+Tab should dedent every selected line: {:?}",
            harness.state()
        );
    }

    #[test]
    fn backspace_removes_one_space_not_a_tab_stop() {
        let mut harness = code_tab_harness("    ret\n");
        harness
            .get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        harness.run();
        for _ in 0..4 {
            harness.key_press(egui::Key::ArrowLeft); // cursor after the spaces
        }
        harness.run();
        harness.key_press(egui::Key::Backspace);
        harness.run();

        assert_eq!(
            harness.state(),
            "   ret\n",
            "Backspace should delete a single space: {:?}",
            harness.state()
        );
    }

    fn code_tab_harness(text: &str) -> Harness<'static, String> {
        Harness::new_ui_state(
            |ui, state: &mut String| {
                let text = state.clone();
                let out = code_view(
                    ui,
                    CodeViewConfig {
                        id_salt: "tab_indent",
                        text: &text,
                        highlighter: None,
                        interactive: true,
                        show_current_line: false,
                        find: None,
                        marks: &[],
                        jump_to: None,
                        debug: &SourceDebug::default(),
                    },
                );
                if let Some(edited) = out.edited {
                    *state = edited;
                }
            },
            text.to_owned(),
        )
    }
}

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
    jump_to: Option<usize>,
) -> CodeViewOutput {
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
            jump_to,
            debug: &SourceDebug::default(),
        },
    )
}

fn enclosing_function(text: &str, line: usize) -> Option<String> {
    text.lines()
        .take(line.max(1))
        .filter_map(fn_def_name)
        .last()
        .map(str::to_owned)
}

// An unindented `name: (` or `name<..>: (`; excludes struct/const/impl declarations.
fn fn_def_name(line: &str) -> Option<&str> {
    let name_end = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if name_end == 0 || line.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let name = &line[..name_end];
    let mut rest = line[name_end..].trim_start();
    if let Some(generics) = rest.strip_prefix('<') {
        rest = generics[generics.find('>')?..][1..].trim_start();
    }
    rest.strip_prefix(':')
        .is_some_and(|sig| sig.trim_start().starts_with('('))
        .then_some(name)
}

// IR locates the `define name(..)` header; assembly locates the `name:` label.
fn locate_in_stage(text: &str, layer: Layer, name: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let hit = match layer {
            Layer::Ir => {
                let t = line.trim_start();
                let t = t.strip_prefix("export ").unwrap_or(t);
                t.strip_prefix("define ")
                    .is_some_and(|signature| signature.starts_with(&format!("{name}(")))
            }
            Layer::Asm => line
                .strip_prefix(name)
                .is_some_and(|rest| rest.trim_end() == ":"),
            Layer::Hll | Layer::Elf => false,
        };
        if hit {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

#[derive(Default)]
struct CodeViewOutput {
    edited: Option<String>,
    cursor: Option<(usize, usize)>,
    bp_toggle: Option<usize>,
    run_to: Option<usize>,
    // True once the jump target is inside the viewport, so callers re-jump until it lands.
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
                match gutter(ui, line_count, &font, marks, row_height, &debug.breakpoints) {
                    Some((line, true)) => out.run_to = Some(line),
                    Some((line, false)) => out.bp_toggle = Some(line),
                    None => {}
                }
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
                if interactive {
                    handle_code_tab(ui, ("code_edit", id_salt.clone()), &mut buf, &mut out);
                }
                let output = egui::TextEdit::multiline(&mut buf)
                    .id_salt(("code_edit", id_salt.clone()))
                    .font(font.clone())
                    // A TextEdit drops Key::Tab by default (it is egui's focus key), so
                    // without this Tab never reaches the branch that indents.
                    .lock_focus(true)
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

// Tab indents with real spaces aligned to a multiple of four (and Shift+Tab
// dedents to the previous multiple) before the TextEdit can insert a literal
// tab or remove a whole tab stop. Shift+Tab must be consumed first: egui's
// logical modifier matching lets a plain Tab pattern also match a held shift.
fn handle_code_tab<Id>(ui: &egui::Ui, id_salt: Id, buf: &mut String, out: &mut CodeViewOutput)
where
    Id: std::hash::Hash + Clone,
{
    // The TextEdit derives its id from `.id_salt(x)` as `Id::new(x)` hashed
    // into the parent Ui, so mirror that exactly or focus/state lookups miss.
    let edit_id = ui.make_persistent_id(egui::Id::new(id_salt));
    if ui.ctx().memory(|m| m.focused() != Some(edit_id)) {
        return;
    }
    let outdent = ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
    let indent = !outdent && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    if !indent && !outdent {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit_id) else {
        return;
    };
    let (primary, secondary) = match state.cursor.char_range() {
        Some(range) => (range.primary.index, range.secondary.index),
        None => (0, 0),
    };
    let (primary, secondary) = apply_tab_edit(buf, primary, secondary, outdent);
    state.cursor.set_char_range(Some(CCursorRange {
        primary: CCursor::new(primary),
        secondary: CCursor::new(secondary),
        h_pos: None,
    }));
    egui::TextEdit::store_state(ui.ctx(), edit_id, state);
    out.edited = Some(buf.clone());
    out.cursor = Some(line_col(buf, primary));
}

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

// Painted from coords stashed last frame, so this frame's text lands on top.
fn paint_debug_band(ui: &egui::Ui, dbg_id: egui::Id) {
    let Some(band) = ui.data(|d| d.get_temp::<[f32; 4]>(dbg_id)) else {
        return;
    };
    let rect = egui::Rect::from_min_max(pos2(band[2], band[0]), pos2(band[3], band[1]));
    ui.painter().rect_filled(rect, 0.0, theme::SURFACE);
    let bar = egui::Rect::from_min_max(rect.left_top(), pos2(band[2] + 2.0, band[1]));
    ui.painter().rect_filled(bar, 0.0, theme::ELECTRIC);
}

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

const TAB_WIDTH: usize = 4;

// Tab inserts real spaces aligned to the next multiple of TAB_WIDTH, and
// Shift+Tab removes leading whitespace back to the previous multiple of four.
// Takes cursor positions in chars and returns the new (primary, secondary)
// cursor after the edit.
fn apply_tab_edit(
    buf: &mut String,
    primary: usize,
    secondary: usize,
    outdent: bool,
) -> (usize, usize) {
    if primary == secondary {
        let at = primary;
        if !outdent {
            let column = at - line_start_char(buf, at);
            let spaces = TAB_WIDTH - (column % TAB_WIDTH);
            buf.insert_str(char_to_byte(buf, at), &" ".repeat(spaces));
            (at + spaces, at + spaces)
        } else {
            let line_start = line_start_char(buf, at);
            let removed = leading_outdent_len(buf, line_start);
            if removed == 0 {
                (at, at)
            } else {
                let from = char_to_byte(buf, line_start);
                let to = char_to_byte(buf, line_start + removed);
                buf.replace_range(from..to, "");
                let new_at = if at >= line_start + removed {
                    at - removed
                } else {
                    line_start
                };
                (new_at, new_at)
            }
        }
    } else {
        let (min, max) = if primary < secondary {
            (primary, secondary)
        } else {
            (secondary, primary)
        };
        let starts = line_starts_in(buf, min, max);
        if !outdent {
            let n = starts.len();
            for &line_start in starts.iter().rev() {
                buf.insert_str(char_to_byte(buf, line_start), "    ");
            }
            let new_min = min + TAB_WIDTH;
            let new_max = max + TAB_WIDTH * n;
            if primary < secondary {
                (new_min, new_max)
            } else {
                (new_max, new_min)
            }
        } else {
            let removals: Vec<(usize, usize)> = starts
                .iter()
                .map(|&line_start| (line_start, leading_outdent_len(buf, line_start)))
                .filter(|&(_, removed)| removed > 0)
                .collect();
            for &(line_start, removed) in removals.iter().rev() {
                let from = char_to_byte(buf, line_start);
                let to = char_to_byte(buf, line_start + removed);
                buf.replace_range(from..to, "");
            }
            let (mut new_min, mut new_max) = (min, max);
            for &(line_start, removed) in removals.iter().rev() {
                new_min = remap_after_removal(new_min, line_start, removed);
                new_max = remap_after_removal(new_max, line_start, removed);
            }
            if primary < secondary {
                (new_min, new_max)
            } else {
                (new_max, new_min)
            }
        }
    }
}

fn remap_after_removal(edge: usize, at: usize, removed: usize) -> usize {
    if edge >= at + removed {
        edge - removed
    } else if edge > at {
        at
    } else {
        edge
    }
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn line_start_char(text: &str, char_idx: usize) -> usize {
    let byte = char_to_byte(text, char_idx);
    text[..byte]
        .rfind('\n')
        .map(|newline| text[..newline].chars().count() + 1)
        .unwrap_or(0)
}

// Char index of the first newline at or after `from`.
fn next_newline_char(text: &str, from: usize) -> Option<usize> {
    text.char_indices()
        .skip(from)
        .find(|(_, c)| *c == '\n')
        .map(|(byte, _)| text[..byte].chars().count())
}

// Starts (in chars) of the lines a selection [min, max) touches: the line
// holding `min`, plus every later line whose start lies inside the selection.
fn line_starts_in(text: &str, min: usize, max: usize) -> Vec<usize> {
    let limit = if max > min && text.chars().nth(max - 1) == Some('\n') {
        max - 1
    } else {
        max
    };
    let mut starts = vec![line_start_char(text, min)];
    let mut newline = next_newline_char(text, starts[0]);
    while let Some(nl) = newline {
        if nl >= limit {
            break;
        }
        starts.push(nl + 1);
        newline = next_newline_char(text, nl + 1);
    }
    starts
}

// Leading whitespace a Shift+Tab removes from a line: one tab char, or the
// leading spaces trimmed to the previous multiple of four (a full stop when
// the line is already aligned).
fn leading_outdent_len(text: &str, line_start: usize) -> usize {
    let mut rest = text[char_to_byte(text, line_start)..].chars();
    match rest.next() {
        Some('\t') => 1,
        Some(' ') => {
            let spaces = 1 + rest.take_while(|c| *c == ' ').count();
            let rem = spaces % TAB_WIDTH;
            if rem == 0 { TAB_WIDTH.min(spaces) } else { rem }
        }
        _ => 0,
    }
}

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

fn gutter(
    ui: &mut egui::Ui,
    line_count: usize,
    font: &FontId,
    marks: &[DiagMark],
    row_height: f32,
    breakpoints: &[usize],
) -> Option<(usize, bool)> {
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
    let dot_y = |line: usize| rect.top() + (line as f32 - 0.5) * pitch;
    for &line in breakpoints {
        if line >= 1 && line <= line_count {
            ui.painter()
                .circle_filled(pos2(dot_x, dot_y(line)), 3.0, theme::DANGER);
        }
    }
    let mut toggled = None;
    for line in 1..=line_count {
        let line_rect = egui::Rect::from_min_size(
            pos2(rect.left(), rect.top() + (line - 1) as f32 * pitch),
            vec2(rect.width(), pitch),
        );
        let resp = ui.interact(line_rect, ui.id().with(("gutter_bp", line)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("toggle breakpoint line {line}"),
            )
        });
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if !breakpoints.contains(&line) {
                ui.painter().circle_stroke(
                    pos2(dot_x, dot_y(line)),
                    3.0,
                    egui::Stroke::new(1.0, theme::TEXT_FAINT),
                );
            }
        }
        if resp.clicked() {
            toggled = Some((line, ui.input(|i| i.modifiers.command)));
        }
    }

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

// Ranges are validated against the live buffer, which may be newer than them.
fn valid_ranges(text: &str, ranges: impl Iterator<Item = (usize, usize)>) -> Vec<(usize, usize)> {
    ranges
        .filter(|&(start, end)| {
            end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end)
        })
        .collect()
}

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
