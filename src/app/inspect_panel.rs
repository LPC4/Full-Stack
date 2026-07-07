//! The Inspect tool window: live CPU/OS state for the resident machine session.
//! Registers/pipeline/caches work for every run; process/FS/trace are kernel-only.

use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::inspect::{fs_view, os_view};
use full_stack::session::{MachineState, RegisterSnapshot};
use virtual_machine::cpu::pipeline::StageEntry;
use virtual_machine::memory::cache::CacheSnapshot;

/// Minimum seconds between probe captures while the machine is running.
const RUNNING_CAPTURE_INTERVAL: f64 = 0.25;
/// Preview cap for guest FS files, in bytes.
const FS_PREVIEW_CAP: usize = 4096;
/// Pipeline stage names, in order (`pipeline_snapshot().stages[0..5]`).
const STAGE_NAMES: [&str; 5] = ["IF", "ID", "EX", "MEM", "WB"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InspectTab {
    Cpu,
    Pipeline,
    Caches,
    Proc,
    Fs,
    Trace,
}

/// Throttled snapshot of the kernel probes, refreshed as the machine steps.
pub(crate) struct InspectCache {
    steps: u64,
    time: f64,
    processes: Vec<os_view::ProcessInfo>,
    fs_root: Option<fs_view::FsNode>,
    trace: Vec<os_view::TraceEntry>,
}

impl InspectCache {
    /// Live process count, shown in the machine dock for kernel runs.
    pub(super) fn process_count(&self) -> usize {
        self.processes.len()
    }
}

/// Shared throttle for the step-driven probes: recapture when there is no prior
/// snapshot, or the step count advanced and we are paused or past the interval.
fn capture_due(last: Option<(u64, f64)>, steps: u64, running: bool, now: f64) -> bool {
    match last {
        None => true,
        Some((last_steps, last_time)) => {
            last_steps != steps && (!running || now - last_time > RUNNING_CAPTURE_INTERVAL)
        }
    }
}

/// A throttled cache-occupancy snapshot for the Caches tab. L1 keeps its full
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
    pub(super) fn inspect_panel(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Inspect");
        let kernel = self
            .live
            .as_ref()
            .is_some_and(|live| live.os_symbols().is_some());
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Cpu, "cpu");
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Pipeline, "pipe");
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Caches, "cache");
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Proc, "proc");
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Fs, "fs");
            inspect_tab_button(ui, &mut self.inspect_tab, InspectTab::Trace, "trace");
        });
        ui.add_space(metrics::SECTION_GAP);
        chrome::hairline(ui);
        ui.add_space(metrics::SECTION_GAP);

        match self.inspect_tab {
            InspectTab::Cpu => self.registers_tab(ui),
            InspectTab::Pipeline => self.pipeline_tab(ui),
            InspectTab::Caches => self.caches_tab(ui),
            InspectTab::Proc | InspectTab::Fs | InspectTab::Trace if !kernel => {
                chrome::indented(ui, |ui| {
                    ui.label(theme::mono(
                        "Kernel runs only.\nBoot a kernel config to inspect OS state.",
                        metrics::STATUS_TEXT_SIZE,
                        theme::TEXT_FAINT,
                    ));
                });
            }
            InspectTab::Proc => self.processes_tab(ui),
            InspectTab::Fs => self.guest_fs_tab(ui),
            InspectTab::Trace => self.trace_tab(ui),
        }
    }

    /// Re-run the kernel probes when the machine has stepped since the last
    /// capture: immediately while paused, throttled while running.
    pub(super) fn refresh_inspect_cache(&mut self, now: f64) {
        let Some(live) = &self.live else {
            self.inspect_cache = None;
            return;
        };
        let due = capture_due(
            self.inspect_cache.as_ref().map(|c| (c.steps, c.time)),
            live.steps,
            live.state == MachineState::Running,
            now,
        );
        if !due {
            return;
        }
        let vm = live.vm();
        self.inspect_cache = Some(InspectCache {
            steps: live.steps,
            time: now,
            processes: live
                .os_symbols()
                .map(|sym| os_view::capture(vm, sym))
                .unwrap_or_default(),
            fs_root: live.fs_symbols().and_then(|sym| fs_view::capture(vm, sym)),
            trace: live
                .os_symbols()
                .map(|sym| os_view::capture_trace(vm, sym))
                .unwrap_or_default(),
        });
    }

    /// The register file: live values while a session exists (with changed-by-
    /// last-step highlighting), else the post-mortem snapshot of the last run.
    fn registers_tab(&self, ui: &mut egui::Ui) {
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

    /// Scheduler snapshot: one row per live PCB; selecting one unfolds its
    /// details and a stack walk through that process's own page table.
    fn processes_tab(&mut self, ui: &mut egui::Ui) {
        let Some(cache) = &self.inspect_cache else {
            chrome::centered_hint(ui, "No capture yet.");
            return;
        };
        if cache.processes.is_empty() {
            chrome::centered_hint(ui, "No live processes.");
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("proc_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for process in &cache.processes {
                    let selected = self.inspect_pid == Some(process.pid);
                    if process_row(ui, process, selected).clicked() {
                        clicked = Some(process.pid);
                    }
                    if selected {
                        self.process_detail(ui, process);
                    }
                }
            });
        if let Some(pid) = clicked {
            self.inspect_pid = (self.inspect_pid != Some(pid)).then_some(pid);
        }
    }

    fn process_detail(&self, ui: &mut egui::Ui, process: &os_view::ProcessInfo) {
        chrome::indented(ui, |ui| {
            ui.add_space(metrics::SECTION_GAP);
            kv_line(ui, "parent", &process.parent.to_string());
            kv_line(ui, "pcb", &format!("{:#x}", process.pcb_pa));
            kv_line(ui, "page root", &format!("{:#x}", process.page_root));
            kv_line(ui, "sp", &format!("{:#x}", process.sp));
            if process.role == os_view::Role::Zombie {
                kv_line(ui, "exit", &process.exit_code.to_string());
            }
            if process.fb_mapped {
                kv_line(ui, "framebuffer", "mapped");
            }
            ui.add_space(metrics::SECTION_GAP);
        });
        let Some(live) = &self.live else {
            return;
        };
        let stack = os_view::capture_stack(live.vm(), process);
        if stack.is_empty() {
            return;
        }
        chrome::indented(ui, |ui| {
            ui.label(theme::label("stack"));
        });
        for word in stack.iter().take(16) {
            let text = if word.mapped {
                format!("{:#012x}  {:016x}", word.va, word.value)
            } else {
                format!("{:#012x}  (unmapped)", word.va)
            };
            let color = if word.mapped {
                theme::TEXT_DIM
            } else {
                theme::TEXT_FAINT
            };
            mono_row(ui, &text, color);
        }
        ui.add_space(metrics::SECTION_GAP);
    }

    /// The guest filesystem tree; selecting a file shows its head as text.
    fn guest_fs_tab(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self.inspect_cache.as_ref().and_then(|c| c.fs_root.as_ref()) else {
            chrome::centered_hint(ui, "Guest FS is not readable yet.");
            return;
        };
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("guest_fs_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                fs_node_rows(ui, root, 0, self.inspect_inode, &mut clicked);
                if let Some(inode) = self.inspect_inode {
                    self.fs_preview(ui, inode);
                }
            });
        if let Some(inode) = clicked {
            self.inspect_inode = (self.inspect_inode != Some(inode)).then_some(inode);
        }
    }

    fn fs_preview(&self, ui: &mut egui::Ui, inode: u64) {
        let Some(live) = &self.live else {
            return;
        };
        let (Some(sym), Some(root)) = (
            live.fs_symbols(),
            self.inspect_cache.as_ref().and_then(|c| c.fs_root.as_ref()),
        ) else {
            return;
        };
        let Some(node) = find_fs_node(root, inode) else {
            return;
        };
        let bytes = fs_view::file_preview(live.vm(), sym, node);
        ui.add_space(metrics::SECTION_GAP);
        chrome::hairline(ui);
        chrome::indented(ui, |ui| {
            ui.add_space(metrics::SECTION_GAP);
            ui.label(theme::label(&node.name));
            let shown = &bytes[..bytes.len().min(FS_PREVIEW_CAP)];
            let text = match std::str::from_utf8(shown) {
                Ok(text) => text.to_owned(),
                Err(_) => format!("(binary, {} bytes)", node.size),
            };
            ui.label(theme::mono(
                text,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
            if bytes.len() > FS_PREVIEW_CAP {
                ui.label(theme::mono(
                    format!("... {} more byte(s)", bytes.len() - FS_PREVIEW_CAP),
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            }
        });
    }

    /// The syscall trace ring, newest first.
    fn trace_tab(&self, ui: &mut egui::Ui) {
        let Some(cache) = &self.inspect_cache else {
            chrome::centered_hint(ui, "No capture yet.");
            return;
        };
        if cache.trace.is_empty() {
            chrome::centered_hint(ui, "No syscalls traced yet.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("trace_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in &cache.trace {
                    trace_row(ui, entry);
                }
            });
    }

    /// The 5-stage pipeline feed plus cumulative performance counters. Reads the
    /// resident VM directly each frame; both accessors are cheap and alloc-free.
    fn pipeline_tab(&self, ui: &mut egui::Ui) {
        let Some(live) = &self.live else {
            chrome::indented(ui, |ui| {
                ui.label(theme::mono(
                    "No run yet.\nPress Run to watch the pipeline.",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            });
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
    fn caches_tab(&self, ui: &mut egui::Ui) {
        if self.live.is_none() {
            chrome::centered_hint(ui, "No run yet.");
            return;
        }
        let Some(grid) = &self.cache_grid else {
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

    /// Recompute the cache occupancy grid, only while the Caches tab is showing
    /// (`get_cache_snapshots` clones every line). Throttled running, exact paused.
    pub(super) fn refresh_cache_grid(&mut self, now: f64) {
        if self.inspect_tab != InspectTab::Caches {
            return;
        }
        let Some(live) = &self.live else {
            self.cache_grid = None;
            return;
        };
        let due = capture_due(
            self.cache_grid.as_ref().map(|g| (g.steps, g.time)),
            live.steps,
            live.state == MachineState::Running,
            now,
        );
        if !due {
            return;
        }
        let (l1, l2, l3) = live.vm().get_cache_snapshots();
        self.cache_grid = Some(CacheGrid {
            steps: live.steps,
            time: now,
            levels: [
                CacheLevelGrid::from_snapshot("L1", &l1, true),
                CacheLevelGrid::from_snapshot("L2", &l2, false),
                CacheLevelGrid::from_snapshot("L3", &l3, false),
            ],
        });
    }
}

/// A left-inset section eyebrow, matching the panel header style.
fn section_label(ui: &mut egui::Ui, text: &str) {
    chrome::indented(ui, |ui| {
        ui.label(theme::label(text));
    });
}

/// A two-column mono row: dim key on the left, value on the right.
fn stat_row(ui: &mut egui::Ui, key: &str, value: &str) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        key,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::TEXT_DIM,
    );
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::TEXT,
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

/// Percentage of `num`/`den` as text, or a dash when there are no samples.
fn rate_str(num: u64, den: u64) -> String {
    if den == 0 {
        "--".to_owned()
    } else {
        format!("{:.1}%", num as f64 / den as f64 * 100.0)
    }
}

/// A count with its share of `den` appended, e.g. `123 (4.5%)`.
fn percent(num: u64, den: u64, label: &str) -> String {
    if den == 0 {
        label.to_owned()
    } else {
        format!("{label} ({:.1}%)", num as f64 / den as f64 * 100.0)
    }
}

/// Cache size in binary units: `4 KiB`, `256 KiB`, `8 MiB`.
fn cache_bytes(n: usize) -> String {
    if n >= 1 << 20 {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 10 {
        format!("{} KiB", n >> 10)
    } else {
        format!("{n} B")
    }
}

fn inspect_tab_button(ui: &mut egui::Ui, active: &mut InspectTab, tab: InspectTab, label: &str) {
    let color = if *active == tab {
        theme::ELECTRIC
    } else {
        theme::TEXT_DIM
    };
    let resp = ui
        .add(egui::Button::new(theme::mono(label, metrics::STATUS_TEXT_SIZE, color)).frame(false));
    if resp.clicked() {
        *active = tab;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
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

fn process_row(
    ui: &mut egui::Ui,
    process: &os_view::ProcessInfo,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        format!("pid {}", process.pid),
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        if selected {
            theme::ELECTRIC
        } else {
            theme::TEXT
        },
    );
    let (role, color) = role_style(process.role);
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        role,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        color,
    );
    resp
}

fn role_style(role: os_view::Role) -> (&'static str, egui::Color32) {
    match role {
        os_view::Role::Running => ("running", theme::ELECTRIC),
        os_view::Role::Ready => ("ready", theme::TEXT_DIM),
        os_view::Role::Waiting => ("waiting", theme::WARNING),
        os_view::Role::Zombie => ("zombie", theme::TEXT_FAINT),
    }
}

fn fs_node_rows(
    ui: &mut egui::Ui,
    node: &fs_view::FsNode,
    depth: usize,
    selected: Option<u64>,
    clicked: &mut Option<u64>,
) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    let is_selected = !node.is_dir && selected == Some(node.inode);
    if is_selected {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
    } else if resp.hovered() && !node.is_dir {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let indent = metrics::PANEL_INSET_X + depth as f32 * metrics::TREE_INDENT;
    let (label, color) = if node.is_dir {
        (format!("{}/", node.name), theme::TEXT_DIM)
    } else {
        (
            node.name.clone(),
            if is_selected {
                theme::ELECTRIC
            } else {
                theme::TEXT
            },
        )
    };
    painter.text(
        pos2(rect.left() + indent, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    if !node.is_dir {
        let mut trailer = human_size(node.size);
        if node.open_fds > 0 {
            trailer = format!("{} fd:{}", trailer, node.open_fds);
        }
        painter.text(
            pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
            Align2::RIGHT_CENTER,
            trailer,
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            theme::TEXT_FAINT,
        );
        if resp.clicked() {
            *clicked = Some(node.inode);
        }
    }
    for child in &node.children {
        fs_node_rows(ui, child, depth + 1, selected, clicked);
    }
}

fn find_fs_node(node: &fs_view::FsNode, inode: u64) -> Option<&fs_view::FsNode> {
    if node.inode == inode && !node.is_dir {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_fs_node(child, inode))
}

fn trace_row(ui: &mut egui::Ui, entry: &os_view::TraceEntry) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{} p{} {}", entry.seq, entry.pid, syscall_name(entry.num)),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT,
    );
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{:x} -> {:x}", entry.a0, entry.ret),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
    resp.on_hover_text(format!(
        "seq {}  pid {}  {} ({})\na0  {:#x}\nret {:#x}",
        entry.seq,
        entry.pid,
        syscall_name(entry.num),
        entry.num,
        entry.a0,
        entry.ret
    ));
}

fn kv_line(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(theme::label(key));
        ui.label(theme::mono(
            value.to_owned(),
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_DIM,
        ));
    });
}

fn mono_row(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    ui.painter_at(rect).text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        color,
    );
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}k", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

/// Names for the kernel's syscall numbers (core/syscall.hll); raw number otherwise.
fn syscall_name(num: u64) -> String {
    let name = match num {
        2 => "yield",
        46 => "ftruncate",
        56 => "open",
        57 => "close",
        62 => "lseek",
        63 => "read",
        64 => "write",
        82 => "rename",
        83 => "mkdir",
        93 => "exit",
        100 => "readchar",
        101 => "readdir",
        102 => "stat",
        103 => "exec",
        104 => "pidalive",
        105 => "unlink",
        106 => "rmdir",
        107 => "mapfb",
        108 => "pollkey",
        110 => "exec_redir",
        129 => "kill",
        214 => "brk",
        220 => "fork",
        260 => "wait",
        261 => "waitpid",
        _ => return format!("sys{num}"),
    };
    name.to_owned()
}
