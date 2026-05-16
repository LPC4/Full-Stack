use crate::view::debug::snapshot::SlotState;
use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Align2, Color32, FontId, Rect, RichText, Stroke, Ui, pos2, vec2};

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
        (a.a() as f32 * (1.0 - t) + b.a() as f32 * t) as u8,
    )
}

// ---------------------------------------------------------------------------
// Layout Constants
// ---------------------------------------------------------------------------

const NUM_ROWS: usize = 12;
const CYCLE_COL_W: f32 = 50.0;
const ROW_H: f32 = 42.0;
const HEADER_H: f32 = 28.0;
const CORNER: f32 = 3.0;

const STAGE_LABELS: [&str; 5] = ["IF", "ID", "EX", "MEM", "WB"];

#[derive(Clone, Default)]
pub struct PipelineView;

impl CompilerView for PipelineView {
    fn title(&self) -> &'static str {
        "Pipeline"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        let palette = theme.pipeline;
        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let history = session.snapshot.pipeline.clone();
        ui.add_space(8.0);

        let available_w = ui.available_width();
        // Scale row height and cycle column to available space
        let cycle_col_w = (available_w * 0.07).clamp(30.0, CYCLE_COL_W);
        let stage_w = (available_w - cycle_col_w) / 5.0;
        let available_h = ui.available_height();
        let row_h = ((available_h - HEADER_H - 60.0) / NUM_ROWS as f32).clamp(20.0, ROW_H);
        let total_h = HEADER_H + NUM_ROWS as f32 * row_h;

        let (area, _) = ui.allocate_exact_size(vec2(available_w, total_h), egui::Sense::hover());
        let p = ui.painter_at(area);

        p.rect_filled(area, CORNER, palette.background);

        // -- Header --
        for (si, label) in STAGE_LABELS.iter().enumerate() {
            let x = area.min.x + cycle_col_w + si as f32 * stage_w;
            let cell = Rect::from_min_size(pos2(x, area.min.y), vec2(stage_w, HEADER_H));
            p.line_segment(
                [
                    pos2(cell.min.x + 4.0, cell.max.y - 2.0),
                    pos2(cell.max.x - 4.0, cell.max.y - 2.0),
                ],
                Stroke::new(2.0, palette.stage[si]),
            );
            p.text(
                cell.center(),
                Align2::CENTER_CENTER,
                *label,
                FontId::proportional(12.0),
                theme.text,
            );
        }

        // -- Grid --
        for row in 0..=NUM_ROWS {
            let y = area.min.y + HEADER_H + row as f32 * row_h;
            p.line_segment(
                [pos2(area.min.x, y), pos2(area.max.x, y)],
                Stroke::new(1.0, palette.grid),
            );
        }

        // -- Rows --
        let mnemonic_size = (row_h * 0.27).clamp(8.0, 12.0);
        let pc_size = (row_h * 0.21).clamp(7.0, 10.0);
        for row in 0..NUM_ROWS {
            let row_y = area.min.y + HEADER_H + row as f32 * row_h;
            let cyc_cell = Rect::from_min_size(pos2(area.min.x, row_y), vec2(cycle_col_w, row_h));
            if let Some(c) = history.cycle_for_row(row) {
                p.text(
                    cyc_cell.center(),
                    Align2::CENTER_CENTER,
                    format!("{c}"),
                    FontId::monospace(10.0),
                    palette.cycle_text,
                );
            }

            for si in 0..5 {
                let x = area.min.x + cycle_col_w + si as f32 * stage_w;
                let cell = Rect::from_min_size(pos2(x, row_y), vec2(stage_w, row_h)).shrink(2.0);

                match history.slot(si, row) {
                    Some(SlotState::Normal(entry)) => {
                        p.rect_stroke(
                            cell,
                            CORNER,
                            Stroke::new(1.0, palette.stage[si].gamma_multiply(0.5)),
                            egui::StrokeKind::Inside,
                        );
                        p.rect_filled(cell, CORNER, palette.cell);

                        let pc_y = cell.min.y + row_h * 0.28;
                        let mn_y = cell.min.y + row_h * 0.62;
                        p.text(
                            pos2(cell.center().x, pc_y),
                            Align2::CENTER_CENTER,
                            format!("{:#06x}", entry.pc & 0xFFFF),
                            FontId::monospace(pc_size),
                            lerp_color(palette.stage[si], theme.text, 0.4),
                        );
                        p.text(
                            pos2(cell.center().x, mn_y),
                            Align2::CENTER_CENTER,
                            &entry.mnemonic,
                            FontId::monospace(mnemonic_size),
                            Color32::WHITE,
                        );
                    }
                    Some(SlotState::StallBubble) => {
                        p.rect_stroke(
                            cell,
                            CORNER,
                            Stroke::new(1.0, palette.stall.gamma_multiply(0.6)),
                            egui::StrokeKind::Inside,
                        );
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "STALL",
                            FontId::monospace(10.0),
                            palette.stall,
                        );
                    }
                    Some(SlotState::FlushBubble) => {
                        p.rect_stroke(
                            cell,
                            CORNER,
                            Stroke::new(1.0, palette.flush.gamma_multiply(0.6)),
                            egui::StrokeKind::Inside,
                        );
                        p.rect_filled(cell, CORNER, palette.flush.gamma_multiply(0.05));
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "FLUSH",
                            FontId::monospace(10.0),
                            palette.flush,
                        );
                    }
                    _ => {
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "·",
                            FontId::monospace(12.0),
                            palette.grid,
                        );
                    }
                }
            }
        }

        // -- Stats --
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 15.0;
            let mut label = |txt, val, color: Color32| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(txt).small().weak());
                    ui.label(RichText::new(val).small().color(color).monospace());
                });
            };
            label("CYCLES", format!("{}", history.total_cycles), theme.text);
            label("STALLS", format!("{}", history.stall_cycles), palette.stall);
            label(
                "FLUSHES",
                format!("{}", history.flush_cycles),
                palette.flush,
            );
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
