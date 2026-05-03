use crate::view::debug::snapshot::SlotState;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Align2, Color32, FontId, Rect, RichText, Stroke, Ui, pos2, vec2};

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const NUM_ROWS: usize = 10;
const CYCLE_COL_W: f32 = 64.0;
const ROW_H: f32 = 46.0;
const HEADER_H: f32 = 24.0;
const CORNER: f32 = 2.0;

const STAGE_LABELS: [&str; 5] = ["IF", "ID", "EX", "MEM", "WB"];

// Per-stage background (blue gradient: darker fetch -> brighter writeback)
const STAGE_BG: [Color32; 5] = [
    Color32::from_rgb(22,  42,  78),
    Color32::from_rgb(28,  56,  100),
    Color32::from_rgb(34,  76,  128),
    Color32::from_rgb(40,  98,  152),
    Color32::from_rgb(50, 122,  182),
];

const STAGE_HEADER_BG: [Color32; 5] = [
    Color32::from_rgb(16, 30, 55),
    Color32::from_rgb(20, 40, 70),
    Color32::from_rgb(24, 52, 88),
    Color32::from_rgb(28, 66, 104),
    Color32::from_rgb(34, 82, 124),
];

const EMPTY_BG:       Color32 = Color32::from_rgb(18, 18, 28);
const STALL_BG:       Color32 = Color32::from_rgb(80, 50, 10);
const FLUSH_BG:       Color32 = Color32::from_rgb(70, 15, 15);
const CYCLE_BG:       Color32 = Color32::from_rgb(15, 15, 25);
const GRID_LINE:      Color32 = Color32::from_rgb(40, 40, 60);
const PC_TEXT:        Color32 = Color32::from_rgb(140, 190, 255);
const MNEM_TEXT:      Color32 = Color32::WHITE;
const EMPTY_TEXT:     Color32 = Color32::from_rgb(40, 40, 55);
const STALL_TEXT:     Color32 = Color32::from_rgb(220, 140, 40);
const FLUSH_TEXT:     Color32 = Color32::from_rgb(220, 70, 70);
const CYCLE_TEXT:     Color32 = Color32::from_rgb(120, 120, 150);
const HEADER_TEXT:    Color32 = Color32::from_rgb(180, 190, 210);

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

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
        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let history = session.snapshot.pipeline.clone();
        let pc = session.snapshot.cpu.pc;
        let steps = session.step_count;

        ui.add_space(6.0);

        // -- Status bar ----------------------------------------------------
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("PC  {pc:#018x}"))
                    .monospace()
                    .color(Color32::WHITE),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("Cycle #{}", history.total_cycles))
                    .monospace()
                    .color(Color32::from_gray(170)),
            );
            ui.separator();
            ui.label(RichText::new("IF -> ID -> EX -> MEM -> WB").weak().small());
        });

        ui.add_space(10.0);

        // -- Waterfall diagram ---------------------------------------------
        let available_w = ui.available_width();
        let stage_w = (available_w - CYCLE_COL_W) / 5.0;
        let total_h = HEADER_H + NUM_ROWS as f32 * ROW_H;

        let (area, _) = ui.allocate_exact_size(
            vec2(available_w, total_h),
            egui::Sense::hover(),
        );
        let p = ui.painter_at(area);

        // Header row
        {
            let h_rect = Rect::from_min_size(area.min, vec2(available_w, HEADER_H));
            p.rect_filled(h_rect, 0.0, Color32::from_rgb(12, 12, 20));

            let cyc_rect = Rect::from_min_size(area.min, vec2(CYCLE_COL_W, HEADER_H));
            p.text(
                cyc_rect.center(),
                Align2::CENTER_CENTER,
                "Cycle",
                FontId::proportional(10.0),
                CYCLE_TEXT,
            );

            for (si, label) in STAGE_LABELS.iter().enumerate() {
                let x = area.min.x + CYCLE_COL_W + si as f32 * stage_w;
                let cell = Rect::from_min_size(pos2(x, area.min.y), vec2(stage_w, HEADER_H));
                p.rect_filled(cell, 0.0, STAGE_HEADER_BG[si]);
                p.text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    *label,
                    FontId::proportional(11.5),
                    HEADER_TEXT,
                );
            }
        }

        // Vertical grid lines
        p.line_segment(
            [pos2(area.min.x + CYCLE_COL_W, area.min.y),
             pos2(area.min.x + CYCLE_COL_W, area.max.y)],
            Stroke::new(1.0, GRID_LINE),
        );
        for si in 1..5 {
            let x = area.min.x + CYCLE_COL_W + si as f32 * stage_w;
            p.line_segment(
                [pos2(x, area.min.y), pos2(x, area.max.y)],
                Stroke::new(1.0, GRID_LINE),
            );
        }

        // Horizontal line under header
        let hy = area.min.y + HEADER_H;
        p.line_segment(
            [pos2(area.min.x, hy), pos2(area.max.x, hy)],
            Stroke::new(1.0, GRID_LINE),
        );

        // Data rows -- row 0 = most recent cycle
        for row in 0..NUM_ROWS {
            let row_y = area.min.y + HEADER_H + row as f32 * ROW_H;

            p.line_segment(
                [pos2(area.min.x, row_y + ROW_H),
                 pos2(area.max.x, row_y + ROW_H)],
                Stroke::new(1.0, GRID_LINE),
            );

            // Cycle number column
            let cyc_cell = Rect::from_min_size(pos2(area.min.x, row_y), vec2(CYCLE_COL_W, ROW_H));
            p.rect_filled(cyc_cell, 0.0, CYCLE_BG);
            let cycle_label = history
                .cycle_for_row(row)
                .map(|c| format!("#{c}"))
                .unwrap_or_else(|| "-".into());
            p.text(
                cyc_cell.center(),
                Align2::CENTER_CENTER,
                cycle_label,
                FontId::monospace(10.0),
                CYCLE_TEXT,
            );

            // Stage cells
            for si in 0..5 {
                let x = area.min.x + CYCLE_COL_W + si as f32 * stage_w;
                let cell = Rect::from_min_size(pos2(x, row_y), vec2(stage_w, ROW_H));

                match history.slot(si, row) {
                    Some(SlotState::Normal(entry)) => {
                        p.rect_filled(cell, CORNER, STAGE_BG[si]);

                        let pc_short = format!("{:#010x}", entry.pc);
                        p.text(
                            pos2(cell.center().x, cell.min.y + ROW_H * 0.28),
                            Align2::CENTER_CENTER,
                            pc_short,
                            FontId::monospace(9.0),
                            PC_TEXT,
                        );
                        p.text(
                            pos2(cell.center().x, cell.min.y + ROW_H * 0.68),
                            Align2::CENTER_CENTER,
                            &entry.mnemonic,
                            FontId::monospace(12.5),
                            MNEM_TEXT,
                        );
                    }
                    Some(SlotState::StallBubble) => {
                        p.rect_filled(cell, CORNER, STALL_BG);
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "STALL",
                            FontId::monospace(11.0),
                            STALL_TEXT,
                        );
                    }
                    Some(SlotState::FlushBubble) => {
                        p.rect_filled(cell, CORNER, FLUSH_BG);
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "FLUSH",
                            FontId::monospace(11.0),
                            FLUSH_TEXT,
                        );
                    }
                    Some(SlotState::Empty) | None => {
                        p.rect_filled(cell, CORNER, EMPTY_BG);
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            "-",
                            FontId::monospace(11.0),
                            EMPTY_TEXT,
                        );
                    }
                }
            }
        }

        ui.add_space(10.0);

        // -- Stats footer --------------------------------------------------
        let cycles = history.total_cycles;
        let stalls = history.stall_cycles;
        let flushes = history.flush_cycles;
        let branches = history.branches_seen;
        let mispredicts = history.branches_mispredicted;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Steps: {steps}   *   Cycles: {cycles}"))
                    .small()
                    .weak(),
            );
            ui.separator();
            if stalls > 0 {
                ui.label(
                    RichText::new(format!("Stalls: {stalls}"))
                        .small()
                        .color(STALL_TEXT),
                );
                ui.separator();
            }
            if flushes > 0 {
                ui.label(
                    RichText::new(format!("Flushes: {flushes}"))
                        .small()
                        .color(FLUSH_TEXT),
                );
                ui.separator();
            }
            if branches > 0 {
                let accuracy = 100.0 * (1.0 - mispredicts as f64 / branches as f64);
                ui.label(
                    RichText::new(format!("Branch acc: {accuracy:.1}%"))
                        .small()
                        .weak(),
                );
            }
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
