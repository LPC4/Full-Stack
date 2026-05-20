use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use virtual_machine::memory::cache::{CacheParamsSnapshot, CacheSnapshot};
use egui::{Color32, Frame, Grid, Rect, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};

// State colors for cache lines
const COLOR_INVALID: Color32 = Color32::from_rgb(40, 40, 45);
const COLOR_CLEAN: Color32 = Color32::from_rgb(56, 142, 80); // muted green
const COLOR_DIRTY: Color32 = Color32::from_rgb(204, 120, 40); // amber

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
        let theme = ui_theme();
        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let (l1, l2, l3) = session.cache_snapshots();

        ui.heading("Cache Hierarchy");
        ui.add_space(6.0);

        Frame::NONE
            .fill(theme.panel_alt)
            .stroke(Stroke::new(1.0, theme.border_soft))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.colored_label(theme.success, "Three-Level Cache Hierarchy Active");
                ui.label(
                    RichText::new(hierarchy_summary(&l1.params, &l2.params, &l3.params)).weak(),
                );
            });

        ui.add_space(8.0);

        // Legend
        ui.horizontal(|ui| {
            color_swatch(ui, COLOR_CLEAN);
            ui.label(RichText::new("clean").small());
            ui.add_space(6.0);
            color_swatch(ui, COLOR_DIRTY);
            ui.label(RichText::new("dirty").small());
            ui.add_space(6.0);
            color_swatch(ui, COLOR_INVALID);
            ui.label(RichText::new("invalid").small());
        });

        ui.add_space(8.0);

        ScrollArea::vertical().show(ui, |ui| {
            // L1: full set×way grid, small enough to show every line
            cache_section(ui, "L1", &l1, GridDetail::Full);
            ui.add_space(12.0);
            // L2: per-way utilization bars, 512 sets, compact summary per way
            cache_section(ui, "L2", &l2, GridDetail::WayBars);
            ui.add_space(12.0);
            // L3: aggregate only, 8192 sets is too dense for per-set display
            cache_section(ui, "L3", &l3, GridDetail::Aggregate);
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_bytes(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{}MB", n / (1024 * 1024))
    } else if n >= 1024 {
        format!("{}KB", n / 1024)
    } else {
        format!("{}B", n)
    }
}

fn hierarchy_summary(
    l1: &CacheParamsSnapshot,
    l2: &CacheParamsSnapshot,
    l3: &CacheParamsSnapshot,
) -> String {
    let policy = |p: &CacheParamsSnapshot| if p.write_back { "WB" } else { "WT" };
    format!(
        "L1: {}, {}-way, {} | L2: {}, {}-way, {} | L3: {}, {}-way, {} | {}-byte lines",
        format_bytes(l1.size),
        l1.associativity,
        policy(l1),
        format_bytes(l2.size),
        l2.associativity,
        policy(l2),
        format_bytes(l3.size),
        l3.associativity,
        policy(l3),
        l1.block_size,
    )
}

fn color_swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

// ---------------------------------------------------------------------------
// Per-level section
// ---------------------------------------------------------------------------

enum GridDetail {
    Full,
    WayBars,
    Aggregate,
}

fn cache_section(ui: &mut Ui, label: &str, snap: &CacheSnapshot, detail: GridDetail) {
    let theme = ui_theme();
    let n_sets = snap.sets.len();
    let n_ways = snap.params.associativity;

    ui.label(
        RichText::new(format!(
            "{} Cache, {}, {}-way, {}-byte lines, {} sets",
            label,
            format_bytes(snap.params.size),
            n_ways,
            snap.params.block_size,
            n_sets,
        ))
        .strong(),
    );

    Frame::NONE
        .stroke(Stroke::new(1.0, theme.border_soft))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            stats_block(
                ui,
                &format!("{label}_stats"),
                snap.stats.read_hits,
                snap.stats.read_misses,
                snap.stats.write_hits,
                snap.stats.write_misses,
            );

            ui.add_space(8.0);

            match detail {
                GridDetail::Full => draw_full_grid(ui, snap),
                GridDetail::WayBars => draw_way_bars(ui, snap),
                GridDetail::Aggregate => draw_aggregate(ui, snap),
            }
        });
}

// ---------------------------------------------------------------------------
// L1, full set×way pixel grid
// ---------------------------------------------------------------------------

fn draw_full_grid(ui: &mut Ui, snap: &CacheSnapshot) {
    let n_sets = snap.sets.len();
    let n_ways = snap.params.associativity;

    let gap = 1.0_f32;
    let label_w = 44.0_f32;

    // Scale cells to fit available width; at least 3×3 px per cell
    let avail_for_grid = (ui.available_width() - label_w - 24.0).max(60.0);
    let cell_w = ((avail_for_grid / n_sets as f32) - gap).clamp(3.0, 20.0);
    let cell_h = (cell_w * 0.7).clamp(3.0, 14.0);

    let grid_w = n_sets as f32 * (cell_w + gap) - gap;
    let grid_h = n_ways as f32 * (cell_h + gap) - gap;

    // Axis labels
    ui.horizontal(|ui| {
        ui.add_space(label_w + 4.0);
        ui.label(RichText::new(format!("← {} sets →", n_sets)).small().weak());
    });

    ui.horizontal(|ui| {
        // Way labels column
        ui.vertical(|ui| {
            for way in 0..n_ways {
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(label_w, cell_h + gap), Sense::hover());
                ui.painter().text(
                    r.right_center(),
                    egui::Align2::RIGHT_CENTER,
                    format!("Way {way}"),
                    egui::FontId::proportional(10.0),
                    ui_theme().text_dim,
                );
            }
        });

        ui.add_space(4.0);

        // Grid
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(grid_w, grid_h), Sense::hover());

        let painter = ui.painter();
        for (set_idx, ways) in snap.sets.iter().enumerate() {
            for (way_idx, line) in ways.iter().enumerate() {
                let x = rect.min.x + set_idx as f32 * (cell_w + gap);
                let y = rect.min.y + way_idx as f32 * (cell_h + gap);
                let cell = Rect::from_min_size(egui::pos2(x, y), Vec2::new(cell_w, cell_h));
                painter.rect_filled(cell, 1.5, line_color(line.valid, line.dirty));
            }
        }

        // Tooltip: find hovered cell
        if resp.hovered() {
            if let Some(pos) = resp.hover_pos() {
                let col = ((pos.x - rect.min.x) / (cell_w + gap)).floor() as usize;
                let row = ((pos.y - rect.min.y) / (cell_h + gap)).floor() as usize;
                if col < n_sets && row < n_ways {
                    let line = &snap.sets[col][row];
                    let desc = if !line.valid {
                        "invalid".to_string()
                    } else if line.dirty {
                        format!("dirty  tag={:#010x}", line.tag)
                    } else {
                        format!("clean  tag={:#010x}", line.tag)
                    };
                    resp.on_hover_text(format!("Set {:3}  Way {}  {}", col, row, desc));
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// L2, per-way utilization bars
// ---------------------------------------------------------------------------

fn draw_way_bars(ui: &mut Ui, snap: &CacheSnapshot) {
    let theme = ui_theme();
    let n_sets = snap.sets.len();
    let n_ways = snap.params.associativity;

    // For each way, count valid and dirty lines across all sets
    let mut valid_counts = vec![0usize; n_ways];
    let mut dirty_counts = vec![0usize; n_ways];
    for set in &snap.sets {
        for (way_idx, line) in set.iter().enumerate() {
            if line.valid {
                valid_counts[way_idx] += 1;
                if line.dirty {
                    dirty_counts[way_idx] += 1;
                }
            }
        }
    }

    let bar_w = (ui.available_width() - 180.0).max(60.0);
    let bar_h = 8.0_f32;

    Grid::new("l2_way_bars")
        .num_columns(4)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Way").small().color(theme.text_dim));
            ui.label(RichText::new("Valid").small().color(theme.text_dim));
            ui.label(RichText::new("Dirty").small().color(theme.text_dim));
            ui.label(RichText::new("Fill").small().color(theme.text_dim));
            ui.end_row();

            for way in 0..n_ways {
                let valid = valid_counts[way];
                let dirty = dirty_counts[way];
                let fill_frac = valid as f32 / n_sets as f32;
                let dirty_frac = dirty as f32 / n_sets as f32;

                ui.label(RichText::new(format!("{way}")).monospace().small());
                ui.label(RichText::new(format!("{valid}")).monospace().small());
                ui.label(RichText::new(format!("{dirty}")).monospace().small());

                // Stacked bar: valid (green) + dirty overlay (amber) + empty (dark)
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, bar_h), Sense::hover());
                    let painter = ui.painter();

                    // background
                    painter.rect_filled(rect, 1.0, COLOR_INVALID);

                    // valid portion
                    let valid_rect =
                        Rect::from_min_size(rect.min, Vec2::new(bar_w * fill_frac, bar_h));
                    painter.rect_filled(valid_rect, 1.0, COLOR_CLEAN);

                    // dirty portion (overlay from left edge of valid)
                    if dirty > 0 {
                        let dirty_rect =
                            Rect::from_min_size(rect.min, Vec2::new(bar_w * dirty_frac, bar_h));
                        painter.rect_filled(dirty_rect, 1.0, COLOR_DIRTY);
                    }

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{:.0}%", fill_frac * 100.0))
                            .monospace()
                            .small(),
                    );
                });

                ui.end_row();
            }
        });
}

// ---------------------------------------------------------------------------
// L3, aggregate summary
// ---------------------------------------------------------------------------

fn draw_aggregate(ui: &mut Ui, snap: &CacheSnapshot) {
    let theme = ui_theme();
    let total: usize = snap.sets.iter().map(|s| s.len()).sum();
    let valid: usize = snap
        .sets
        .iter()
        .flat_map(|s| s.iter())
        .filter(|l| l.valid)
        .count();
    let dirty: usize = snap
        .sets
        .iter()
        .flat_map(|s| s.iter())
        .filter(|l| l.valid && l.dirty)
        .count();

    let valid_frac = if total > 0 {
        valid as f32 / total as f32
    } else {
        0.0
    };
    let dirty_frac = if total > 0 {
        dirty as f32 / total as f32
    } else {
        0.0
    };

    let bar_w = (ui.available_width() - 100.0).max(60.0);
    let bar_h = 10.0_f32;

    ui.label(
        RichText::new(format!(
            "{} total lines ({} sets x {} ways)",
            total,
            snap.sets.len(),
            snap.params.associativity
        ))
        .small()
        .color(theme.text_dim),
    );
    ui.add_space(4.0);

    for (label, frac, count, color) in [
        ("Occupied", valid_frac, valid, COLOR_CLEAN),
        ("Dirty   ", dirty_frac, dirty, COLOR_DIRTY),
    ] {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).monospace().small());
            let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, bar_h), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, COLOR_INVALID);
            let fill = Rect::from_min_size(rect.min, Vec2::new(bar_w * frac, bar_h));
            painter.rect_filled(fill, 2.0, color);
            ui.label(
                RichText::new(format!("{count} / {total}  ({:.1}%)", frac * 100.0))
                    .monospace()
                    .small(),
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Stats block (shared)
// ---------------------------------------------------------------------------

fn stats_block(
    ui: &mut Ui,
    id: &str,
    read_hits: u64,
    read_misses: u64,
    write_hits: u64,
    write_misses: u64,
) {
    let theme = ui_theme();
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

    Grid::new(id)
        .num_columns(3)
        .spacing([20.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("").monospace());
            ui.label(RichText::new("Hits").monospace().color(theme.text_dim));
            ui.label(RichText::new("Misses").monospace().color(theme.text_dim));
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
}

fn hit_rate_bar(ui: &mut Ui, label: &str, pct: f64) {
    ui.vertical(|ui| {
        let theme = ui_theme();
        ui.label(RichText::new(label).small().color(theme.text_dim));
        let bar_w = 120.0;
        let bar_h = 8.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, bar_h), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, theme.surface_alt);
        let fill_w = bar_w * (pct as f32 / 100.0).clamp(0.0, 1.0);
        let fill = Rect::from_min_size(rect.min, Vec2::new(fill_w, bar_h));
        let color = if pct >= 90.0 {
            theme.success
        } else if pct >= 60.0 {
            theme.warning
        } else {
            theme.error
        };
        ui.painter().rect_filled(fill, 2.0, color);
        ui.label(RichText::new(format!("{pct:.1}%")).monospace().small());
    });
}

fn line_color(valid: bool, dirty: bool) -> Color32 {
    if !valid {
        COLOR_INVALID
    } else if dirty {
        COLOR_DIRTY
    } else {
        COLOR_CLEAN
    }
}
