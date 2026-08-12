mod code_view;
mod framebuffer;
#[cfg(test)]
mod tests;

use super::diagnostics::DiagMark;
use super::find::FindHighlight;
use super::{FullStackApp, SourceKind, chrome, metrics};
use crate::ui::{Layer, theme};
pub(super) use code_view::console_view;
use code_view::{CodeViewConfig, code_view, enclosing_function, locate_in_stage, read_only_code};
use eframe::egui;
use egui::text::{CCursor, CCursorRange, LayoutJob, LayoutSection};
use egui::{FontId, Sense, TextStyle, pos2, vec2};
pub(super) use framebuffer::ctrl_scancode;
#[cfg(test)]
use framebuffer::{fit_framebuffer_size, key_scancode, printable_scancode, special_scancode};
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
                        .failure
                        .as_ref()
                        .map(ToString::to_string)
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
