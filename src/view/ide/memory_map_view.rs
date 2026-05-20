use asm_to_binary::assembler::section::SectionKind;
use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Color32, Frame, Grid, Rect, RichText, Sense, Stroke, Vec2};

const COLOR_TEXT: Color32 = Color32::from_rgb(80, 140, 220);
const COLOR_RODATA: Color32 = Color32::from_rgb(60, 180, 160);
const COLOR_DATA: Color32 = Color32::from_rgb(80, 190, 100);
const COLOR_BSS: Color32 = Color32::from_rgb(200, 160, 60);
const COLOR_HEAP: Color32 = Color32::from_rgb(160, 100, 200);
const COLOR_STACK: Color32 = Color32::from_rgb(220, 80, 80);

#[derive(Default, Clone)]
pub struct MemoryMapView;

struct SectionEntry {
    name: String,
    start: u64,
    end: u64,
    color: Color32,
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn section_color(kind: &SectionKind) -> Color32 {
    match kind {
        SectionKind::Text => COLOR_TEXT,
        SectionKind::RoData => COLOR_RODATA,
        SectionKind::Data => COLOR_DATA,
        SectionKind::Bss => COLOR_BSS,
        SectionKind::Custom(_) => Color32::from_rgb(140, 140, 140),
    }
}

fn collect_sections(
    assembled: &asm_to_binary::assembler::output::AssembledOutput,
    load_base: u64,
) -> Vec<SectionEntry> {
    let mut entries = Vec::new();
    let mut running = load_base;
    // Non-BSS first (mirrors ELF layout)
    for pass_bss in [false, true] {
        for sec in &assembled.sections {
            if let Some(kind) = &sec.kind {
                if matches!(kind, SectionKind::Bss) != pass_bss {
                    continue;
                }
                let start = running;
                let end = start + sec.bytes.len() as u64;
                entries.push(SectionEntry {
                    name: kind.name().to_owned(),
                    start,
                    end,
                    color: section_color(kind),
                });
                running = end;
            }
        }
    }

    // Heap (from symbol table)
    if let Some(&heap_off) = assembled.symbol_table.get("__heap_start") {
        let heap_start = load_base + heap_off;
        let heap_end = assembled
            .symbol_table
            .get("__heap_end")
            .map(|&o| load_base + o)
            .unwrap_or(heap_start + 0x1_0000); // 64 KB default hint
        entries.push(SectionEntry {
            name: "heap".to_owned(),
            start: heap_start,
            end: heap_end,
            color: COLOR_HEAP,
        });
    }

    // Stack (from symbol table)
    if let Some(&stack_off) = assembled.symbol_table.get("__stack_top") {
        let stack_top = load_base + stack_off;
        entries.push(SectionEntry {
            name: "stack".to_owned(),
            start: stack_top.saturating_sub(0x8000), // 32 KB hint
            end: stack_top,
            color: COLOR_STACK,
        });
    }

    entries
}

impl CompilerView for MemoryMapView {
    fn title(&self) -> &'static str {
        "Memory Map"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();

        let Some(assembled) = &state.assembled else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Compile code to see the memory map.").weak());
            });
            return;
        };

        let sections = collect_sections(assembled, state.load_base);
        if sections.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No sections found.").weak());
            });
            return;
        }

        let total_bytes: u64 = assembled.total_bytes() as u64;
        let lo = sections.iter().map(|s| s.start).min().unwrap_or(0);
        let hi = sections.iter().map(|s| s.end).max().unwrap_or(lo + 1);
        let span = (hi - lo).max(1) as f64;

        // Live registers from debug session
        let live_pc = state.debug_session.as_ref().map(|s| s.snapshot.cpu.pc);
        let live_sp = state
            .debug_session
            .as_ref()
            .map(|s| s.snapshot.cpu.xregs[2]);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                // ---- Mini-map visualization ----
                let map_h = (ui.available_height() * 0.35).clamp(80.0, 200.0);
                let map_w = ui.available_width() - 16.0;

                Frame::NONE
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border_soft))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Virtual Address Space").small().weak());
                        ui.add_space(4.0);

                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(map_w, map_h), Sense::hover());
                        let p = ui.painter();

                        // Background
                        p.rect_filled(rect, 2.0, theme.panel);

                        // Draw each section as a colored band
                        for sec in &sections {
                            let x0 = rect.min.x
                                + (sec.start.saturating_sub(lo) as f64 / span * map_w as f64) as f32;
                            let x1 = rect.min.x
                                + (sec.end.saturating_sub(lo) as f64 / span * map_w as f64) as f32;
                            let band = Rect::from_min_max(
                                egui::pos2(x0, rect.min.y + 4.0),
                                egui::pos2((x1 - 1.0).max(x0 + 2.0), rect.max.y - 20.0),
                            );
                            p.rect_filled(band, 2.0, sec.color.gamma_multiply(0.7));
                            p.rect_stroke(
                                band,
                                2.0,
                                Stroke::new(1.0, sec.color),
                                egui::StrokeKind::Inside,
                            );
                            // Label if wide enough
                            if band.width() > 20.0 {
                                p.text(
                                    band.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &sec.name,
                                    egui::FontId::proportional(10.0),
                                    Color32::WHITE,
                                );
                            }
                        }

                        // Address ruler at bottom
                        let ruler_y = rect.max.y - 16.0;
                        p.line_segment(
                            [
                                egui::pos2(rect.min.x, ruler_y),
                                egui::pos2(rect.max.x, ruler_y),
                            ],
                            Stroke::new(1.0, theme.border_soft),
                        );
                        for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
                            let addr = lo + (span * frac) as u64;
                            let rx = rect.min.x + frac as f32 * map_w;
                            p.line_segment(
                                [egui::pos2(rx, ruler_y), egui::pos2(rx, ruler_y + 4.0)],
                                Stroke::new(1.0, theme.border_soft),
                            );
                            p.text(
                                egui::pos2(rx, rect.max.y - 2.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{addr:#010x}"),
                                egui::FontId::monospace(8.0),
                                theme.text_dim,
                            );
                        }

                        // Live PC marker
                        if let Some(pc) = live_pc {
                            if pc >= lo && pc <= hi {
                                let pcx = rect.min.x
                                    + ((pc - lo) as f64 / span * map_w as f64) as f32;
                                p.line_segment(
                                    [
                                        egui::pos2(pcx, rect.min.y),
                                        egui::pos2(pcx, ruler_y),
                                    ],
                                    Stroke::new(2.0, theme.info),
                                );
                                p.text(
                                    egui::pos2(pcx, rect.min.y + 2.0),
                                    egui::Align2::CENTER_TOP,
                                    "PC",
                                    egui::FontId::proportional(9.0),
                                    theme.info,
                                );
                            }
                        }

                        // Live SP marker
                        if let Some(sp) = live_sp {
                            if sp >= lo && sp <= hi {
                                let spx = rect.min.x
                                    + ((sp - lo) as f64 / span * map_w as f64) as f32;
                                p.line_segment(
                                    [
                                        egui::pos2(spx, rect.min.y),
                                        egui::pos2(spx, ruler_y),
                                    ],
                                    Stroke::new(2.0, theme.warning),
                                );
                                p.text(
                                    egui::pos2(spx, rect.min.y + 12.0),
                                    egui::Align2::CENTER_TOP,
                                    "SP",
                                    egui::FontId::proportional(9.0),
                                    theme.warning,
                                );
                            }
                        }
                    });

                ui.add_space(10.0);

                // ---- Section table ----
                Frame::NONE
                    .stroke(Stroke::new(1.0, theme.border_soft))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        Grid::new("memory_map_grid")
                            .num_columns(5)
                            .spacing([16.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                // Header
                                for h in ["Section", "Start", "End", "Size", ""] {
                                    ui.label(
                                        RichText::new(h).small().strong().color(theme.text_dim),
                                    );
                                }
                                ui.end_row();

                                for sec in &sections {
                                    let size = (sec.end - sec.start) as usize;
                                    // Color swatch
                                    let (sw, _) = ui.allocate_exact_size(
                                        Vec2::new(8.0, 8.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(sw, 2.0, sec.color);
                                    // HACK: that allocated to the wrong column - use a horizontal layout
                                    // Redo: just use label with colored text
                                    let _ = sw; // already painted
                                    ui.label(
                                        RichText::new(&sec.name)
                                            .monospace()
                                            .small()
                                            .color(sec.color),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:#010x}", sec.start))
                                            .monospace()
                                            .small(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:#010x}", sec.end))
                                            .monospace()
                                            .small(),
                                    );
                                    ui.label(RichText::new(format_size(size)).small());
                                    // Mini bar
                                    let bar_w = (size as f64 / total_bytes as f64 * 120.0) as f32;
                                    let (br, _) = ui.allocate_exact_size(
                                        Vec2::new(120.0, 8.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(br, 1.0, theme.surface_alt);
                                    ui.painter().rect_filled(
                                        Rect::from_min_size(br.min, Vec2::new(bar_w.max(1.0), 8.0)),
                                        1.0,
                                        sec.color.gamma_multiply(0.7),
                                    );
                                    ui.end_row();
                                }
                            });
                    });

                ui.add_space(10.0);

                // ---- Symbol summary ----
                let n_syms = assembled.symbol_table.len();
                if n_syms > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{n_syms} symbols  |  {} globals  |  total {}",
                            assembled.global_symbols.len(),
                            format_size(assembled.total_bytes())
                        ))
                        .small()
                        .weak(),
                    );
                }

                // ---- Live state ----
                if let (Some(pc), Some(sp)) = (live_pc, live_sp) {
                    ui.add_space(6.0);
                    Frame::NONE
                        .fill(theme.panel_alt)
                        .stroke(Stroke::new(1.0, theme.border_soft))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("PC").small().color(theme.info));
                                ui.label(
                                    RichText::new(format!("{pc:#010x}"))
                                        .monospace()
                                        .small()
                                        .color(theme.info),
                                );
                                ui.add_space(16.0);
                                ui.label(RichText::new("SP").small().color(theme.warning));
                                ui.label(
                                    RichText::new(format!("{sp:#010x}"))
                                        .monospace()
                                        .small()
                                        .color(theme.warning),
                                );
                            });
                        });
                }
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
