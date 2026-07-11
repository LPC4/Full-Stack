//! The Inspect tool window: a capability-driven runtime and artifact inspector.
//! Universal panes always render; machine/kernel panes appear only when backed.

mod build_info;
mod kernel;
mod machine;
mod overview;
mod widgets;

pub(crate) use kernel::InspectCache;
pub(crate) use machine::CacheGrid;

use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::compilation_pipeline::TargetMode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InspectorPane {
    Overview,
    Diagnostics,
    Artifacts,
    Symbols,
    Cpu,
    Pipeline,
    Memory,
    Disassembly,
    Caches,
    Processes,
    GuestFs,
    Trace,
}

impl InspectorPane {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Diagnostics => "Diagnostics",
            Self::Artifacts => "Artifacts",
            Self::Symbols => "Symbols",
            Self::Cpu => "CPU",
            Self::Pipeline => "Pipeline",
            Self::Memory => "Memory",
            Self::Disassembly => "Disassembly",
            Self::Caches => "Caches",
            Self::Processes => "Processes",
            Self::GuestFs => "Guest FS",
            Self::Trace => "Trace",
        }
    }
}

/// Selector groups, rendered as separate sections. The `general` group carries
/// no eyebrow; it is the panel's own top level.
const PANE_GROUPS: &[(&str, &[InspectorPane])] = &[
    ("", &[InspectorPane::Overview, InspectorPane::Diagnostics]),
    ("build", &[InspectorPane::Artifacts, InspectorPane::Symbols]),
    (
        "machine",
        &[
            InspectorPane::Cpu,
            InspectorPane::Pipeline,
            InspectorPane::Memory,
            InspectorPane::Disassembly,
            InspectorPane::Caches,
        ],
    ),
    (
        "kernel",
        &[
            InspectorPane::Processes,
            InspectorPane::GuestFs,
            InspectorPane::Trace,
        ],
    ),
];

/// All inspector-owned app state, grouped so the shell only carries one field.
pub(crate) struct InspectorState {
    pub(super) pane: InspectorPane,
    pub(super) symbol_filter: String,
    /// Throttled kernel-probe snapshot (processes, guest FS, syscall trace).
    pub(super) cache: Option<InspectCache>,
    pub(super) pid: Option<u64>,
    pub(super) inode: Option<u64>,
    /// Throttled cache line-occupancy grid; recomputed only while the Caches
    /// pane is visible, since `get_cache_snapshots` clones every line.
    pub(super) cache_grid: Option<CacheGrid>,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            pane: InspectorPane::Overview,
            symbol_filter: String::new(),
            cache: None,
            pid: None,
            inode: None,
            cache_grid: None,
        }
    }
}

impl InspectorState {
    /// Drop machine-session captures when the session goes away; the selected
    /// pane and filter survive so the panel does not jump around.
    pub(super) fn clear_session(&mut self) {
        self.cache = None;
        self.pid = None;
        self.inode = None;
        self.cache_grid = None;
    }
}

/// What the inspector can show right now, derived from app state each frame.
#[derive(Clone, Copy)]
pub(super) struct Capabilities {
    /// A resident VM session exists (live or finished but retained).
    pub(super) live: bool,
    /// Register state exists: a session or a post-mortem snapshot.
    pub(super) registers: bool,
    /// A successful build's artifacts are retained.
    pub(super) build: bool,
    /// The session carries kernel scheduler/trace symbols.
    pub(super) kernel: bool,
    /// The session carries guest-filesystem symbols.
    pub(super) guest_fs: bool,
}

impl Capabilities {
    fn allows(self, pane: InspectorPane) -> bool {
        match pane {
            InspectorPane::Overview | InspectorPane::Diagnostics => true,
            InspectorPane::Artifacts => self.build,
            InspectorPane::Symbols => self.build || self.live,
            InspectorPane::Cpu => self.registers,
            InspectorPane::Pipeline
            | InspectorPane::Memory
            | InspectorPane::Disassembly
            | InspectorPane::Caches => self.live,
            InspectorPane::Processes | InspectorPane::Trace => self.kernel,
            InspectorPane::GuestFs => self.guest_fs,
        }
    }
}

impl FullStackApp {
    fn inspector_capabilities(&self) -> Capabilities {
        Capabilities {
            live: self.live.is_some(),
            registers: self.live.is_some()
                || self
                    .vm_result
                    .as_ref()
                    .is_some_and(|vm| vm.registers.is_some()),
            build: self.last_build.is_some(),
            kernel: self
                .live
                .as_ref()
                .is_some_and(|live| live.os_symbols().is_some()),
            guest_fs: self
                .live
                .as_ref()
                .is_some_and(|live| live.fs_symbols().is_some()),
        }
    }

    /// Fall back to Overview when the selected pane lost its backing state
    /// (session ended, workspace changed); Overview is always renderable.
    pub(super) fn ensure_inspector_pane_available(&mut self) {
        if !self.inspector_capabilities().allows(self.inspector.pane) {
            self.inspector.pane = InspectorPane::Overview;
        }
    }

    pub(super) fn inspect_panel(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Inspect");
        self.ensure_inspector_pane_available();
        let caps = self.inspector_capabilities();
        self.pane_selector(ui, caps);
        ui.add_space(metrics::SECTION_GAP);
        chrome::hairline(ui);
        ui.add_space(metrics::SECTION_GAP);

        match self.inspector.pane {
            InspectorPane::Overview => self.overview_pane(ui, caps),
            InspectorPane::Diagnostics => self.diagnostics_pane(ui),
            InspectorPane::Artifacts => self.artifacts_pane(ui),
            InspectorPane::Symbols => self.symbols_pane(ui),
            InspectorPane::Cpu => self.cpu_pane(ui),
            InspectorPane::Pipeline => self.pipeline_pane(ui),
            InspectorPane::Memory => self.memory_pane(ui),
            InspectorPane::Disassembly => self.disassembly_pane(ui),
            InspectorPane::Caches => self.caches_pane(ui),
            InspectorPane::Processes => self.processes_pane(ui),
            InspectorPane::GuestFs => self.guest_fs_pane(ui),
            InspectorPane::Trace => self.trace_pane(ui),
        }
    }

    /// The grouped vertical pane list; only panes that can render now appear.
    fn pane_selector(&mut self, ui: &mut egui::Ui, caps: Capabilities) {
        for (group, panes) in PANE_GROUPS {
            let visible = panes.iter().copied().filter(|pane| caps.allows(*pane));
            let mut labeled = false;
            for pane in visible {
                if !labeled && !group.is_empty() {
                    ui.add_space(metrics::SECTION_GAP);
                    chrome::indented(ui, |ui| {
                        ui.label(theme::label(group));
                    });
                }
                labeled = true;
                if pane_row(ui, pane.label(), self.inspector.pane == pane).clicked() {
                    self.inspector.pane = pane;
                }
            }
        }
    }
}

/// Human tag for a build target mode, shared by the overview/artifact panes.
fn target_tag(target: TargetMode) -> &'static str {
    match target {
        TargetMode::Hosted => "hosted",
        TargetMode::Kernel => "kernel",
        TargetMode::Freestanding => "freestanding",
    }
}

/// A full-width selectable pane row; the active pane carries the accent bar,
/// matching the run-config and explorer selection idiom.
fn pane_row(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if active {
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
        label,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        if active { theme::ELECTRIC } else { theme::TEXT },
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTHING: Capabilities = Capabilities {
        live: false,
        registers: false,
        build: false,
        kernel: false,
        guest_fs: false,
    };

    fn visible(caps: Capabilities) -> Vec<InspectorPane> {
        PANE_GROUPS
            .iter()
            .flat_map(|(_, panes)| panes.iter().copied())
            .filter(|pane| caps.allows(*pane))
            .collect()
    }

    #[test]
    fn universal_panes_survive_an_empty_workspace() {
        assert_eq!(
            visible(NOTHING),
            vec![InspectorPane::Overview, InspectorPane::Diagnostics]
        );
    }

    #[test]
    fn a_build_without_a_run_unlocks_artifacts_and_symbols() {
        let caps = Capabilities {
            build: true,
            ..NOTHING
        };
        let panes = visible(caps);
        assert!(panes.contains(&InspectorPane::Artifacts));
        assert!(panes.contains(&InspectorPane::Symbols));
        assert!(!panes.contains(&InspectorPane::Cpu));
        assert!(!panes.contains(&InspectorPane::Processes));
    }

    #[test]
    fn a_hosted_run_shows_machine_panes_but_no_kernel_probes() {
        let caps = Capabilities {
            live: true,
            registers: true,
            build: true,
            ..NOTHING
        };
        let panes = visible(caps);
        for pane in [
            InspectorPane::Cpu,
            InspectorPane::Pipeline,
            InspectorPane::Memory,
            InspectorPane::Disassembly,
            InspectorPane::Caches,
        ] {
            assert!(panes.contains(&pane), "{pane:?} should be visible");
        }
        for pane in [
            InspectorPane::Processes,
            InspectorPane::GuestFs,
            InspectorPane::Trace,
        ] {
            assert!(!panes.contains(&pane), "{pane:?} should be hidden");
        }
    }

    #[test]
    fn a_kernel_boot_shows_every_pane() {
        let caps = Capabilities {
            live: true,
            registers: true,
            build: true,
            kernel: true,
            guest_fs: true,
        };
        let all: Vec<InspectorPane> = PANE_GROUPS
            .iter()
            .flat_map(|(_, panes)| panes.iter().copied())
            .collect();
        assert_eq!(visible(caps), all);
    }

    #[test]
    fn a_post_mortem_snapshot_keeps_cpu_without_live_panes() {
        let caps = Capabilities {
            registers: true,
            ..NOTHING
        };
        let panes = visible(caps);
        assert!(panes.contains(&InspectorPane::Cpu));
        assert!(!panes.contains(&InspectorPane::Pipeline));
        assert!(!panes.contains(&InspectorPane::Memory));
    }
}
