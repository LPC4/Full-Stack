use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, Frame, Grid, RichText, Stroke, Ui};

#[derive(Clone, Default)]
pub struct CacheView;

impl CompilerView for CacheView {
    fn title(&self) -> &'static str {
        "Cache"
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

        ui.heading("Cache Hierarchy");
        ui.add_space(6.0);

        // Get cache stats from the debug session snapshot
        let l1_stats = &session.snapshot.l1_stats;
        let l2_stats = &session.snapshot.l2_stats;
        let l3_stats = &session.snapshot.l3_stats;

        Frame::NONE
            .fill(Color32::from_gray(28))
            .stroke(Stroke::new(1.0, Color32::from_gray(60)))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.colored_label(
                    Color32::from_rgb(60, 200, 80),
                    "Three-Level Cache Hierarchy Active",
                );
                ui.label(
                    RichText::new(
                        "L1: 4KB, 2-way | L2: 256KB, 8-way | L3: 8MB, 16-way | 64-byte blocks, write-back",
                    )
                    .weak(),
                );
            });

        ui.add_space(12.0);

        // Display L1 cache statistics
        ui.label(RichText::new("L1 Cache (4KB, 2-way)").strong());
        stats_block(
            ui,
            "l1",
            l1_stats.read_hits,
            l1_stats.read_misses,
            l1_stats.write_hits,
            l1_stats.write_misses,
        );

        ui.add_space(8.0);

        // Display L2 cache statistics
        ui.label(RichText::new("L2 Cache (256KB, 8-way)").strong());
        stats_block(
            ui,
            "l2",
            l2_stats.read_hits,
            l2_stats.read_misses,
            l2_stats.write_hits,
            l2_stats.write_misses,
        );

        ui.add_space(8.0);

        // Display L3 cache statistics
        ui.label(RichText::new("L3 Cache (8MB, 16-way)").strong());
        stats_block(
            ui,
            "l3",
            l3_stats.read_hits,
            l3_stats.read_misses,
            l3_stats.write_hits,
            l3_stats.write_misses,
        );
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

fn stats_block(
    ui: &mut Ui,
    id: &str,
    read_hits: u64,
    read_misses: u64,
    write_hits: u64,
    write_misses: u64,
) {
    let total_reads = read_hits + read_misses;
    let total_writes = write_hits + write_misses;
    let read_rate = if total_reads > 0 {
        read_hits as f64 / total_reads as f64 * 100.0
    } else {
        0.0
    };
    let write_rate = if total_writes > 0 {
        write_hits as f64 / total_writes as f64 * 100.0
    } else {
        0.0
    };

    Frame::NONE
        .stroke(Stroke::new(1.0, Color32::from_gray(55)))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            Grid::new(id)
                .num_columns(3)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("").monospace());
                    ui.label(
                        RichText::new("Hits")
                            .monospace()
                            .color(Color32::from_gray(160)),
                    );
                    ui.label(
                        RichText::new("Misses")
                            .monospace()
                            .color(Color32::from_gray(160)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Reads").monospace());
                    ui.label(RichText::new(read_hits.to_string()).monospace());
                    ui.label(RichText::new(read_misses.to_string()).monospace());
                    ui.end_row();

                    ui.label(RichText::new("Writes").monospace());
                    ui.label(RichText::new(write_hits.to_string()).monospace());
                    ui.label(RichText::new(write_misses.to_string()).monospace());
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                hit_rate_bar(ui, "Read hit rate", read_rate);
                ui.add_space(12.0);
                hit_rate_bar(ui, "Write hit rate", write_rate);
            });
        });
}

fn hit_rate_bar(ui: &mut Ui, label: &str, pct: f64) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).color(Color32::from_gray(150)));
        let bar_w = 120.0;
        let bar_h = 10.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, Color32::from_gray(40));
        let fill_w = bar_w * (pct as f32 / 100.0).clamp(0.0, 1.0);
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, bar_h));
        let color = if pct >= 90.0 {
            Color32::from_rgb(60, 200, 80)
        } else if pct >= 60.0 {
            Color32::from_rgb(200, 180, 40)
        } else {
            Color32::from_rgb(200, 60, 60)
        };
        ui.painter().rect_filled(fill, 2.0, color);
        ui.label(RichText::new(format!("{pct:.1}%")).monospace());
    });
}
