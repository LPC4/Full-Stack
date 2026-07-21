//! Binary viewer for ELF executables and objects: decoded header, a colored
//! byte map of the file, section/symbol/relocation tables, and a hex dump.

use super::metrics;
use crate::ui::{Layer, theme};
use eframe::egui;
use egui::{Align2, Color32, FontId, Sense, pos2, vec2};
use full_stack::inspect::elf::{ElfInfo, SHT_NOBITS, parse_elf};

const MAP_HEIGHT: f32 = 22.0;
const HEX_MAX_BYTES: usize = 1024;
const HEX_COLS: usize = 16;

pub(super) fn show(ui: &mut egui::Ui, id_salt: &str, bytes: &[u8]) {
    match parse_elf(bytes) {
        Ok(info) => decoded_view(ui, id_salt, bytes, &info),
        Err(err) => fallback_view(ui, id_salt, bytes, &err),
    }
}

fn decoded_view(ui: &mut egui::Ui, id_salt: &str, bytes: &[u8], info: &ElfInfo) {
    let sel_id = egui::Id::new(("elf_selected", id_salt));
    let mut selected = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(sel_id))
        .unwrap_or_else(|| default_selection(info));

    egui::ScrollArea::vertical()
        .id_salt(("elf_scroll", id_salt))
        .show(ui, |ui| {
            overview(ui, info);
            file_map(ui, bytes.len(), info, &mut selected);
            section_table(ui, info, &mut selected);
            if !info.segments.is_empty() {
                segment_table(ui, info);
            }
            if !info.symbols.is_empty() {
                symbol_table(ui, info);
            }
            if !info.relocations.is_empty() {
                reloc_table(ui, info);
            }
            hex_view(ui, bytes, info, selected);
            ui.add_space(metrics::SECTION_GAP * 2.0);
        });

    ui.ctx().data_mut(|d| d.insert_temp(sel_id, selected));
}

fn fallback_view(ui: &mut egui::Ui, id_salt: &str, bytes: &[u8], err: &str) {
    egui::ScrollArea::vertical()
        .id_salt(("elf_scroll", id_salt))
        .show(ui, |ui| {
            eyebrow(ui, "binary file");
            inset_line(
                ui,
                &format!("not a readable ELF image: {err}"),
                theme::WARNING,
            );
            inset_line(ui, &format!("{} bytes", bytes.len()), theme::TEXT_DIM);
            eyebrow(ui, "hex");
            hex_dump(ui, bytes, 0, HEX_MAX_BYTES);
        });
}

fn default_selection(info: &ElfInfo) -> usize {
    info.sections
        .iter()
        .position(|s| s.type_name == "PROGBITS")
        .unwrap_or(0)
}

fn section_color(name: &str) -> Color32 {
    if name == ".text" {
        theme::ELECTRIC
    } else if name == ".data" {
        Layer::Hll.color()
    } else if name == ".rodata" {
        Layer::Ir.color()
    } else if name == ".bss" {
        Layer::Asm.color()
    } else if name.starts_with(".rela") {
        theme::WARNING
    } else if name.ends_with("strtab") || name == ".symtab" {
        theme::TEXT_FAINT
    } else {
        Layer::Elf.color()
    }
}

// --- Overview ---

fn overview(ui: &mut egui::Ui, info: &ElfInfo) {
    ui.add_space(metrics::SECTION_GAP);
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        let type_color = if info.is_exec {
            theme::ELECTRIC
        } else {
            Layer::Ir.color()
        };
        ui.label(theme::mono(
            info.type_name.clone(),
            metrics::LABEL_TEXT_SIZE,
            type_color,
        ));
        ui.label(theme::mono(
            format!(
                "ELF64 \u{00B7} little-endian \u{00B7} {}",
                info.machine_name
            ),
            metrics::LABEL_TEXT_SIZE,
            theme::TEXT,
        ));
    });
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        if info.is_exec {
            ui.label(theme::label("entry"));
            ui.label(theme::mono(
                format!("{:#x}", info.entry),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
        }
        ui.label(theme::label(&format!("{} bytes", info.file_len)));
        ui.label(theme::label(&format!("{} sections", info.sections.len())));
        ui.label(theme::label(&format!("{} symbols", info.symbols.len())));
        if !info.relocations.is_empty() {
            ui.label(theme::label(&format!(
                "{} relocations",
                info.relocations.len()
            )));
        }
    });
}

// --- File map ---

fn file_map(ui: &mut egui::Ui, file_len: usize, info: &ElfInfo, sel: &mut usize) {
    if file_len == 0 {
        return;
    }
    eyebrow(ui, "file map");
    let width = ui.available_width() - metrics::PANEL_INSET_X * 2.0;
    let (outer, resp) = ui.allocate_exact_size(
        vec2(width + metrics::PANEL_INSET_X * 2.0, MAP_HEIGHT),
        Sense::click(),
    );
    let rect = egui::Rect::from_min_size(
        pos2(outer.left() + metrics::PANEL_INSET_X, outer.top()),
        vec2(width, MAP_HEIGHT),
    );
    let painter = ui.painter_at(outer);
    painter.rect_filled(rect, 0.0, theme::WELL);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, theme::HAIRLINE_SOFT),
        egui::StrokeKind::Inside,
    );

    let span_of = |offset: u64, size: u64| {
        let x0 = rect.left() + (offset as f32 / file_len as f32) * rect.width();
        let x1 = rect.left() + ((offset + size) as f32 / file_len as f32) * rect.width();
        egui::Rect::from_min_max(
            pos2(x0, rect.top() + 2.0),
            pos2(x1.max(x0 + 2.0), rect.bottom() - 2.0),
        )
    };
    let hovered = resp.hover_pos().and_then(|pos| {
        mapped_sections(info, file_len)
            .find(|&(_, offset, size)| span_of(offset, size).expand2(vec2(1.0, 2.0)).contains(pos))
    });
    for (index, offset, size) in mapped_sections(info, file_len) {
        let color = section_color(&info.sections[index].name);
        let strong = index == *sel || hovered.is_some_and(|(h, _, _)| h == index);
        let fill = if strong {
            color
        } else {
            color.gamma_multiply(0.55)
        };
        painter.rect_filled(span_of(offset, size), 0.0, fill);
    }
    if let Some((index, _, _)) = hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            *sel = index;
        }
    }

    // Caption: the hovered span, else the selected section.
    let index = hovered.map(|(i, _, _)| i).unwrap_or(*sel);
    if let Some(section) = info.sections.get(index) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::mono(
                "\u{25AE}",
                metrics::STATUS_TEXT_SIZE,
                section_color(&section.name),
            ));
            ui.label(theme::mono(
                format!(
                    "{} \u{00B7} {:#06x}..{:#06x} \u{00B7} {} bytes",
                    section.name,
                    section.offset,
                    section.offset + section.size,
                    section.size
                ),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
        });
    }
}

fn mapped_sections(
    info: &ElfInfo,
    file_len: usize,
) -> impl Iterator<Item = (usize, u64, u64)> + '_ {
    info.sections.iter().enumerate().filter_map(move |(i, s)| {
        (s.size > 0 && s.sh_type != SHT_NOBITS && (s.offset + s.size) as usize <= file_len)
            .then_some((i, s.offset, s.size))
    })
}

// --- Tables ---

// `x` is a fraction of the row width; right-aligned cells anchor their right edge there.
struct Cell<'a> {
    text: &'a str,
    color: Color32,
    x: f32,
    right: bool,
}

fn table_row(ui: &mut egui::Ui, cells: &[Cell<'_>], selectable: bool) -> egui::Response {
    let width = ui.available_width();
    let sense = if selectable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), sense);
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        if selectable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    paint_cells(&painter, rect, cells);
    resp
}

fn paint_cells(painter: &egui::Painter, rect: egui::Rect, cells: &[Cell<'_>]) {
    let inner = rect.width() - metrics::PANEL_INSET_X * 2.0;
    for cell in cells {
        let x = rect.left() + metrics::PANEL_INSET_X + inner * cell.x;
        let (anchor, align) = if cell.right {
            (x, Align2::RIGHT_CENTER)
        } else {
            (x, Align2::LEFT_CENTER)
        };
        painter.text(
            pos2(anchor, rect.center().y),
            align,
            cell.text,
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            cell.color,
        );
    }
}

fn header_cells<'a>(layout: &'a [(&'a str, f32, bool)]) -> Vec<Cell<'a>> {
    layout
        .iter()
        .map(|&(text, x, right)| Cell {
            text,
            color: theme::TEXT_FAINT,
            x,
            right,
        })
        .collect()
}

const SECTION_COLS: [(&str, f32, bool); 6] = [
    ("name", 0.03, false),
    ("type", 0.34, false),
    ("flags", 0.5, false),
    ("addr", 0.72, true),
    ("offset", 0.86, true),
    ("size", 1.0, true),
];

fn section_table(ui: &mut egui::Ui, info: &ElfInfo, sel: &mut usize) {
    eyebrow(ui, "sections");
    table_row(ui, &header_cells(&SECTION_COLS), false);
    for (index, section) in info.sections.iter().enumerate() {
        if section.type_name == "NULL" {
            continue;
        }
        let addr = format!("{:#x}", section.addr);
        let offset = format!("{:#x}", section.offset);
        let size = section.size.to_string();
        let name_color = if index == *sel {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        let cells = [
            Cell {
                text: &section.name,
                color: name_color,
                x: SECTION_COLS[0].1,
                right: false,
            },
            Cell {
                text: &section.type_name,
                color: theme::TEXT_FAINT,
                x: SECTION_COLS[1].1,
                right: false,
            },
            Cell {
                text: &section.flags,
                color: theme::TEXT_DIM,
                x: SECTION_COLS[2].1,
                right: false,
            },
            Cell {
                text: &addr,
                color: theme::TEXT_DIM,
                x: SECTION_COLS[3].1,
                right: true,
            },
            Cell {
                text: &offset,
                color: theme::TEXT_DIM,
                x: SECTION_COLS[4].1,
                right: true,
            },
            Cell {
                text: &size,
                color: theme::TEXT,
                x: SECTION_COLS[5].1,
                right: true,
            },
        ];
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
        let painter = ui.painter_at(rect);
        if index == *sel {
            painter.rect_filled(rect, 0.0, theme::SURFACE);
            let bar =
                egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
            painter.rect_filled(bar, 0.0, theme::ELECTRIC);
        } else if resp.hovered() {
            painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let chip = egui::Rect::from_center_size(
            pos2(rect.left() + metrics::PANEL_INSET_X + 2.0, rect.center().y),
            vec2(5.0, 5.0),
        );
        painter.rect_filled(chip, 0.0, section_color(&section.name));
        paint_cells(&painter, rect, &cells);
        if resp.clicked() {
            *sel = index;
        }
    }
}

const SEGMENT_COLS: [(&str, f32, bool); 6] = [
    ("segment", 0.0, false),
    ("flags", 0.2, false),
    ("vaddr", 0.55, true),
    ("filesz", 0.72, true),
    ("memsz", 0.86, true),
    ("align", 1.0, true),
];

fn segment_table(ui: &mut egui::Ui, info: &ElfInfo) {
    eyebrow(ui, "program headers");
    table_row(ui, &header_cells(&SEGMENT_COLS), false);
    for segment in &info.segments {
        let vaddr = format!("{:#x}", segment.vaddr);
        let filesz = segment.file_size.to_string();
        let memsz = segment.mem_size.to_string();
        let align = format!("{:#x}", segment.align);
        table_row(
            ui,
            &[
                Cell {
                    text: &segment.type_name,
                    color: theme::TEXT,
                    x: SEGMENT_COLS[0].1,
                    right: false,
                },
                Cell {
                    text: &segment.flags,
                    color: theme::ELECTRIC,
                    x: SEGMENT_COLS[1].1,
                    right: false,
                },
                Cell {
                    text: &vaddr,
                    color: theme::TEXT_DIM,
                    x: SEGMENT_COLS[2].1,
                    right: true,
                },
                Cell {
                    text: &filesz,
                    color: theme::TEXT_DIM,
                    x: SEGMENT_COLS[3].1,
                    right: true,
                },
                Cell {
                    text: &memsz,
                    color: theme::TEXT_DIM,
                    x: SEGMENT_COLS[4].1,
                    right: true,
                },
                Cell {
                    text: &align,
                    color: theme::TEXT_FAINT,
                    x: SEGMENT_COLS[5].1,
                    right: true,
                },
            ],
            false,
        );
    }
}

const SYMBOL_COLS: [(&str, f32, bool); 4] = [
    ("value", 0.0, false),
    ("symbol", 0.24, false),
    ("bind", 0.78, false),
    ("section", 1.0, true),
];

fn symbol_table(ui: &mut egui::Ui, info: &ElfInfo) {
    eyebrow(ui, "symbols");
    table_row(ui, &header_cells(&SYMBOL_COLS), false);
    let mut order: Vec<usize> = (0..info.symbols.len()).collect();
    order.sort_by_key(|&i| info.symbols[i].value);
    for index in order {
        let symbol = &info.symbols[index];
        let value = format!("{:016x}", symbol.value);
        let (bind, name_color) = if symbol.global {
            ("GLOBAL", theme::TEXT)
        } else {
            ("LOCAL", theme::TEXT_DIM)
        };
        table_row(
            ui,
            &[
                Cell {
                    text: &value,
                    color: theme::TEXT_FAINT,
                    x: SYMBOL_COLS[0].1,
                    right: false,
                },
                Cell {
                    text: &symbol.name,
                    color: name_color,
                    x: SYMBOL_COLS[1].1,
                    right: false,
                },
                Cell {
                    text: bind,
                    color: if symbol.global {
                        theme::ELECTRIC
                    } else {
                        theme::TEXT_FAINT
                    },
                    x: SYMBOL_COLS[2].1,
                    right: false,
                },
                Cell {
                    text: &symbol.section,
                    color: theme::TEXT_DIM,
                    x: SYMBOL_COLS[3].1,
                    right: true,
                },
            ],
            false,
        );
    }
}

const RELOC_COLS: [(&str, f32, bool); 4] = [
    ("where", 0.0, false),
    ("type", 0.3, false),
    ("symbol", 0.62, false),
    ("addend", 1.0, true),
];

fn reloc_table(ui: &mut egui::Ui, info: &ElfInfo) {
    eyebrow(ui, "relocations");
    table_row(ui, &header_cells(&RELOC_COLS), false);
    for reloc in &info.relocations {
        let place = format!("{}+{:#x}", reloc.section, reloc.offset);
        let addend = reloc.addend.to_string();
        table_row(
            ui,
            &[
                Cell {
                    text: &place,
                    color: theme::TEXT_DIM,
                    x: RELOC_COLS[0].1,
                    right: false,
                },
                Cell {
                    text: &reloc.type_name,
                    color: theme::WARNING,
                    x: RELOC_COLS[1].1,
                    right: false,
                },
                Cell {
                    text: &reloc.symbol,
                    color: theme::TEXT,
                    x: RELOC_COLS[2].1,
                    right: false,
                },
                Cell {
                    text: &addend,
                    color: theme::TEXT_FAINT,
                    x: RELOC_COLS[3].1,
                    right: true,
                },
            ],
            false,
        );
    }
}

// --- Hex dump ---

fn hex_view(ui: &mut egui::Ui, bytes: &[u8], info: &ElfInfo, selected: usize) {
    let Some(section) = info.sections.get(selected) else {
        return;
    };
    eyebrow(ui, &format!("hex \u{00B7} {}", section.name));
    if section.sh_type == SHT_NOBITS {
        inset_line(
            ui,
            &format!(
                "occupies no file bytes; zero-filled to {} bytes at load",
                section.size
            ),
            theme::TEXT_FAINT,
        );
        return;
    }
    let start = section.offset as usize;
    let end = (start + section.size as usize).min(bytes.len());
    if start >= end {
        inset_line(ui, "(empty section)", theme::TEXT_FAINT);
        return;
    }
    // Addresses prefer the load address so .text lines up with the VM's pc.
    let base = if section.addr != 0 {
        section.addr as usize
    } else {
        start
    };
    hex_dump(ui, &bytes[start..end], base, HEX_MAX_BYTES);
}

fn hex_dump(ui: &mut egui::Ui, bytes: &[u8], base: usize, max: usize) {
    let shown = &bytes[..bytes.len().min(max)];
    let mut text = String::with_capacity(shown.len() * 4);
    for (row, chunk) in shown.chunks(HEX_COLS).enumerate() {
        text.push_str(&format!("{:08x}  ", base + row * HEX_COLS));
        for i in 0..HEX_COLS {
            match chunk.get(i) {
                Some(byte) => text.push_str(&format!("{byte:02x} ")),
                None => text.push_str("   "),
            }
            if i == HEX_COLS / 2 - 1 {
                text.push(' ');
            }
        }
        text.push(' ');
        for byte in chunk {
            let c = *byte;
            text.push(if (0x20..0x7f).contains(&c) {
                c as char
            } else {
                '\u{00B7}'
            });
        }
        text.push('\n');
    }
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.add(
            egui::Label::new(theme::mono(
                text,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ))
            .extend(),
        );
    });
    if bytes.len() > max {
        inset_line(
            ui,
            &format!("\u{2026} {} more bytes", bytes.len() - max),
            theme::TEXT_FAINT,
        );
    }
}

// --- Small chrome ---

fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.add_space(metrics::SECTION_GAP * 2.0);
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.label(theme::label(text));
    });
    ui.add_space(2.0);
}

fn inset_line(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.label(theme::mono(text, metrics::STATUS_TEXT_SIZE, color));
    });
}
