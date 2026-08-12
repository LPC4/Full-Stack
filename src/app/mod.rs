//! The IDE shell: an instrument panel laid out around the Stack Spine. State lives
//! in the `full_stack` backend; deep panels (run, machine, inspectors) wire in next.

mod build_stage;
mod launch_debug;
mod shortcuts;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
mod workspace;

use workspace::{WorkspacePreset, open_startup_workspace, reference_location};
#[cfg(all(test, not(target_arch = "wasm32")))]
use workspace::{demo_workspace_dir, reopen_last_workspace};

mod activity_rail;
mod chrome;
mod debug_view;
mod diagnostics;
mod editor;
mod editor_tabs;
mod elf_view;
mod explorer;
mod find;
mod inspector;
mod machine_dock;
mod metrics;
mod quick_open;
mod search;
mod settings;
mod status_bar;
mod terminal;
mod tool_window;
mod top_bar;

use crate::ui::{Layer, spine, theme};
use asm_to_binary::AssembledOutput;
use eframe::egui;
use full_stack::build::{
    BuildError, BuildExecutor, BuildManifest, BuildObject, BuildSource, StdlibPolicy, is_asm_path,
};
use full_stack::compilation_pipeline::{
    BinaryOutput, CompilationPipeline, PipelineFailure, PipelineResult, TargetMode,
};
use full_stack::runconfig::{BuildRef, RunConfig, RunKind};
use full_stack::session::{
    LiveMachine, MachineState, SessionEngine, SessionEngineError, VmExecutionResult,
};
#[cfg(not(target_arch = "wasm32"))]
use full_stack::workspace::NativeFs;
use full_stack::workspace::{IdeWorkspace, VPath};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use terminal::{ConsoleStream, TerminalKind, TerminalState};

const BUILD_GLOW_SECS: f64 = 0.85;
#[cfg(not(target_arch = "wasm32"))]
const LESSONS_WORKSPACE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/programs/lessons");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Files,
    Search,
    Run,
    Inspect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceKind {
    Build,
    Hll,
    Ir,
    Asm,
    Elf,
    Other,
}

const HLL_LAYERS: &[Layer] = &[Layer::Hll, Layer::Ir, Layer::Asm, Layer::Elf];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainSurface {
    Document,
    Debugger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaunchMode {
    Run,
    Debug,
}
#[derive(Clone, PartialEq)]
struct BuildTarget {
    manifest_path: VPath,
    root_path: VPath,
}

struct ShellLayout {
    tool_window_width: f32,
    tool_window_drag_start: Option<f32>,
}

impl Default for ShellLayout {
    fn default() -> Self {
        Self {
            tool_window_width: metrics::TOOL_WINDOW_DEFAULT,
            tool_window_drag_start: None,
        }
    }
}

pub(super) struct StageCache {
    path: String,
    source: String,
    result: PipelineResult,
}

// Restart boots these again when the config and its source fingerprint are unchanged.
struct LaunchCache {
    config: RunConfig,
    fingerprint: u64,
    launch: CachedLaunch,
}

enum CachedLaunch {
    Hosted(Box<full_stack::session::BuildOutcome>),
    Kernel(Box<full_stack::session::KernelBootOutcome>),
}

pub struct FullStackApp {
    pub(super) workspace: IdeWorkspace,
    pub(super) tool: Tool,
    pub(super) layer: Layer,
    layout: ShellLayout,
    pub(super) stage_cache: Option<StageCache>,
    // Per-mode cache so the live preview links the stdlib matching the active target.
    stdlib_objects: Vec<(TargetMode, Vec<(String, AssembledOutput)>)>,
    pub(super) status_message: Option<String>,
    build_glow_start: Option<f64>,
    build_lit_until: Option<Layer>,
    // Target of `build_lit_until`, so the lit spine only reads stale for a different program.
    built_target: Option<BuildTarget>,
    active_build_target: Option<BuildTarget>,
    pub(super) selected_config: usize,
    manifest_cache: Option<(f64, Vec<VPath>)>,
    idle_seen: Option<(u64, f64)>,
    idle_checked: u64,
    // Present from Run until the next config change.
    pub(super) live: Option<LiveMachine>,
    launch_cache: Option<LaunchCache>,
    pub(super) breakpoint_entry: String,
    pub(super) mount_host_entry: String,
    pub(super) mount_guest_entry: String,
    pub(super) pending_jump: Option<(VPath, usize)>,
    pub(super) stage_locate: Option<String>,
    pub(super) vm_result: Option<VmExecutionResult>,
    pub(super) machine_texture: Option<egui::TextureHandle>,
    pub(super) machine_texture_fill_count: Option<u64>,
    pub(super) terminal_open: bool,
    terminal: TerminalState,
    pub(super) find: find::FindState,
    pub(super) quick_open: Option<quick_open::QuickOpenState>,
    pub(super) explorer: explorer::ExplorerState,
    pub(super) search: search::SearchState,
    pub(super) pending_close: Option<VPath>,
    pub(super) cursor_pos: Option<(usize, usize)>,
    main_surface: MainSurface,
    pub(super) debug_view: debug_view::DebugView,
    debugger_open: bool,
    pub(super) session_tab_label: Option<String>,
    pub(super) memory_view: debug_view::MemoryViewState,
    // Sticky across stretches of pc with no line info, so a line stays highlighted.
    pub(super) code_follow: Option<(String, u32)>,
    // Last auto-scroll target, so follow-pc does not fight a manual scroll every frame.
    pub(super) code_follow_scrolled: Option<(String, u32)>,
    launch_mode: LaunchMode,
    pub(super) step_count: u64,
    step_unit: machine_dock::StepUnit,
    pub(super) time_scrub: usize,
    // Pinned to the newest frame until the user drags back; re-armed by "now" or the end.
    pub(super) time_follow_now: bool,
    pub(super) inspector: inspector::InspectorState,
    // Retained so Artifacts/Symbols work without a session; cleared on config switch.
    last_build: Option<full_stack::session::BuildOutcome>,
    pub(super) settings: settings::AppSettings,
    pub(super) settings_open: bool,
    pub(super) repository_paths: HashMap<VPath, VPath>,
    reference_jump: Option<(VPath, usize)>,
}

impl FullStackApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_reference(cc, crate::web_reference::StartupReference::default())
    }

    pub fn new_with_reference(
        cc: &eframe::CreationContext<'_>,
        startup: crate::web_reference::StartupReference,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings: settings::AppSettings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, settings::APP_KEY))
            .unwrap_or_default();
        settings.apply(&cc.egui_ctx);
        let workspace = startup
            .document
            .as_ref()
            .and_then(|reference| reference_location(&reference.path))
            .and_then(|location| location.preset.memory_workspace().ok())
            .unwrap_or_else(|| open_startup_workspace(&settings));
        let mut app = Self::with_workspace(workspace, settings);
        if let Some(reference) = startup.document {
            app.open_repository_reference(reference, startup.status);
        } else if startup.status.is_some() {
            app.status_message = startup.status;
        }
        app
    }

    fn with_workspace(workspace: IdeWorkspace, settings: settings::AppSettings) -> Self {
        let mut app = Self {
            workspace,
            tool: Tool::Files,
            layer: Layer::Hll,
            layout: ShellLayout::default(),
            stage_cache: None,
            stdlib_objects: Vec::new(),
            status_message: None,
            build_glow_start: None,
            build_lit_until: None,
            built_target: None,
            active_build_target: None,
            selected_config: 0,
            manifest_cache: None,
            idle_seen: None,
            idle_checked: 0,
            live: None,
            launch_cache: None,
            breakpoint_entry: String::new(),
            mount_host_entry: String::new(),
            mount_guest_entry: String::new(),
            pending_jump: None,
            stage_locate: None,
            vm_result: None,
            machine_texture: None,
            machine_texture_fill_count: None,
            terminal_open: false,
            terminal: TerminalState::default(),
            find: find::FindState::default(),
            quick_open: None,
            explorer: explorer::ExplorerState::default(),
            search: search::SearchState::default(),
            pending_close: None,
            cursor_pos: None,
            main_surface: MainSurface::Document,
            debug_view: debug_view::DebugView::Disasm,
            debugger_open: false,
            session_tab_label: None,
            memory_view: debug_view::MemoryViewState::default(),
            code_follow: None,
            code_follow_scrolled: None,
            launch_mode: LaunchMode::Run,
            step_count: 1,
            step_unit: machine_dock::StepUnit::Line,
            time_scrub: 0,
            time_follow_now: true,
            inspector: inspector::InspectorState::default(),
            last_build: None,
            settings,
            settings_open: false,
            repository_paths: HashMap::new(),
            reference_jump: None,
        };
        if !app.workspace.run_configs().configs.is_empty() {
            app.select_config(0);
        }
        app
    }

    fn build_glow_t(&mut self, now: f64) -> Option<f32> {
        let start = self.build_glow_start?;
        let t = ((now - start) / BUILD_GLOW_SECS) as f32;
        if t >= 1.0 {
            self.build_glow_start = None;
            None
        } else {
            Some(t)
        }
    }

    fn visible_layers(&self) -> Option<&'static [Layer]> {
        let active = self.active_path()?;
        // Any source file that resolves to a build target rides that target's
        // spine, so modules keep the complete compilation pipeline in reach.
        self.active_build_target.as_ref()?;
        match Self::source_kind_for_path(active.as_str()) {
            SourceKind::Hll => Some(HLL_LAYERS),
            _ => None,
        }
    }

    pub(super) fn source_layer_for_kind(kind: SourceKind) -> Layer {
        match kind {
            SourceKind::Build | SourceKind::Hll | SourceKind::Other => Layer::Hll,
            SourceKind::Ir => Layer::Ir,
            SourceKind::Asm => Layer::Asm,
            SourceKind::Elf => Layer::Elf,
        }
    }

    pub(super) fn reset_layer_for_active_file(&mut self) {
        self.layer = Self::source_layer_for_kind(self.active_source_kind());
    }

    fn ensure_layer_visible(&mut self) {
        let Some(layers) = self.visible_layers() else {
            return;
        };
        if !layers.contains(&self.layer)
            && let Some(first) = layers.first()
        {
            self.layer = *first;
        }
    }

    fn spine_panel(&mut self, ui: &mut egui::Ui, build_glow_t: Option<f32>) {
        let Some(layers) = self.visible_layers() else {
            return;
        };
        if let Some(layer) = spine::show(
            ui,
            spine::SpineProps {
                layers,
                active: self.layer,
                pulse_t: build_glow_t,
                build_lit_until: self.lit_layer_for_active_target(),
                always_enabled_until: layers.first().copied(),
            },
        ) {
            // The spine always speaks to the document stack; a click from the
            // debugger surface returns to the source tab at the chosen stage.
            if self.main_surface == MainSurface::Debugger {
                self.main_surface = MainSurface::Document;
            }
            // Landing on IR/ASM scrolls the stage view to the function under
            // the editor caret, so the lowering you meant is in view.
            if matches!(layer, Layer::Ir | Layer::Asm) && layer != self.layer {
                self.stage_locate = self.caret_function_name();
            }
            self.layer = layer;
        }
    }

    fn body(&mut self, ui: &mut egui::Ui, build_glow_t: Option<f32>) {
        self.refresh_build_target();
        self.ensure_layer_visible();
        let rect = ui.available_rect_before_wrap();
        ui.expand_to_include_rect(rect);

        let show_spine = !self.settings_open && self.visible_layers().is_some();
        let spine_width = if show_spine { spine::width() } else { 0.0 };
        let max_width = metrics::TOOL_WINDOW_MAX
            .min((rect.width() - spine_width - metrics::EDITOR_MIN).max(0.0));
        let max_width = max_width.max(metrics::TOOL_WINDOW_MIN.min(rect.width()));
        self.layout.tool_window_width = self
            .layout
            .tool_window_width
            .clamp(metrics::TOOL_WINDOW_MIN.min(max_width), max_width);

        let tool_rect = egui::Rect::from_min_size(
            rect.left_top(),
            egui::vec2(self.layout.tool_window_width, rect.height()),
        );
        let spine_rect = egui::Rect::from_min_size(
            egui::pos2(tool_rect.right(), rect.top()),
            egui::vec2(spine_width, rect.height()),
        );
        let editor_rect = egui::Rect::from_min_max(
            egui::pos2(spine_rect.right(), rect.top()),
            rect.right_bottom(),
        );

        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("tool_window")
                .max_rect(tool_rect),
            |ui| {
                self.tool_window(ui);
            },
        );
        if show_spine {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt("stack_spine")
                    .max_rect(spine_rect),
                |ui| {
                    self.spine_panel(ui, build_glow_t);
                },
            );
        }
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("editor")
                .max_rect(editor_rect),
            |ui| {
                self.editor(ui);
            },
        );

        // Dividers span the working area (under the top bar to the status bar); the
        // zero-margin central frame lets them meet egui's panel separators cleanly.
        let sep_top = rect.top();
        self.resize_tool_window(ui, tool_rect, max_width, sep_top);
        if show_spine {
            draw_separator(ui, spine_rect.right(), sep_top, spine_rect.bottom(), false);
        }
    }

    fn resize_tool_window(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        max_width: f32,
        sep_top: f32,
    ) {
        let edge_x = rect.right();
        let hit_rect = egui::Rect::from_min_max(
            egui::pos2(edge_x - metrics::RESIZE_HIT_RADIUS, rect.top()),
            egui::pos2(edge_x + metrics::RESIZE_HIT_RADIUS, rect.bottom()),
        );
        let response = ui.interact(
            hit_rect,
            ui.id().with("tool_window_resize"),
            egui::Sense::drag(),
        );

        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.drag_started() {
            self.layout.tool_window_drag_start = Some(self.layout.tool_window_width);
        }
        if response.dragged() {
            let start = self
                .layout
                .tool_window_drag_start
                .unwrap_or(self.layout.tool_window_width);
            let delta = response.total_drag_delta().unwrap_or_default().x;
            self.layout.tool_window_width =
                (start + delta).clamp(metrics::TOOL_WINDOW_MIN.min(max_width), max_width);
            ui.ctx().request_repaint();
        }
        if response.drag_stopped() {
            self.layout.tool_window_drag_start = None;
        }

        draw_separator(
            ui,
            edge_x,
            sep_top,
            rect.bottom(),
            response.hovered() || response.dragged(),
        );
    }
}

impl eframe::App for FullStackApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, settings::APP_KEY, &self.settings);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        self.drive_live_machine(ui.ctx());
        self.refresh_inspect_cache(now);
        self.refresh_cache_grid(now);
        self.poll_idle_check(ui.ctx(), now);
        self.handle_shortcuts(ui, now);
        let build_glow_t = self.build_glow_t(now);
        if build_glow_t.is_some() {
            ui.ctx().request_repaint();
        }

        // Each band fills to its own edges (zero horizontal margin); egui's built-in
        // panel separators draw every boundary once, so no divider is doubled.
        let chrome_band = egui::Frame::new()
            .fill(theme::CHROME)
            .inner_margin(egui::Margin::symmetric(metrics::PANEL_INSET_X as i8, 0));
        // The rail carries its own icons to the edge, so it takes CHROME without the inset.
        let rail_band = egui::Frame::new().fill(theme::CHROME);
        let well_band = egui::Frame::new().fill(theme::WELL);
        let canvas_band = egui::Frame::new().fill(theme::CANVAS);
        let terminal_band = egui::Frame::new().fill(theme::TERMINAL);

        egui::Panel::bottom("status_bar")
            .resizable(false)
            .exact_size(metrics::STATUS_BAR_HEIGHT)
            .frame(chrome_band)
            .show_inside(ui, |ui| self.status_bar(ui));

        egui::Panel::top("top_bar")
            .resizable(false)
            .exact_size(metrics::TOP_BAR_HEIGHT)
            .frame(chrome_band)
            .show_inside(ui, |ui| self.top_bar(ui, now));

        egui::Panel::left("activity_rail")
            .resizable(false)
            .exact_size(metrics::ACTIVITY_RAIL_WIDTH)
            .frame(rail_band)
            .show_inside(ui, |ui| self.activity_rail(ui));

        if self.debugger_dock_visible() {
            egui::Panel::right("machine_dock")
                .resizable(true)
                .default_size(metrics::MACHINE_DOCK_DEFAULT)
                .size_range(metrics::MACHINE_DOCK_MIN..=metrics::MACHINE_DOCK_MAX)
                .frame(well_band)
                .show_inside(ui, |ui| self.machine_dock(ui));
        }

        if self.terminal_open {
            let terminal_max = ui.available_height().max(metrics::TERMINAL_MIN_HEIGHT);
            egui::Panel::bottom("terminal")
                .resizable(true)
                .default_size(metrics::TERMINAL_DEFAULT_HEIGHT)
                .size_range(metrics::TERMINAL_MIN_HEIGHT..=terminal_max)
                .frame(terminal_band)
                .show_inside(ui, |ui| self.terminal_panel(ui, now));
        }

        egui::CentralPanel::default()
            .frame(canvas_band)
            .show_inside(ui, |ui| self.body(ui, build_glow_t));

        self.quick_open_overlay(ui.ctx());
        self.explorer_confirm_overlay(ui.ctx());
        self.framebuffer_window(ui.ctx());
    }
}

// Body paints last: hairlines land above panel fills yet under floating windows.
fn separator_painter(ui: &egui::Ui) -> egui::Painter {
    ui.ctx().layer_painter(egui::LayerId::background())
}

fn draw_separator(ui: &egui::Ui, x: f32, top: f32, bottom: f32, active: bool) {
    let color = if active {
        theme::ELECTRIC
    } else {
        theme::HAIRLINE
    };
    let painter = separator_painter(ui);
    painter.line_segment(
        [egui::pos2(x, top), egui::pos2(x, bottom)],
        egui::Stroke::new(1.0, color),
    );
}
