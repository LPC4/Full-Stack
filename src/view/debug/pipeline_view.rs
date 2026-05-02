use crate::view::debug::snapshot::PipelineHistory;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, FontId, Rect, RichText, Ui, vec2};

const STAGE_LABELS: [&str; 5] = ["Fetch", "Decode", "Execute", "Memory", "Writeback"];
const STAGE_COLORS: [Color32; 5] = [
    Color32::from_rgb(30, 55, 90),
    Color32::from_rgb(35, 70, 110),
    Color32::from_rgb(40, 90, 140),
    Color32::from_rgb(45, 110, 160),
    Color32::from_rgb(55, 135, 195),
];

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

        ui.add_space(8.0);

        // PC bar
        {
            let w = ui.available_width();
            let h = 28.0;
            let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(50, 50, 70));
            ui.painter().text(
                rect.left_center() + vec2(10.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "PC",
                FontId::monospace(11.0),
                Color32::from_gray(140),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{pc:#018x}"),
                FontId::monospace(13.0),
                Color32::WHITE,
            );
        }

        ui.add_space(16.0);
        pipeline_diagram(ui, &history, ui.available_width());
        ui.add_space(16.0);

        ui.label(
            RichText::new(
                "Pipeline stages are simulated from the last 5 committed PCs.\n\
                 Fetch = oldest, Writeback = most recently retired.",
            )
            
            .weak(),
        );
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

pub fn pipeline_diagram(ui: &mut Ui, history: &PipelineHistory, w: f32) {
    let stages = history.stages();
    let bar_h = 40.0;
    let gap = 4.0;
    let label_w = 0.0;
    let bar_w = (w - label_w - gap * 4.0) / 5.0;

    let total_h = bar_h + 20.0; // room for stage label above
    let (area, _) = ui.allocate_exact_size(vec2(w, total_h), egui::Sense::hover());

    for (display_idx, label) in STAGE_LABELS.iter().enumerate() {
        // display_idx 0 = Fetch = history[4] (oldest)
        let history_idx = 4 - display_idx;
        let pc_opt = stages[history_idx];
        let color = if pc_opt.is_some() {
            STAGE_COLORS[display_idx]
        } else {
            Color32::from_gray(28)
        };

        let x = area.min.x + display_idx as f32 * (bar_w + gap);
        let bar_rect = Rect::from_min_size(
            egui::pos2(x, area.min.y + 18.0),
            vec2(bar_w, bar_h),
        );

        ui.painter().rect_filled(bar_rect, 3.0, color);

        // Stage label above bar
        ui.painter().text(
            bar_rect.center_top() - vec2(0.0, 2.0),
            egui::Align2::CENTER_BOTTOM,
            *label,
            FontId::proportional(10.0),
            Color32::from_gray(160),
        );

        // PC value inside bar
        let pc_text = pc_opt
            .map(|p| format!("{p:#010x}"))
            .unwrap_or_else(|| "- - -".into());
        ui.painter().text(
            bar_rect.center(),
            egui::Align2::CENTER_CENTER,
            pc_text,
            FontId::monospace(10.5),
            if pc_opt.is_some() { Color32::WHITE } else { Color32::from_gray(50) },
        );
    }
}
