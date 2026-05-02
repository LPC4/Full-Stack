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
        let Some(_session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        ui.heading("Cache");
        ui.add_space(6.0);

        // The cache is structurally defined in memory/cache.rs but is not yet
        // wired into the SystemBus — the bus talks directly to Ram/Rom. Stats
        // will appear here automatically once a Cache<Ram> replaces the raw Ram
        // in SystemBus and the snapshot accessor is plumbed through.
        Frame::NONE
            .fill(Color32::from_gray(28))
            .stroke(Stroke::new(1.0, Color32::from_gray(60)))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.colored_label(
                    Color32::from_rgb(200, 160, 50),
                    "Cache not yet wired into SystemBus.",
                );
                ui.label(
                    RichText::new(
                        "Replace Ram with Cache<Ram> in bus.rs to enable live stats.",
                    )
                    
                    .weak(),
                );
            });

        ui.add_space(12.0);

        // Layout for when cache IS wired (shown with placeholder zeros).
        ui.label(RichText::new("L1 Data Cache").strong());
        stats_block(ui, "l1d", 0, 0, 0, 0);

        ui.add_space(8.0);
        ui.label(RichText::new("L1 Instruction Cache").strong());
        stats_block(ui, "l1i", 0, 0, 0, 0);
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
                    ui.label(RichText::new("Hits").monospace().color(Color32::from_gray(160)));
                    ui.label(RichText::new("Misses").monospace().color(Color32::from_gray(160)));
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
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
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
