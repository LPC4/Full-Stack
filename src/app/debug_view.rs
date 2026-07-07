//! Instruction-level debug views on the machine rung: a disassembly window
//! around the live pc and a hex memory view with a VA/PA toggle.

use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::session::MachineState;
use full_stack::workspace::VPath;
use virtual_machine::cpu::decoder::{self, DecodedInsn};

/// Instructions shown before and after the anchor pc.
const DISASM_BACK: u64 = 24;
const DISASM_AHEAD: u64 = 40;
/// Bytes per memory-view row and rows per page.
const MEM_ROW_BYTES: usize = 16;
const MEM_ROWS: usize = 32;
/// Left gutter (breakpoint dot + pc arrow) width in the disassembly, in px.
const DISASM_GUTTER: f32 = 24.0;
const XREG_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// Which surface the machine (L4) rung is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineView {
    Console,
    Screen,
    Disasm,
    Memory,
}

pub(crate) struct MemoryViewState {
    pub(super) addr_text: String,
    addr: u64,
    physical: bool,
}

impl Default for MemoryViewState {
    fn default() -> Self {
        let addr = virtual_machine::bus::RAM_BASE;
        Self {
            addr_text: format!("{addr:x}"),
            addr,
            physical: false,
        }
    }
}

/// What a disassembly row asked the session to do this frame.
enum DisasmAction {
    Toggle(u64),
    RunTo(u64),
}

/// Character-grid metrics all debug rows share, so columns line up exactly.
struct Grid {
    font: FontId,
    char_w: f32,
}

impl Grid {
    fn new(ui: &egui::Ui) -> Self {
        let font = FontId::monospace(metrics::LABEL_TEXT_SIZE);
        let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
        Self { font, char_w }
    }

    /// X position of character column `col` from `left`.
    fn x(&self, left: f32, col: usize) -> f32 {
        left + col as f32 * self.char_w
    }
}

/// Addresses in this VM fit 8 hex digits; wider ones fall back to 12.
fn format_addr(addr: u64) -> String {
    if u32::try_from(addr).is_ok() {
        format!("{addr:08x}")
    } else {
        format!("{addr:012x}")
    }
}

impl FullStackApp {
    /// The disassembly window, anchored to the live pc each frame.
    pub(super) fn show_disasm(&mut self, ui: &mut egui::Ui) {
        let Some(live) = self.live.as_ref() else {
            chrome::centered_hint(ui, "No machine session. Press Run to start one.");
            return;
        };
        let regs = live.registers();
        let pc = regs.pc;
        let start = pc.saturating_sub(DISASM_BACK * 4) & !3;
        let count = (DISASM_BACK + DISASM_AHEAD) as usize;
        let bytes = live.read_virtual(start, count * 4);
        let paused = live.state == MachineState::Paused;
        // Resolve source markers up front: the render loop can then show real
        // source text without overlapping the live borrow with a workspace read.
        let markers = self.disasm_source_markers(start, count);
        let Some(live) = &mut self.live else { return };
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::label("disassembly"));
            if let Some((name, offset)) = live.symbol_near(pc) {
                let tag = if offset == 0 {
                    name.to_owned()
                } else {
                    format!("{name}+{offset:#x}")
                };
                ui.label(theme::mono(tag, metrics::STATUS_TEXT_SIZE, theme::TEXT_DIM));
            }
            if !paused {
                ui.label(theme::mono(
                    "pause to step",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(metrics::PANEL_INSET_X);
                ui.label(theme::mono(
                    "click: breakpoint \u{00B7} ctrl+click: run to",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            });
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        chrome::hairline(ui);

        let grid = Grid::new(ui);
        let mut action = None;
        // Keep the pc row in view whenever the machine moved since last frame.
        let follow_id = ui.make_persistent_id("disasm_follow_pc");
        let follow = ui.data(|d| d.get_temp::<u64>(follow_id)) != Some(pc);
        egui::ScrollArea::vertical()
            .id_salt("disasm_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in 0..count {
                    let addr = start + i as u64 * 4;
                    let word =
                        u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap_or([0u8; 4]));
                    if let Some(name) = live.symbol_at(addr) {
                        symbol_label_row(ui, name, i > 0);
                    }
                    if let Some((file, line, text)) = &markers[i] {
                        source_loc_row(ui, file, *line, text.as_deref());
                    }
                    let has_bp = live.breakpoints.contains(&addr);
                    let target = jump_target(addr, word, &regs.x);
                    let target_label = target
                        .map(|target| format_target_operand(target, live.symbol_near(target)));
                    let (rect, row_action) = disasm_row(
                        ui,
                        &grid,
                        addr,
                        word,
                        target_label.as_deref(),
                        addr == pc,
                        has_bp,
                    );
                    if row_action.is_some() {
                        action = row_action;
                    }
                    if addr == pc && follow {
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                }
            });
        ui.data_mut(|d| d.insert_temp(follow_id, pc));

        let mut persist_breakpoints = false;
        match action {
            Some(DisasmAction::Toggle(addr)) => {
                if !live.breakpoints.remove(&addr) {
                    live.breakpoints.insert(addr);
                }
                persist_breakpoints = true;
            }
            Some(DisasmAction::RunTo(addr)) => live.run_to(addr),
            None => {}
        }
        if persist_breakpoints {
            self.persist_live_breakpoints();
        }
    }

    /// Per-instruction source markers for the disasm window: `Some((file, line,
    /// text))` at each address where the source line changes, else `None`.
    fn disasm_source_markers(
        &self,
        start: u64,
        count: usize,
    ) -> Vec<Option<(String, u32, Option<String>)>> {
        let mut locs: Vec<Option<(String, u32)>> = Vec::with_capacity(count);
        if let Some(live) = self.live.as_ref() {
            let mut last: Option<(String, u32)> = None;
            for i in 0..count {
                let addr = start + i as u64 * 4;
                let loc = live
                    .source_for_pc(addr)
                    .map(|(file, line)| (file.to_owned(), line));
                if loc.is_some() && loc != last {
                    locs.push(loc.clone());
                    last = loc;
                } else {
                    locs.push(None);
                }
            }
        } else {
            locs.resize(count, None);
        }
        // Resolve text after the live borrow ends so the workspace read is free.
        locs.into_iter()
            .map(|loc| {
                loc.map(|(file, line)| {
                    let text = self.source_line_text(&file, line);
                    (file, line, text)
                })
            })
            .collect()
    }

    /// Trimmed text of 1-based `line` in workspace `file`, preferring the open
    /// (possibly unsaved) buffer over the on-disk copy.
    fn source_line_text(&self, file: &str, line: u32) -> Option<String> {
        let path = VPath::parse(file).ok()?;
        let text = match self.workspace.document(&path) {
            Some(doc) if doc.is_binary() => return None,
            Some(doc) => doc.text().to_owned(),
            None => String::from_utf8(self.workspace.fs().read_file(&path).ok()?).ok()?,
        };
        let idx = (line as usize).checked_sub(1)?;
        Some(text.lines().nth(idx)?.trim().to_owned())
    }

    /// Hex dump at an address, read through the MMU or straight from RAM.
    pub(super) fn show_memory(&mut self, ui: &mut egui::Ui) {
        let Some(live) = &mut self.live else {
            chrome::centered_hint(ui, "No machine session. Press Run to start one.");
            return;
        };
        let regs = live.registers();

        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::label("memory"));
            ui.label(theme::mono(
                "0x",
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.memory_view.addr_text)
                    .id_salt("memory_addr")
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme::TEXT)
                    .desired_width(110.0),
            );
            if edit.changed()
                && let Ok(addr) = u64::from_str_radix(self.memory_view.addr_text.trim(), 16)
            {
                self.memory_view.addr = addr;
            }
            if let Some(physical) = address_space_toggle(ui, self.memory_view.physical) {
                self.memory_view.physical = physical;
            }
            if small_toggle(ui, "pc")
                .on_hover_text("Jump to the program counter")
                .clicked()
            {
                self.memory_view.set_addr(regs.pc);
            }
            if small_toggle(ui, "sp")
                .on_hover_text("Jump to the stack pointer")
                .clicked()
            {
                self.memory_view.set_addr(regs.x[2]);
            }
            if !self.memory_view.physical {
                match live.translate(self.memory_view.addr) {
                    Some(pa) => ui.label(theme::mono(
                        format!("\u{2192} pa {pa:#x}"),
                        metrics::STATUS_TEXT_SIZE,
                        theme::TEXT_FAINT,
                    )),
                    None => ui.label(theme::mono(
                        "\u{2192} unmapped",
                        metrics::STATUS_TEXT_SIZE,
                        theme::WARNING,
                    )),
                };
            }
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y);
        chrome::hairline(ui);

        let base = self.memory_view.addr & !0xF;
        let len = MEM_ROW_BYTES * MEM_ROWS;
        let bytes = if self.memory_view.physical {
            live.read_physical(base, len)
        } else {
            live.read_virtual(base, len)
        };
        let grid = Grid::new(ui);
        let min_width = grid.x(0.0, ascii_col() + MEM_ROW_BYTES) + 2.0 * metrics::PANEL_INSET_X;
        egui::ScrollArea::both()
            .id_salt("memory_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.set_min_width(min_width);
                hex_header_row(ui, &grid);
                for row in 0..MEM_ROWS {
                    hex_row(
                        ui,
                        &grid,
                        base + (row * MEM_ROW_BYTES) as u64,
                        &bytes[row * MEM_ROW_BYTES..(row + 1) * MEM_ROW_BYTES],
                        row % 2 == 1,
                    );
                }
            });
    }
}

impl MemoryViewState {
    fn set_addr(&mut self, addr: u64) {
        self.addr = addr;
        self.addr_text = format!("{addr:x}");
    }
}

/// One disassembly row: gutter (breakpoint dot, pc arrow), then address, raw
/// word, mnemonic, and operands on the shared character grid.
/// Returns the row rect (for follow-pc scrolling) and any requested action.
fn disasm_row(
    ui: &mut egui::Ui,
    grid: &Grid,
    addr: u64,
    word: u32,
    target_label: Option<&str>,
    is_pc: bool,
    has_bp: bool,
) -> (egui::Rect, Option<DisasmAction>) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if is_pc {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let mid_y = rect.center().y;
    let dot = pos2(rect.left() + metrics::PANEL_INSET_X + 4.0, mid_y);
    if has_bp {
        painter.circle_filled(dot, 3.0, theme::DANGER);
    } else if resp.hovered() {
        // Hollow dot hints that a click sets a breakpoint here.
        painter.circle_stroke(dot, 3.0, egui::Stroke::new(1.0, theme::TEXT_FAINT));
    }
    let left = rect.left() + metrics::PANEL_INSET_X + DISASM_GUTTER;
    if is_pc {
        painter.text(
            pos2(left - 12.0, mid_y),
            Align2::LEFT_CENTER,
            ">",
            grid.font.clone(),
            theme::ELECTRIC,
        );
    }

    let addr_color = if is_pc {
        theme::ELECTRIC
    } else {
        theme::TEXT_DIM
    };
    painter.text(
        pos2(grid.x(left, 0), mid_y),
        Align2::LEFT_CENTER,
        format_addr(addr),
        grid.font.clone(),
        addr_color,
    );
    painter.text(
        pos2(grid.x(left, 11), mid_y),
        Align2::LEFT_CENTER,
        format!("{word:08x}"),
        grid.font.clone(),
        theme::TEXT_FAINT,
    );
    let decoded = match decoder::decode(word) {
        Ok(insn) => disasm_text(&insn, addr, target_label),
        Err(_) if word == 0 => String::new(),
        Err(_) => format!(".word {word:#010x}"),
    };
    let mut parts = decoded.split_whitespace();
    let mnemonic = parts.next().unwrap_or_default();
    let operands = parts.collect::<Vec<_>>().join(" ");
    painter.text(
        pos2(grid.x(left, 22), mid_y),
        Align2::LEFT_CENTER,
        mnemonic,
        grid.font.clone(),
        if is_pc { theme::TEXT } else { theme::TEXT_DIM },
    );
    painter.text(
        pos2(grid.x(left, 30), mid_y),
        Align2::LEFT_CENTER,
        operands,
        grid.font.clone(),
        theme::TEXT_DIM,
    );

    let action = if resp.clicked() {
        if ui.input(|i| i.modifiers.command) {
            Some(DisasmAction::RunTo(addr))
        } else {
            Some(DisasmAction::Toggle(addr))
        }
    } else {
        None
    };
    (rect, action)
}

fn jump_target(addr: u64, word: u32, regs: &[u64; 32]) -> Option<u64> {
    match decoder::decode(word).ok()? {
        DecodedInsn::Jal { imm, .. } | DecodedInsn::Branch { imm, .. } => {
            Some(addr.wrapping_add(imm as u64))
        }
        DecodedInsn::Jalr { rs1, imm, .. } => Some(regs.get(rs1)?.wrapping_add(imm as u64) & !1u64),
        _ => None,
    }
}

fn disasm_text(insn: &DecodedInsn, addr: u64, target_label: Option<&str>) -> String {
    match insn {
        DecodedInsn::Jal { rd, imm } => {
            let target = target_label
                .map(str::to_owned)
                .unwrap_or_else(|| format_target_operand(addr.wrapping_add(*imm as u64), None));
            format!("{:<10} {}, {}", "jal", xreg_name(*rd), target)
        }
        DecodedInsn::Jalr { rd, rs1, imm } => {
            if let Some(target) = target_label {
                format!(
                    "{:<10} {}, {} ; {}({})",
                    "jalr",
                    xreg_name(*rd),
                    target,
                    imm,
                    xreg_name(*rs1)
                )
            } else {
                insn.to_string()
            }
        }
        DecodedInsn::Branch { rs1, rs2, imm, .. } => {
            let target = target_label
                .map(str::to_owned)
                .unwrap_or_else(|| format_target_operand(addr.wrapping_add(*imm as u64), None));
            format!(
                "{:<10} {}, {}, {}",
                insn.mnemonic(),
                xreg_name(*rs1),
                xreg_name(*rs2),
                target
            )
        }
        _ => insn.to_string(),
    }
}

fn format_target_operand(target: u64, symbol: Option<(&str, u64)>) -> String {
    let mut text = format!("{target:#x}");
    if let Some((name, offset)) = symbol {
        let label = if offset == 0 {
            name.to_owned()
        } else {
            format!("{name}+{offset:#x}")
        };
        text.push_str(&format!(" <{label}>"));
    }
    text
}

fn xreg_name(reg: usize) -> &'static str {
    XREG_NAMES.get(reg).copied().unwrap_or("??")
}

/// A faint source-line marker where the pc-to-source line changes. Shows the
/// real source text when available, falling back to `; file:line`.
fn source_loc_row(ui: &mut egui::Ui, file: &str, line: u32, text: Option<&str>) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let label = match text {
        Some(src) if !src.is_empty() => format!("{line:>4}  {src}"),
        _ => format!("; {file}:{line}"),
    };
    ui.painter_at(rect).text(
        pos2(
            rect.left() + metrics::PANEL_INSET_X + DISASM_GUTTER,
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        label,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
}

fn symbol_label_row(ui: &mut egui::Ui, name: &str, gap_above: bool) {
    if gap_above {
        ui.add_space(metrics::SECTION_GAP);
    }
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let text_rect = painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{name}:"),
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::ELECTRIC,
    );
    painter.line_segment(
        [
            pos2(text_rect.right() + 10.0, rect.center().y),
            pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        ],
        egui::Stroke::new(1.0, theme::HAIRLINE_SOFT),
    );
}

/// Character column where the ascii gutter starts (after 16 spaced hex bytes).
fn ascii_col() -> usize {
    // addr(8) + gap(3) + 16 bytes at 3 chars + mid gap(1) + gap(2)
    8 + 3 + MEM_ROW_BYTES * 3 + 1 + 2
}

/// Byte-offset header over the hex grid.
fn hex_header_row(ui: &mut egui::Ui, grid: &Grid) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let left = rect.left() + metrics::PANEL_INSET_X;
    for i in 0..MEM_ROW_BYTES {
        painter.text(
            pos2(grid.x(left, byte_col(i)), rect.center().y),
            Align2::LEFT_CENTER,
            format!("{i:02x}"),
            grid.font.clone(),
            theme::TEXT_FAINT,
        );
    }
    painter.text(
        pos2(grid.x(left, ascii_col()), rect.center().y),
        Align2::LEFT_CENTER,
        "ascii",
        grid.font.clone(),
        theme::TEXT_FAINT,
    );
}

/// Character column of byte `i` in a hex row (extra gap after the 8th byte).
fn byte_col(i: usize) -> usize {
    8 + 3 + i * 3 + usize::from(i >= 8)
}

/// One hex-dump row: address, 16 bytes (zeros dimmed), ascii gutter.
fn hex_row(ui: &mut egui::Ui, grid: &Grid, addr: u64, bytes: &[u8], stripe: bool) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if stripe {
        painter.rect_filled(rect, 0.0, theme::WELL);
    }
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    let left = rect.left() + metrics::PANEL_INSET_X;
    let mid_y = rect.center().y;
    painter.text(
        pos2(grid.x(left, 0), mid_y),
        Align2::LEFT_CENTER,
        format_addr(addr),
        grid.font.clone(),
        theme::TEXT_DIM,
    );
    let mut ascii = String::with_capacity(bytes.len());
    for (i, byte) in bytes.iter().enumerate() {
        let color = if *byte == 0 {
            theme::TEXT_FAINT
        } else {
            theme::TEXT
        };
        painter.text(
            pos2(grid.x(left, byte_col(i)), mid_y),
            Align2::LEFT_CENTER,
            format!("{byte:02x}"),
            grid.font.clone(),
            color,
        );
        ascii.push(if byte.is_ascii_graphic() || *byte == b' ' {
            *byte as char
        } else {
            '.'
        });
    }
    painter.text(
        pos2(grid.x(left, ascii_col()), mid_y),
        Align2::LEFT_CENTER,
        ascii,
        grid.font.clone(),
        theme::TEXT_DIM,
    );
}

/// Two-segment va|pa switch: the active address space reads as pressed.
/// Returns the newly chosen space (physical?) when a segment is clicked.
fn address_space_toggle(ui: &mut egui::Ui, physical: bool) -> Option<bool> {
    const SEG_W: f32 = 26.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(SEG_W * 2.0, 18.0), Sense::click());
    let painter = ui.painter_at(rect);
    for (i, (label, active)) in [("va", !physical), ("pa", physical)].iter().enumerate() {
        let seg = egui::Rect::from_min_size(
            pos2(rect.left() + i as f32 * SEG_W, rect.top()),
            vec2(SEG_W, rect.height()),
        );
        if *active {
            painter.rect_filled(seg, 0.0, theme::SURFACE);
        }
        painter.text(
            seg.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            if *active {
                theme::ELECTRIC
            } else {
                theme::TEXT_FAINT
            },
        );
    }
    painter.line_segment(
        [
            pos2(rect.center().x, rect.top()),
            pos2(rect.center().x, rect.bottom()),
        ],
        egui::Stroke::new(1.0, theme::HAIRLINE),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, theme::HAIRLINE),
        egui::StrokeKind::Inside,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let clicked = resp
        .interact_pointer_pos()
        .filter(|_| resp.clicked())
        .map(|pos| pos.x >= rect.center().x);
    resp.on_hover_text("Address space: va reads through the MMU, pa reads raw RAM");
    clicked
}

/// A small framed toggle/action chip for the memory-view header.
fn small_toggle(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let resp = ui.add(egui::Button::new(theme::mono(
        label,
        metrics::STATUS_TEXT_SIZE,
        theme::TEXT,
    )));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use asm_to_binary::riscv::rv64i::{Beq, Jal};
    use asm_to_binary::traits::Instruction as _;

    #[test]
    fn disassembly_formats_jal_with_absolute_target() {
        let pc = 0x8000_1000;
        let word = Jal::new(1, 0x40).encode();
        let insn = decoder::decode(word).expect("decode jal");
        let regs = [0u64; 32];

        assert_eq!(jump_target(pc, word, &regs), Some(0x8000_1040));
        assert_eq!(disasm_text(&insn, pc, None), "jal        ra, 0x80001040");
    }

    #[test]
    fn disassembly_formats_branch_with_symbolic_target() {
        let pc = 0x8000_1000;
        let word = Beq::new(10, 0, -0x10).encode();
        let insn = decoder::decode(word).expect("decode branch");
        let regs = [0u64; 32];

        assert_eq!(jump_target(pc, word, &regs), Some(0x8000_0ff0));
        assert_eq!(
            disasm_text(&insn, pc, Some("0x80000ff0 <loop+0x8>")),
            "beq        a0, zero, 0x80000ff0 <loop+0x8>"
        );
    }

    #[test]
    fn disassembly_formats_jalr_with_computed_target() {
        use asm_to_binary::riscv::rv64i::Jalr;

        let pc = 0x8000_1000;
        let word = Jalr::new(1, 5, 6).encode();
        let insn = decoder::decode(word).expect("decode jalr");
        let mut regs = [0u64; 32];
        regs[5] = 0x8000_2003;

        assert_eq!(jump_target(pc, word, &regs), Some(0x8000_2008));
        assert_eq!(
            disasm_text(&insn, pc, Some("0x80002008 <callee>")),
            "jalr       ra, 0x80002008 <callee> ; 6(t0)"
        );
    }
}
