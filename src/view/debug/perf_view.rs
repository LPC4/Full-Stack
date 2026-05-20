//! Performance dashboard: IPC, stall/flush rate, branch accuracy, cache hit rates.

use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Color32, RichText, Ui, Vec2};

#[derive(Clone, Default)]
pub struct PerfView;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hit_rate(hits: u64, misses: u64) -> f32 {
    let total = hits + misses;
    if total == 0 {
        0.0
    } else {
        hits as f32 / total as f32
    }
}

fn draw_metric(
    ui: &mut Ui,
    label: &str,
    frac: f32,
    bar_color: Color32,
    value_text: &str,
    note: &str,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            Vec2::new(110.0, 16.0),
            egui::Label::new(RichText::new(label).monospace().size(11.0).weak()),
        );
        ui.add_sized(
            Vec2::new(62.0, 16.0),
            egui::Label::new(
                RichText::new(value_text)
                    .monospace()
                    .size(11.0)
                    .strong()
                    .color(bar_color),
            ),
        );

        let bar_w = ui.available_width().max(20.0);
        let bar_h = 13.0;
        let (bar_rect, resp) =
            ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(bar_rect, 2.0, bar_color.gamma_multiply(0.12));
        let fill_rect = bar_rect.with_max_x(
            bar_rect.min.x + bar_rect.width() * frac.clamp(0.0, 1.0),
        );
        p.rect_filled(fill_rect, 2.0, bar_color.gamma_multiply(0.65));

        if !note.is_empty() {
            resp.on_hover_text(note);
        }
    });
    ui.add_space(3.0);
}

fn section_header(ui: &mut Ui, title: &str, theme_color: Color32) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(3.0, 14.0),
            ),
            0.0,
            theme_color,
        );
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong().size(12.0).color(theme_color));
    });
    ui.add_space(4.0);
}

// ---------------------------------------------------------------------------
// CompilerView impl
// ---------------------------------------------------------------------------

impl CompilerView for PerfView {
    fn title(&self) -> &'static str {
        "Performance"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();

        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let history = &session.snapshot.pipeline;
        let csrs = &session.snapshot.cpu.csrs;

        let total = history.total_cycles;
        let instret = csrs.instret;
        let stalls = history.stall_cycles;
        let flushes = history.flush_cycles;
        let branches = history.branches_seen;
        let mispredicts = history.branches_mispredicted;

        let ipc = if total > 0 { instret as f32 / total as f32 } else { 0.0 };
        let stall_rate = if total > 0 { stalls as f32 / total as f32 } else { 0.0 };
        let flush_rate = if total > 0 { flushes as f32 / total as f32 } else { 0.0 };
        let branch_acc = if branches > 0 {
            (branches - mispredicts) as f32 / branches as f32
        } else {
            1.0
        };

        let l1 = &session.snapshot.l1_stats;
        let l2 = &session.snapshot.l2_stats;
        let l3 = &session.snapshot.l3_stats;
        let l1_hr = hit_rate(l1.read_hits + l1.write_hits, l1.read_misses + l1.write_misses);
        let l2_hr = hit_rate(l2.read_hits + l2.write_hits, l2.read_misses + l2.write_misses);
        let l3_hr = hit_rate(l3.read_hits + l3.write_hits, l3.read_misses + l3.write_misses);

        // Color each metric by quality thresholds
        let ipc_color = if ipc >= 0.8 { theme.success } else if ipc >= 0.5 { theme.warning } else { theme.error };
        let stall_color = if stall_rate < 0.10 { theme.success } else if stall_rate < 0.25 { theme.warning } else { theme.error };
        let flush_color = if flush_rate < 0.05 { theme.success } else if flush_rate < 0.15 { theme.warning } else { theme.error };
        let bacc_color = if branch_acc >= 0.95 { theme.success } else if branch_acc >= 0.80 { theme.warning } else { theme.error };

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // ---- CPU Efficiency ----
            section_header(ui, "CPU Efficiency", theme.info);

            draw_metric(
                ui,
                "IPC",
                ipc.min(1.0),
                ipc_color,
                &format!("{ipc:.3}"),
                "target >= 0.8",
            );
            draw_metric(
                ui,
                "Stall rate",
                stall_rate,
                stall_color,
                &format!("{:.1}%", stall_rate * 100.0),
                &format!("{stalls} / {total} cycles"),
            );
            draw_metric(
                ui,
                "Flush rate",
                flush_rate,
                flush_color,
                &format!("{:.1}%", flush_rate * 100.0),
                &format!("{flushes} / {total} cycles"),
            );
            draw_metric(
                ui,
                "Branch acc.",
                branch_acc,
                bacc_color,
                &format!("{:.1}%", branch_acc * 100.0),
                &format!("{} / {} correct", branches - mispredicts, branches),
            );

            // ---- Cache ----
            section_header(ui, "Cache Hierarchy", theme.accent);

            let cache_color = |hr: f32| {
                if hr >= 0.90 { theme.success } else if hr >= 0.70 { theme.warning } else { theme.error }
            };

            draw_metric(
                ui,
                "L1 hit rate",
                l1_hr,
                cache_color(l1_hr),
                &format!("{:.1}%", l1_hr * 100.0),
                &format!(
                    "{} hits / {} misses",
                    l1.read_hits + l1.write_hits,
                    l1.read_misses + l1.write_misses
                ),
            );
            draw_metric(
                ui,
                "L2 hit rate",
                l2_hr,
                cache_color(l2_hr),
                &format!("{:.1}%", l2_hr * 100.0),
                &format!(
                    "{} hits / {} misses",
                    l2.read_hits + l2.write_hits,
                    l2.read_misses + l2.write_misses
                ),
            );
            draw_metric(
                ui,
                "L3 hit rate",
                l3_hr,
                cache_color(l3_hr),
                &format!("{:.1}%", l3_hr * 100.0),
                &format!(
                    "{} hits / {} misses",
                    l3.read_hits + l3.write_hits,
                    l3.read_misses + l3.write_misses
                ),
            );

            // ---- Raw Counters ----
            section_header(ui, "Counters", theme.text_dim);

            egui::Grid::new("perf_counters")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    let mut row = |a: &str, av: u64, b: &str, bv: u64| {
                        ui.label(RichText::new(a).size(11.0).weak());
                        ui.label(
                            RichText::new(format!("{av:>12}"))
                                .monospace()
                                .size(11.0)
                                .strong(),
                        );
                        ui.end_row();
                        ui.label(RichText::new(b).size(11.0).weak());
                        ui.label(
                            RichText::new(format!("{bv:>12}"))
                                .monospace()
                                .size(11.0)
                                .strong(),
                        );
                        ui.end_row();
                    };
                    row("Cycles", total, "Instructions", instret);
                    row("Stalls", stalls, "Flushes", flushes);
                    row("Branches", branches, "Mispredicts", mispredicts);
                });
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
