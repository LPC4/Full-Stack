//! VM-backed panes: the register file, the pipeline feed, the cache grid, and
//! compact bridges to the machine rung's memory and disassembly views.

use super::widgets::{cache_bytes, capture_due, percent, rate_str, section_label, stat_row};
use super::{FullStackApp, InspectorPane};
use crate::app::{chrome, debug_view, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::session::{MachineState, RegisterSnapshot};
use virtual_machine::cpu::pipeline::StageEntry;
use virtual_machine::memory::cache::CacheSnapshot;

/// Pipeline stage names, in order (`pipeline_snapshot().stages[0..5]`).
const STAGE_NAMES: [&str; 5] = ["IF", "ID", "EX", "MEM", "WB"];

/// A throttled cache-occupancy snapshot for the Caches pane. L1 keeps its full
/// per-line grid (small); L2/L3 keep aggregate counts only.
pub(crate) struct CacheGrid {
    steps: u64,
    time: f64,
    levels: [CacheLevelGrid; 3],
}

/// Occupancy and stats for one cache level, derived from a `CacheSnapshot`.
struct CacheLevelGrid {
    name: &'static str,
    size: usize,
    block: usize,
    assoc: usize,
    sets: usize,
    valid: usize,
    dirty: usize,
    total: usize,
    read_hits: u64,
    read_misses: u64,
    write_hits: u64,
    write_misses: u64,
    /// One cell per line (set-major), kept only for the small L1.
    grid: Option<Vec<LineCell>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineCell {
    Empty,
    Clean,
    Dirty,
}

impl CacheLevelGrid {
    fn from_snapshot(name: &'static str, snap: &CacheSnapshot, with_grid: bool) -> Self {
        let mut valid = 0;
        let mut dirty = 0;
        let mut total = 0;
        let mut grid = with_grid.then(Vec::new);
        for set in &snap.sets {
            for line in set {
                total += 1;
                let cell = if !line.valid {
                    LineCell::Empty
                } else if line.dirty {
                    valid += 1;
                    dirty += 1;
                    LineCell::Dirty
                } else {
                    valid += 1;
                    LineCell::Clean
                };
                if let Some(cells) = grid.as_mut() {
                    cells.push(cell);
                }
            }
        }
        Self {
            name,
            size: snap.params.size,
            block: snap.params.block_size,
            assoc: snap.params.associativity,
            sets: snap.sets.len(),
            valid,
            dirty,
            total,
            read_hits: snap.stats.read_hits,
            read_misses: snap.stats.read_misses,
            write_hits: snap.stats.write_hits,
            write_misses: snap.stats.write_misses,
            grid,
        }
    }

    fn accesses(&self) -> u64 {
        self.read_hits + self.read_misses + self.write_hits + self.write_misses
    }

    fn hits(&self) -> u64 {
        self.read_hits + self.write_hits
    }
}

impl FullStackApp {
    /// The register file: live values while a session exists (with changed-by-
    /// last-step highlighting), else the post-mortem snapshot of the last run.
    pub(super) fn cpu_pane(&self, ui: &mut egui::Ui) {
        let (regs, prev) = match &self.live {
            Some(live) => (Some(live.registers()), live.prev_step_registers().cloned()),
            None => (
                self.vm_result.as_ref().and_then(|vm| vm.registers.clone()),
                None,
            ),
        };
        let Some(regs) = regs else {
            chrome::indented(ui, |ui| {
                ui.label(theme::mono(
                    "No run yet.\nPress Run to capture CPU state.",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            });
            return;
        };

        egui::ScrollArea::vertical()
            .id_salt("inspect_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let pc_changed = prev.as_ref().is_some_and(|p| p.pc != regs.pc);
                register_row(ui, "pc", regs.pc, true, pc_changed);
                ui.add_space(metrics::SECTION_GAP);
                chrome::hairline(ui);
                ui.add_space(metrics::SECTION_GAP);
                for (i, value) in regs.x.iter().enumerate() {
                    let name = RegisterSnapshot::ABI_NAMES[i];
                    let changed = prev.as_ref().is_some_and(|p| p.x[i] != *value);
                    // x0 is hardwired zero; keep it dim so live values stand out.
                    register_row(ui, name, *value, i != 0 && *value != 0, changed);
                }
            });
    }

    /// The 5-stage pipeline feed plus cumulative performance counters. Reads the
    /// resident VM directly each frame; both accessors are cheap and alloc-free.
    pub(super) fn pipeline_pane(&self, ui: &mut egui::Ui) {
        let Some(live) = &self.live else {
            chrome::centered_hint(ui, "No machine session.");
            return;
        };
        let vm = live.vm();
        let feed = vm.pipeline_snapshot().clone();
        let stats = vm.pipeline_stats().clone();
        egui::ScrollArea::vertical()
            .id_salt("pipe_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                section_label(ui, "stages");
                for (i, name) in STAGE_NAMES.iter().enumerate() {
                    stage_row(ui, name, feed.stages[i]);
                }
                let hazard = if feed.stalled {
                    ("stalled: load-use", theme::WARNING)
                } else if feed.flushed {
                    ("flushed: mispredict", theme::WARNING)
                } else {
                    ("flowing", theme::TEXT_DIM)
                };
                chrome::indented(ui, |ui| {
                    ui.add_space(metrics::SECTION_GAP);
                    ui.label(theme::mono(hazard.0, metrics::STATUS_TEXT_SIZE, hazard.1));
                });

                ui.add_space(metrics::SECTION_GAP);
                chrome::hairline(ui);
                ui.add_space(metrics::SECTION_GAP);
                section_label(ui, "counters");
                let cycles = stats.cycles.max(1) as f64;
                stat_row(ui, "cycles", &stats.cycles.to_string());
                stat_row(ui, "retired", &stats.insns_retired.to_string());
                stat_row(
                    ui,
                    "ipc",
                    &format!("{:.3}", stats.insns_retired as f64 / cycles),
                );
                stat_row(
                    ui,
                    "stall",
                    &percent(
                        stats.stall_cycles,
                        stats.cycles,
                        &stats.stall_cycles.to_string(),
                    ),
                );
                stat_row(
                    ui,
                    "flush",
                    &percent(
                        stats.flush_cycles,
                        stats.cycles,
                        &stats.flush_cycles.to_string(),
                    ),
                );

                ui.add_space(metrics::SECTION_GAP);
                chrome::hairline(ui);
                ui.add_space(metrics::SECTION_GAP);
                section_label(ui, "branches");
                stat_row(ui, "seen", &stats.branches_seen.to_string());
                let correct = stats
                    .branches_seen
                    .saturating_sub(stats.branches_mispredicted);
                stat_row(
                    ui,
                    "predicted",
                    &percent(correct, stats.branches_seen, &correct.to_string()),
                );
                stat_row(ui, "mispredicted", &stats.branches_mispredicted.to_string());
            });
    }

    /// Cache hierarchy occupancy and hit rates, from the throttled `cache_grid`
    /// snapshot (`refresh_cache_grid`). L1 shows a per-line cell map.
    pub(super) fn caches_pane(&self, ui: &mut egui::Ui) {
        if self.live.is_none() {
            chrome::centered_hint(ui, "No machine session.");
            return;
        }
        let Some(grid) = &self.inspector.cache_grid else {
            chrome::centered_hint(ui, "Reading cache state...");
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("cache_scroll")
            .show(ui, |ui| {
                for (i, level) in grid.levels.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(metrics::SECTION_GAP);
                        chrome::hairline(ui);
                        ui.add_space(metrics::SECTION_GAP);
                    }
                    cache_level_view(ui, level);
                }
            });
    }

    /// Recompute the cache occupancy grid, only while the Caches pane is showing
    /// (`get_cache_snapshots` clones every line). Throttled running, exact paused.
    pub(in crate::app) fn refresh_cache_grid(&mut self, now: f64) {
        if self.inspector.pane != InspectorPane::Caches {
            return;
        }
        let Some(live) = &self.live else {
            self.inspector.cache_grid = None;
            return;
        };
        let due = capture_due(
            self.inspector
                .cache_grid
                .as_ref()
                .map(|g| (g.steps, g.time)),
            live.steps,
            live.state == MachineState::Running,
            now,
        );
        if !due {
            return;
        }
        let (l1, l2, l3) = live.vm().get_cache_snapshots();
        self.inspector.cache_grid = Some(CacheGrid {
            steps: live.steps,
            time: now,
            levels: [
                CacheLevelGrid::from_snapshot("L1", &l1, true),
                CacheLevelGrid::from_snapshot("L2", &l2, false),
                CacheLevelGrid::from_snapshot("L3", &l3, false),
            ],
        });
    }

    /// Bridge pane: a summary plus a jump to the machine rung's memory view,
    /// which needs more width than the left tool window offers.
    pub(super) fn memory_pane(&mut self, ui: &mut egui::Ui) {
        let Some(live) = &self.live else {
            chrome::centered_hint(ui, "No machine session.");
            return;
        };
        let regs = live.registers();
        ui.spacing_mut().item_spacing.y = 0.0;
        stat_row(ui, "pc", &format!("{:016x}", regs.pc));
        stat_row(ui, "sp", &format!("{:016x}", regs.x[2]));
        let translation = match live.translate(regs.x[2]) {
            Some(pa) => format!("sp \u{2192} pa {pa:#x}"),
            None => "sp \u{2192} unmapped".to_owned(),
        };
        stat_row(ui, "mmu", &translation);
        self.machine_rung_bridge(ui, "open memory view", debug_view::MachineView::Memory);
    }

    /// Bridge pane: pc context plus a jump to the machine rung's disassembly.
    pub(super) fn disassembly_pane(&mut self, ui: &mut egui::Ui) {
        let Some(live) = &self.live else {
            chrome::centered_hint(ui, "No machine session.");
            return;
        };
        let regs = live.registers();
        ui.spacing_mut().item_spacing.y = 0.0;
        stat_row(ui, "pc", &format!("{:016x}", regs.pc));
        if let Some((name, offset)) = live.symbol_near(regs.pc) {
            let tag = if offset == 0 {
                name.to_owned()
            } else {
                format!("{name}+{offset:#x}")
            };
            stat_row(ui, "symbol", &tag);
        }
        if let Some((file, line)) = live.source_for_pc(regs.pc) {
            stat_row(ui, "source", &format!("{file}:{line}"));
        }
        self.machine_rung_bridge(ui, "open disassembly", debug_view::MachineView::Disasm);
    }

    /// The shared footer of the bridge panes: why the view lives on the machine
    /// rung, and the button that focuses it there.
    fn machine_rung_bridge(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        view: debug_view::MachineView,
    ) {
        ui.add_space(metrics::SECTION_GAP);
        chrome::indented(ui, |ui| {
            if chrome::action_row(ui, &[(label, chrome::ActionTone::Primary)]).is_some() {
                self.machine_view = view;
                self.focus_machine_tab();
            }
        });
    }
}

/// One register row: ABI name (left) and 64-bit hex value (right). `live` picks a
/// bright value color; `changed` marks registers the last step wrote.
fn register_row(ui: &mut egui::Ui, name: &str, value: u64, live: bool, changed: bool) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    if changed {
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::TEXT_DIM,
    );
    let value_color = if changed {
        theme::ELECTRIC
    } else if live {
        theme::TEXT
    } else {
        theme::TEXT_FAINT
    };
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{value:016x}"),
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        value_color,
    );
}

/// One pipeline stage: name, then the in-flight instruction or a bubble marker.
fn stage_row(ui: &mut egui::Ui, name: &str, entry: StageEntry) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let (text, color) = match entry {
        Some((pc, mnemonic)) => (format!("{pc:#010x}  {mnemonic}"), theme::TEXT),
        None => ("bubble".to_owned(), theme::TEXT_FAINT),
    };
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::ELECTRIC,
    );
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        color,
    );
}

/// One cache level: geometry, hit rate, read/write splits, then occupancy.
fn cache_level_view(ui: &mut egui::Ui, level: &CacheLevelGrid) {
    chrome::indented(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(theme::mono(
                level.name,
                metrics::LABEL_TEXT_SIZE,
                theme::ELECTRIC,
            ));
            ui.label(theme::mono(
                format!(
                    "{} - {}B x {}-way - {} sets",
                    cache_bytes(level.size),
                    level.block,
                    level.assoc,
                    level.sets
                ),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
        });
    });
    ui.add_space(metrics::SECTION_GAP);
    let accesses = level.accesses();
    stat_row(ui, "hit rate", &rate_str(level.hits(), accesses));
    stat_row(ui, "accesses", &accesses.to_string());
    stat_row(
        ui,
        "reads h/m",
        &format!("{} / {}", level.read_hits, level.read_misses),
    );
    stat_row(
        ui,
        "writes h/m",
        &format!("{} / {}", level.write_hits, level.write_misses),
    );
    stat_row(
        ui,
        "valid",
        &format!("{}/{}  ({} dirty)", level.valid, level.total, level.dirty),
    );
    occupancy_bar(ui, level);
    if let Some(cells) = &level.grid {
        line_grid(ui, cells);
    }
}

/// A thin bar: clean lines in electric, dirty in amber, over a hairline track.
fn occupancy_bar(ui: &mut egui::Ui, level: &CacheLevelGrid) {
    const BAR_H: f32 = 8.0;
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(vec2(width, BAR_H + metrics::SECTION_GAP), Sense::hover());
    let painter = ui.painter_at(rect);
    let track = egui::Rect::from_min_size(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.top()),
        vec2(width - 2.0 * metrics::PANEL_INSET_X, BAR_H),
    );
    painter.rect_filled(track, 0.0, theme::HAIRLINE_SOFT);
    if level.total == 0 {
        return;
    }
    let clean = level.valid.saturating_sub(level.dirty) as f32 / level.total as f32;
    let dirty = level.dirty as f32 / level.total as f32;
    let clean_w = track.width() * clean;
    let dirty_w = track.width() * dirty;
    painter.rect_filled(
        egui::Rect::from_min_size(track.min, vec2(clean_w, BAR_H)),
        0.0,
        theme::ELECTRIC,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            pos2(track.left() + clean_w, track.top()),
            vec2(dirty_w, BAR_H),
        ),
        0.0,
        theme::WARNING,
    );
}

/// A wrapped map of L1 lines: empty (track), clean (electric), dirty (amber).
fn line_grid(ui: &mut egui::Ui, cells: &[LineCell]) {
    const CELL: f32 = 9.0;
    const GAP: f32 = 2.0;
    let width = ui.available_width();
    let usable = (width - 2.0 * metrics::PANEL_INSET_X).max(CELL);
    let per_row = ((usable + GAP) / (CELL + GAP)).floor().max(1.0) as usize;
    let rows = cells.len().div_ceil(per_row);
    let (rect, _) = ui.allocate_exact_size(
        vec2(width, rows as f32 * (CELL + GAP) + metrics::SECTION_GAP),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    for (i, cell) in cells.iter().enumerate() {
        let col = i % per_row;
        let row = i / per_row;
        let x = rect.left() + metrics::PANEL_INSET_X + col as f32 * (CELL + GAP);
        let y = rect.top() + row as f32 * (CELL + GAP);
        let color = match cell {
            LineCell::Empty => theme::HAIRLINE_SOFT,
            LineCell::Clean => theme::ELECTRIC,
            LineCell::Dirty => theme::WARNING,
        };
        painter.rect_filled(
            egui::Rect::from_min_size(pos2(x, y), vec2(CELL, CELL)),
            0.0,
            color,
        );
    }
}
