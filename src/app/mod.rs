//! The IDE shell: an instrument panel laid out around the Stack Spine. State lives
//! in the `full_stack` backend; deep panels (run, machine, inspectors) wire in next.

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
    BuildError, BuildExecutor, BuildManifest, BuildObject, BuildSource, StdlibPolicy,
};
use full_stack::compilation_pipeline::{
    BinaryOutput, CompilationPipeline, PipelineResult, TargetMode,
};
use full_stack::runconfig::{BuildRef, RunConfig, RunKind};
use full_stack::session::{
    LiveMachine, MachineState, SessionEngine, SessionEngineError, VmExecutionResult,
};
#[cfg(not(target_arch = "wasm32"))]
use full_stack::workspace::NativeFs;
use full_stack::workspace::{IdeWorkspace, VPath};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use terminal::{ConsoleStream, TerminalKind, TerminalState};

const BUILD_GLOW_SECS: f64 = 0.85;
/// Per-frame step budget when a run config does not specify one.
#[cfg(not(target_arch = "wasm32"))]
const DEMO_WORKSPACE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/programs/demo_workspace");

#[derive(Clone, Copy, PartialEq, Eq)]
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

const HLL_LAYERS: &[Layer] = &[Layer::Hll, Layer::Ir, Layer::Asm, Layer::Elf, Layer::Vm];
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

/// Artifacts from the last successful launch; Restart boots them again when
/// the config and its source fingerprint are unchanged.
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
    /// Per-target-mode stdlib object cache, so the live preview links against the
    /// stdlib matching the active program's target instead of always hosted.
    stdlib_objects: Vec<(TargetMode, Vec<(String, AssembledOutput)>)>,
    pub(super) status_message: Option<String>,
    build_glow_start: Option<f64>,
    build_lit_until: Option<Layer>,
    /// Which target `build_lit_until` belongs to, so the lit spine survives
    /// switching tabs and only reads stale for a different program.
    built_target: Option<BuildTarget>,
    active_build_target: Option<BuildTarget>,
    pub(super) selected_config: usize,
    /// Discovered `.build` manifests with the time they were scanned.
    manifest_cache: Option<(f64, Vec<VPath>)>,
    /// Check-on-idle: dirty-buffer fingerprint last seen and when it changed.
    idle_seen: Option<(u64, f64)>,
    /// The fingerprint the idle checker last compiled, so it runs once per edit.
    idle_checked: u64,
    /// The resident VM session; present from Run until the next config change.
    pub(super) live: Option<LiveMachine>,
    launch_cache: Option<LaunchCache>,
    /// Dock input buffer for adding a `symbol+offset` breakpoint.
    pub(super) breakpoint_entry: String,
    /// Run-panel input buffers for adding a host->guest mount to a config.
    pub(super) mount_host_entry: String,
    pub(super) mount_guest_entry: String,
    /// A click-to-jump target (path, 1-based line) the editor scrolls to.
    pub(super) pending_jump: Option<(VPath, usize)>,
    pub(super) vm_result: Option<VmExecutionResult>,
    pub(super) machine_texture: Option<egui::TextureHandle>,
    pub(super) machine_texture_fill_count: Option<u64>,
    pub(super) terminal_open: bool,
    terminal: TerminalState,
    pub(super) find: find::FindState,
    pub(super) quick_open: Option<quick_open::QuickOpenState>,
    pub(super) explorer: explorer::ExplorerState,
    pub(super) search: search::SearchState,
    /// Dirty document waiting for close confirmation when that setting is on.
    pub(super) pending_close: Option<VPath>,
    /// 1-based cursor line/column of the active editor, for the status bar.
    pub(super) cursor_pos: Option<(usize, usize)>,
    /// Which surface the machine (L4) rung shows for a live session.
    pub(super) machine_view: debug_view::MachineView,
    /// Basename of the running config's root source, naming the [debug] tab.
    pub(super) machine_tab_label: Option<String>,
    pub(super) memory_view: debug_view::MemoryViewState,
    /// Last known source location of the machine Code view (sticky across
    /// stretches of pc with no line info, so a line stays highlighted).
    pub(super) code_follow: Option<(String, u32)>,
    /// Location the Code view last auto-scrolled to, so it follows the pc but
    /// does not fight a manual scroll every frame.
    pub(super) code_follow_scrolled: Option<(String, u32)>,
    /// Whether the current/last live session was launched as a debug session.
    current_launch_debug: bool,
    /// Instruction count for the dock's "step n" control.
    pub(super) step_count: u64,
    step_unit: machine_dock::StepUnit,
    /// Dock time-travel scrubber position (frame index into the session history).
    pub(super) time_scrub: usize,
    /// The Inspect tool window's pane selection and captured probe state.
    pub(super) inspector: inspector::InspectorState,
    /// The last successful config build, retained so the inspector's Artifacts
    /// and Symbols panes work without a session (cleared on config switch).
    last_build: Option<full_stack::session::BuildOutcome>,
    /// User-level settings, persisted through eframe storage.
    pub(super) settings: settings::AppSettings,
    /// Whether the full-page settings view is showing over the editor.
    pub(super) settings_open: bool,
}

impl FullStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings: settings::AppSettings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, settings::APP_KEY))
            .unwrap_or_default();
        settings.apply(&cc.egui_ctx);
        let mut app = Self {
            workspace: open_startup_workspace(&settings),
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
            machine_view: debug_view::MachineView::Console,
            machine_tab_label: None,
            memory_view: debug_view::MemoryViewState::default(),
            code_follow: None,
            code_follow_scrolled: None,
            current_launch_debug: false,
            step_count: 100,
            step_unit: machine_dock::StepUnit::Line,
            time_scrub: 0,
            inspector: inspector::InspectorState::default(),
            last_build: None,
            settings,
            settings_open: false,
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

    fn active_path(&self) -> Option<VPath> {
        self.workspace
            .active_document()
            .map(|doc| doc.path().clone())
    }

    /// Open (or focus) a document and resync the layer, stage cache, and spine.
    pub(super) fn open_path(&mut self, path: VPath) {
        if self.workspace.open_document(path).is_ok() {
            self.document_focus_changed();
        }
    }

    /// Focus an already-open document (a tab click).
    pub(super) fn activate_path(&mut self, path: &VPath) {
        self.workspace.activate_document(path);
        self.document_focus_changed();
    }

    /// Close a tab; unsaved edits are discarded (the tab's dot warns beforehand).
    pub(super) fn close_path(&mut self, path: &VPath) {
        if self.settings.confirm_discard_on_close
            && self
                .workspace
                .document(path)
                .is_some_and(|doc| doc.is_dirty())
        {
            self.pending_close = Some(path.clone());
            self.status_message = Some(format!("{path} has unsaved edits"));
            return;
        }
        self.close_path_confirmed(path);
    }

    pub(super) fn close_path_confirmed(&mut self, path: &VPath) {
        let discarded = self
            .workspace
            .document(path)
            .is_some_and(|doc| doc.is_dirty());
        self.workspace.close_document(path);
        if self.pending_close.as_ref() == Some(path) {
            self.pending_close = None;
        }
        if discarded {
            self.status_message = Some(format!("closed {path} \u{00B7} unsaved edits discarded"));
        }
        self.document_focus_changed();
    }

    pub(super) fn close_active_path(&mut self) {
        if let Some(path) = self.active_path() {
            self.close_path(&path);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn block_open_folder_for_dirty_documents(&mut self) -> bool {
        if self.workspace.dirty_documents().next().is_none() {
            return false;
        }
        self.status_message =
            Some("save or close unsaved files before opening a folder".to_owned());
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn open_native_workspace_folder(&mut self, path: &Path) {
        if self.block_open_folder_for_dirty_documents() {
            return;
        }
        let native_fs = match NativeFs::open(path) {
            Ok(fs) => fs,
            Err(err) => {
                self.status_message = Some(format!("open folder failed: {err}"));
                return;
            }
        };
        let root_label = native_fs.root().display().to_string();
        match IdeWorkspace::new(Box::new(native_fs)) {
            Ok(workspace) => {
                // Remember this folder so "reopen last workspace" can restore it.
                self.settings.last_workspace = Some(root_label.clone());
                self.replace_workspace(workspace, &root_label);
            }
            Err(err) => {
                self.status_message = Some(format!("open folder failed: {err}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn replace_workspace(&mut self, workspace: IdeWorkspace, label: &str) {
        self.workspace = workspace;
        self.live = None;
        self.launch_cache = None;
        self.pending_jump = None;
        self.vm_result = None;
        self.machine_texture = None;
        self.machine_texture_fill_count = None;
        self.stage_cache = None;
        self.active_build_target = None;
        self.manifest_cache = None;
        self.quick_open = None;
        self.find = find::FindState::default();
        self.explorer = explorer::ExplorerState::default();
        self.search = search::SearchState::default();
        self.pending_close = None;
        self.cursor_pos = None;
        self.current_launch_debug = false;
        self.memory_view = debug_view::MemoryViewState::default();
        self.inspector = inspector::InspectorState::default();
        self.last_build = None;
        self.reset_machine_views();
        self.mark_build_stale();
        self.selected_config = 0;
        self.reset_layer_for_active_file();
        if !self.workspace.run_configs().configs.is_empty() {
            self.select_config(0);
        }
        self.status_message = Some(format!("opened {label}"));
    }

    pub(super) fn document_focus_changed(&mut self) {
        self.reset_layer_for_active_file();
        // Switching tabs must not dim the lit spine; only edits and builds may.
        self.stage_cache = None;
    }

    pub(super) fn active_source_kind(&self) -> SourceKind {
        self.active_path()
            .map(|path| Self::source_kind_for_path(path.as_str()))
            .unwrap_or(SourceKind::Other)
    }

    pub(super) fn source_kind_for_path(path: &str) -> SourceKind {
        if path.ends_with(".build") {
            SourceKind::Build
        } else if path.ends_with(".hll") {
            SourceKind::Hll
        } else if path.ends_with(".ir") {
            SourceKind::Ir
        } else if path.ends_with(".s") {
            SourceKind::Asm
        } else if path.ends_with(".elf") || path.ends_with(".o") {
            SourceKind::Elf
        } else {
            SourceKind::Other
        }
    }

    fn visible_layers(&self) -> Option<&'static [Layer]> {
        let active = self.active_path()?;
        // Any source file that resolves to a build target rides that target's
        // spine, so modules keep the pipeline (and the L4 rung) in reach.
        self.active_build_target.as_ref()?;
        match Self::source_kind_for_path(active.as_str()) {
            SourceKind::Hll | SourceKind::Build => Some(HLL_LAYERS),
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
            layers,
            self.layer,
            build_glow_t,
            self.lit_layer_for_active_target(),
            layers.first().copied(),
        ) {
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

    pub(super) fn build_active_document(&mut self, now: f64) {
        let config = self.selected_config().cloned();
        let has_config = config.is_some();
        let target_name = config
            .as_ref()
            .and_then(|config| self.config_root(config))
            .map(|path| path.to_string())
            .unwrap_or_else(|| {
                self.active_path()
                    .map(|path| path.to_string())
                    .unwrap_or_else(|| "active document".to_owned())
            });
        self.terminal
            .start("Build", format!("fsc build {target_name}"));
        self.terminal_open = self.settings.open_terminal_on_run;

        self.refresh_build_target();
        if self.visible_layers().is_some() {
            self.refresh_stage_cache();
        } else {
            self.stage_cache = None;
        }
        let produced_layer = if self.visible_layers().is_some() {
            self.stage_cache.as_ref().map(|cache| {
                let result = &cache.result;
                if result.binary.is_some() {
                    Layer::Elf
                } else if result.asm.is_some() {
                    Layer::Asm
                } else if result.ir.is_some() {
                    Layer::Ir
                } else {
                    Layer::Hll
                }
            })
        } else {
            None
        };
        let preview_failed = self
            .stage_cache
            .as_ref()
            .is_some_and(|cache| cache.result.has_errors() || cache.result.has_assembler_error());

        let mut failed = preview_failed;
        if let Some(config) = config {
            let out_dir = self.workspace.fs().host_out_dir();
            match SessionEngine::new(self.workspace.fs())
                .with_artifact_root(out_dir)
                .build_config(&config)
            {
                Ok(outcome) => {
                    failed = false;
                    self.status_message = Some("built".to_owned());
                    self.report_linked_build(&outcome);
                }
                Err(err) => {
                    failed = true;
                    self.status_message = Some("build failed".to_owned());
                    // Compile failures the preview already diagnosed render as
                    // caret blocks below; the stringified error would repeat them.
                    if !(matches!(err, SessionEngineError::Build(BuildError::Compile(_)))
                        && preview_failed)
                    {
                        self.terminal.push(ConsoleStream::Error, err.to_string());
                    }
                }
            }
        } else if preview_failed {
            self.status_message = Some("build failed".to_owned());
        } else if let Some(cache) = &self.stage_cache {
            self.status_message = Some("built".to_owned());
            self.terminal.push(
                ConsoleStream::Success,
                build_summary(&cache.result, produced_layer),
            );
        } else {
            self.status_message = Some("build skipped".to_owned());
            self.terminal.push(
                ConsoleStream::Warning,
                "No build manifest was found for the active document.",
            );
        }
        if !has_config || failed {
            self.push_build_diagnostics(failed);
        }
        self.build_glow_start = Some(now);
        self.record_build_lit(produced_layer);
    }

    /// Diagnostics land in Output as the rendered CLI block (location, source
    /// line, caret, note); the message and location lines click-to-jump.
    fn push_build_diagnostics(&mut self, failed: bool) {
        use full_stack::compilation_pipeline::DiagnosticLevel;
        let Some(cache) = &self.stage_cache else {
            return;
        };
        let root = self
            .active_build_target
            .as_ref()
            .map(|target| target.root_path.as_str().to_owned());
        for diag in &cache.result.diagnostics {
            let stream = match diag.level {
                DiagnosticLevel::Error => ConsoleStream::Error,
                DiagnosticLevel::Warning => ConsoleStream::Warning,
            };
            let block = match &root {
                Some(root) => diag.render(root),
                None => diag.format_full(),
            };
            let link = match (&diag.span, &root) {
                (Some(span), Some(root)) => Some((root.clone(), span.line as usize)),
                _ => None,
            };
            for (i, line) in block.lines().enumerate() {
                let stream = if i == 0 {
                    stream
                } else {
                    ConsoleStream::Detail
                };
                match &link {
                    Some((path, target)) if i == 0 || line.trim_start().starts_with("-->") => {
                        self.terminal
                            .push_linked(stream, line, path.clone(), *target);
                    }
                    _ => self.terminal.push(stream, line),
                }
            }
        }
        if let Some(err) = &cache.result.assembler_error {
            self.terminal.push(ConsoleStream::Error, err.clone());
        }
        if failed && cache.result.diagnostics.is_empty() && cache.result.assembler_error.is_none() {
            self.terminal.push(
                ConsoleStream::Error,
                "Build failed without a diagnostic payload.",
            );
        }
    }

    /// Open `path` and scroll it to `line` (1-based); used by Output links.
    pub(super) fn jump_to_source(&mut self, path: &str, line: usize) {
        let Ok(vpath) = VPath::parse(path) else {
            return;
        };
        // Only reopen when the file is not already active, so the stage cache
        // (and with it the editor's diagnostic markers) survives the jump.
        if self.active_path().as_ref() != Some(&vpath) {
            self.open_path(vpath.clone());
            if self.active_path().as_ref() != Some(&vpath) {
                return;
            }
        }
        self.pending_jump = Some((vpath, line));
    }

    pub(super) fn mark_build_stale(&mut self) {
        self.build_lit_until = None;
        self.built_target = None;
        self.build_glow_start = None;
    }

    /// Light the spine through `layer` for the target currently in focus.
    fn record_build_lit(&mut self, layer: Option<Layer>) {
        self.build_lit_until = layer;
        self.built_target = self.active_build_target.clone();
    }

    /// The lit layer, only while the active file still belongs to the program
    /// that was built; a different program's spine must not read as built.
    fn lit_layer_for_active_target(&self) -> Option<Layer> {
        (self.built_target.is_some() && self.built_target == self.active_build_target)
            .then_some(self.build_lit_until)
            .flatten()
    }

    pub(super) fn save_active_document(&mut self) {
        match self.workspace.save_active_document() {
            Ok(()) => {
                self.check_after_save("saved");
            }
            Err(err) => {
                self.status_message = Some(format!("save failed: {err:?}"));
            }
        }
    }

    pub(super) fn save_all_documents(&mut self) {
        match self.workspace.save_all() {
            Ok(()) => {
                self.check_after_save("saved all");
            }
            Err(err) => {
                self.status_message = Some(format!("save all failed: {err:?}"));
            }
        }
    }

    /// Check-on-save: re-run the stage cache for the active build target so
    /// editor markers and the status chip reflect the file that was just saved.
    fn check_after_save(&mut self, verb: &str) {
        self.stage_cache = None;
        if !self.settings.check_on_save {
            self.status_message = Some(verb.to_owned());
            return;
        }
        self.refresh_build_target();
        self.refresh_stage_cache();
        let (errors, warnings) = self
            .stage_cache
            .as_ref()
            .map(|cache| diagnostics::count_levels(&cache.result.diagnostics))
            .unwrap_or((0, 0));
        self.status_message = Some(match (errors, warnings) {
            (0, 0) => verb.to_owned(),
            (0, w) => format!(
                "{verb} \u{00B7} {}",
                diagnostics::tally(w as u64, "warning")
            ),
            (e, 0) => format!("{verb} \u{00B7} {}", diagnostics::tally(e as u64, "error")),
            (e, w) => format!(
                "{verb} \u{00B7} {}, {}",
                diagnostics::tally(e as u64, "error"),
                diagnostics::tally(w as u64, "warning")
            ),
        });
    }

    /// Check-on-idle: once the active buffer has sat unchanged past the idle
    /// delay, re-run the stage cache so markers track unsaved edits.
    fn poll_idle_check(&mut self, ctx: &egui::Context, now: f64) {
        const IDLE_CHECK_DELAY_SECS: f64 = 1.0;
        if !self.settings.check_on_idle {
            return;
        }
        let Some(doc) = self.workspace.active_document() else {
            return;
        };
        // Clean buffers match disk; the save path already checked them.
        if !doc.is_dirty() {
            return;
        }
        let fingerprint = {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::hash::DefaultHasher::new();
            doc.text().hash(&mut hasher);
            hasher.finish()
        };
        match self.idle_seen {
            Some((seen, changed_at)) if seen == fingerprint => {
                if self.idle_checked == fingerprint {
                    return;
                }
                if now - changed_at >= IDLE_CHECK_DELAY_SECS {
                    self.idle_checked = fingerprint;
                    self.stage_cache = None;
                    self.refresh_build_target();
                    self.refresh_stage_cache();
                } else {
                    // No input events arrive while the user pauses; keep a
                    // frame queued so the delayed check actually fires.
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
            }
            _ => {
                self.idle_seen = Some((fingerprint, now));
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }

    pub(super) fn refresh_stage_cache(&mut self) {
        let Some(target) = self.active_build_target.clone() else {
            self.stage_cache = None;
            return;
        };
        let Ok(manifest) = self.workspace.load_manifest(&target.manifest_path) else {
            self.stage_cache = None;
            return;
        };
        let (path, mut source) = match &manifest.root {
            BuildSource::Inline {
                source,
                source_path,
                ..
            } => (
                source_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| target.root_path.as_str().to_owned()),
                source.clone(),
            ),
            BuildSource::Path { path, .. } => {
                let path_text = path.display().to_string();
                let Ok(bytes) = self.workspace.fs().read_file(&target.root_path) else {
                    self.stage_cache = None;
                    return;
                };
                let Ok(source) = String::from_utf8(bytes) else {
                    self.stage_cache = None;
                    return;
                };
                (path_text, source)
            }
        };
        // Prefer the open buffer so check-on-idle sees unsaved edits.
        if let Some(doc) = self.workspace.document(&target.root_path) {
            source = doc.text().to_owned();
        }
        if self
            .stage_cache
            .as_ref()
            .is_some_and(|cache| cache.path == path && cache.source == source)
        {
            return;
        }

        // Display stages (tokens/AST/IR/ASM) from the root unit; the authoritative
        // binary/link comes from the full manifest build below.
        let mut pipeline = BuildExecutor::pipeline_for(&manifest);
        pipeline.set_write_artifacts(false);
        pipeline.set_current_source_path(Some(path.clone()));
        let mut result = pipeline.run_full(&source, &[]);
        result.binary = None;
        result.assembler_error = None;

        if !result.has_errors() {
            match self.build_preview_binary(&manifest, &path, &source) {
                Ok(binary) => result.binary = Some(binary),
                Err(err) => result.assembler_error = Some(err),
            }
        }
        self.stage_cache = Some(StageCache {
            path,
            source,
            result,
        });
    }

    /// Link the active program as Build/Run would, reusing the cached per-mode stdlib
    /// and the live root buffer so the preview link matches reality.
    fn build_preview_binary(
        &mut self,
        manifest: &BuildManifest,
        path: &str,
        source: &str,
    ) -> Result<BinaryOutput, String> {
        let stdlib_mode = match manifest.stdlib {
            StdlibPolicy::None => None,
            StdlibPolicy::ForTarget => Some(manifest.target),
            StdlibPolicy::Explicit(mode) => Some(mode),
        };

        let mut manifest = manifest.clone();
        // Build from the live editor buffer, not the on-disk file.
        manifest.root = BuildSource::inline_with_path(
            manifest.name.clone(),
            source.to_owned(),
            std::path::PathBuf::from(path),
        );
        manifest.write_artifacts = false;
        // Inject the cached stdlib objects instead of recompiling them each keystroke.
        manifest.stdlib = StdlibPolicy::None;
        if let Some(mode) = stdlib_mode {
            let stdlib = self
                .stdlib_objects_for(mode)
                .ok_or_else(|| "stdlib build failed".to_owned())?
                .to_vec();
            manifest.extra_objects = stdlib
                .into_iter()
                .map(|(name, assembled)| BuildObject { name, assembled })
                .collect();
        }

        match BuildExecutor::build(&manifest) {
            Ok(artifacts) => Ok(BinaryOutput {
                assembled: artifacts.linked,
            }),
            Err(BuildError::Link(err)) => Err(format!("linker error: {}", err.message)),
            Err(err) => Err(err.to_string()),
        }
    }

    pub(super) fn refresh_build_target(&mut self) {
        let Some(active_path) = self.active_path() else {
            self.active_build_target = None;
            return;
        };
        self.active_build_target = self.find_build_target_for(&active_path);
    }

    fn find_build_target_for(&self, path: &VPath) -> Option<BuildTarget> {
        let manifest_path = if path.as_str().ends_with(".build") {
            path.clone()
        } else {
            self.find_nearest_build_manifest(path)?
        };
        self.build_target_from_manifest(manifest_path)
    }

    fn build_target_from_manifest(&self, manifest_path: VPath) -> Option<BuildTarget> {
        let manifest = self.workspace.load_manifest(&manifest_path).ok()?;
        let root_path = match manifest.root {
            BuildSource::Inline {
                source_path: Some(path),
                ..
            }
            | BuildSource::Path { path, .. } => VPath::parse(&path.display().to_string()).ok()?,
            BuildSource::Inline { .. } => return None,
        };
        Some(BuildTarget {
            manifest_path,
            root_path,
        })
    }

    // --- Run configs ---

    pub(super) fn configs(&self) -> &[RunConfig] {
        &self.workspace.run_configs().configs
    }

    /// Write the run-config store back to `.fullstack/run.toml`.
    pub(super) fn persist_run_configs(&mut self) {
        if let Err(err) = self.workspace.save_run_configs() {
            self.status_message = Some(format!("saving run configs failed: {err}"));
        }
    }

    /// Workspace `.build` manifests for the config editor, cached briefly so
    /// the Run panel does not rescan the tree every frame.
    pub(super) fn discovered_manifests(&mut self, now: f64) -> Vec<VPath> {
        const MANIFEST_CACHE_SECS: f64 = 2.0;
        if let Some((at, manifests)) = &self.manifest_cache
            && now - at < MANIFEST_CACHE_SECS
        {
            return manifests.clone();
        }
        let manifests = full_stack::runconfig::discover_build_manifests(self.workspace.fs())
            .unwrap_or_default();
        self.manifest_cache = Some((now, manifests.clone()));
        manifests
    }

    pub(super) fn selected_config(&self) -> Option<&RunConfig> {
        self.configs().get(self.selected_config)
    }

    /// The [debug] tab's name: the config's root source basename, falling
    /// back to the config name when the root cannot be resolved.
    fn machine_tab_label_for(&self, config: &RunConfig) -> String {
        self.config_root(config)
            .and_then(|root| root.as_str().rsplit('/').next().map(str::to_owned))
            .unwrap_or_else(|| config.name.clone())
    }

    /// Resolve the root source file a config compiles, if it can be located.
    fn config_root(&self, config: &RunConfig) -> Option<VPath> {
        match &config.build {
            BuildRef::Manifest(path) => self
                .build_target_from_manifest(path.clone())
                .map(|target| target.root_path),
            BuildRef::SourceWithDefaults { root, .. } => Some(root.clone()),
        }
    }

    /// Make `idx` the active config and open its root, so the spine and stage views
    /// reflect what Build/Run will act on.
    pub(super) fn select_config(&mut self, idx: usize) {
        let Some(config) = self.configs().get(idx).cloned() else {
            return;
        };
        self.selected_config = idx;
        self.mount_host_entry.clear();
        self.mount_guest_entry.clear();
        self.live = None;
        self.launch_cache = None;
        self.last_build = None;
        self.vm_result = None;
        self.machine_texture = None;
        self.machine_texture_fill_count = None;
        self.reset_machine_views();
        if let Some(root) = self.config_root(&config)
            && self.workspace.open_document(root).is_ok()
        {
            self.reset_layer_for_active_file();
        }
        self.stage_cache = None;
        self.mark_build_stale();
    }

    /// Open the selected config's root, then build it (lighting the spine to the
    /// furthest stage produced).
    /// Flush unsaved edits before a build/run when the setting is on, so the
    /// compiled image matches what the editor shows.
    fn save_dirty_before_run(&mut self) {
        if self.settings.save_all_before_run && self.workspace.dirty_documents().next().is_some() {
            self.workspace.save_all().ok();
        }
    }

    pub(super) fn build_selected_config(&mut self, now: f64) {
        self.save_dirty_before_run();
        if let Some(config) = self.selected_config().cloned()
            && let Some(root) = self.config_root(&config)
            && self.workspace.open_document(root).is_ok()
        {
            self.reset_layer_for_active_file();
        }
        self.build_active_document(now);
    }

    /// Build and run the selected config; a `debug = true` config starts paused.
    pub(super) fn run_selected_config(&mut self, now: f64) {
        let debug = self
            .selected_config()
            .is_some_and(|config| config.debug.enabled);
        self.launch_selected_config(now, debug);
    }

    /// Build the selected config and start it paused at the entry point with
    /// the disassembly view focused, regardless of the config's debug flag.
    pub(super) fn debug_selected_config(&mut self, now: f64) {
        self.launch_selected_config(now, true);
    }

    /// Restart the selected config using the current session's launch mode.
    /// Reuses the last build when the sources are unchanged.
    pub(super) fn restart_selected_config(&mut self, now: f64) {
        let debug = if self.live.is_some() {
            self.current_launch_debug
        } else {
            self.selected_config()
                .is_some_and(|config| config.debug.enabled)
        };
        self.launch_selected_config_inner(now, debug, true);
    }

    /// Build the selected config, then boot its ELF as a live VM session.
    fn launch_selected_config(&mut self, now: f64, debug: bool) {
        self.launch_selected_config_inner(now, debug, false);
    }

    fn launch_selected_config_inner(&mut self, now: f64, debug: bool, allow_reuse: bool) {
        self.save_dirty_before_run();
        let Some(config) = self.selected_config().cloned() else {
            self.status_message = Some("no run config selected".to_owned());
            self.terminal.start("Run", "fsc run".to_owned());
            self.terminal
                .push(ConsoleStream::Error, "No run configuration is selected.");
            self.terminal_open = self.settings.open_terminal_on_run;
            return;
        };
        let run_target = self
            .config_root(&config)
            .map(|path| path.to_string())
            .unwrap_or_else(|| config.name.clone());
        let command = if debug {
            format!("fsc run {run_target} # debug session")
        } else {
            format!("fsc run {run_target}")
        };
        self.terminal.start("Run", command);
        self.terminal_open = self.settings.open_terminal_on_run;

        if let Some(root) = self.config_root(&config)
            && self.workspace.open_document(root).is_ok()
        {
            self.reset_layer_for_active_file();
        }
        self.refresh_build_target();
        self.refresh_stage_cache();

        self.live = None;
        self.vm_result = None;
        self.machine_texture = None;
        self.machine_texture_fill_count = None;
        self.reset_machine_views();
        if allow_reuse
            && self.settings.restart_reuses_build
            && self.try_relaunch_cached(&config, now, debug)
        {
            return;
        }
        self.terminal
            .push(ConsoleStream::Info, "Building run target.");
        let out_dir = self.workspace.fs().host_out_dir();
        let engine = SessionEngine::new(self.workspace.fs()).with_artifact_root(out_dir);
        match config.kind {
            RunKind::Hosted => match engine.build_config(&config) {
                Ok(build) => {
                    self.report_linked_build(&build);
                    self.start_hosted_live_session(&config, &build.artifacts, now, debug);
                    self.store_launch_cache(&config, CachedLaunch::Hosted(Box::new(build)));
                }
                Err(err) => {
                    self.status_message = Some("run failed".to_owned());
                    self.terminal.push(ConsoleStream::Error, err.to_string());
                }
            },
            RunKind::KernelBoot => match engine.boot_kernel_config(&config) {
                Ok(boot) => {
                    self.report_linked_build(&boot.build);
                    self.terminal.push(
                        ConsoleStream::Success,
                        format!(
                            "Built boot FS: {} entries, {} byte image.",
                            boot.fs_entries.len(),
                            boot.fs_image.len()
                        ),
                    );
                    self.start_kernel_live_session(
                        &config,
                        &boot.build.artifacts,
                        &boot.init,
                        &boot.fs_image,
                        now,
                        debug,
                    );
                    self.store_launch_cache(&config, CachedLaunch::Kernel(Box::new(boot)));
                }
                Err(err) => {
                    self.status_message = Some("run failed".to_owned());
                    self.terminal.push(ConsoleStream::Error, err.to_string());
                }
            },
            RunKind::Freestanding => {
                let msg = "run kind Freestanding is not wired yet".to_owned();
                self.status_message = Some(msg.clone());
                self.vm_result = None;
                self.terminal.push(ConsoleStream::Error, msg);
            }
        }
    }

    /// Reboot from the last launch's artifacts when the config and its source
    /// fingerprint are unchanged. True when the session started from the cache.
    fn try_relaunch_cached(&mut self, config: &RunConfig, now: f64, debug: bool) -> bool {
        let Some(cache) = self.launch_cache.take() else {
            return false;
        };
        let fingerprint =
            full_stack::session::config_source_fingerprint(self.workspace.fs(), config);
        if cache.config != *config || cache.fingerprint != fingerprint {
            return false;
        }
        self.terminal.push(
            ConsoleStream::Info,
            "Sources unchanged; restarting from the last build.",
        );
        match &cache.launch {
            CachedLaunch::Hosted(build) => {
                self.start_hosted_live_session(config, &build.artifacts, now, debug);
            }
            CachedLaunch::Kernel(boot) => {
                self.start_kernel_live_session(
                    config,
                    &boot.build.artifacts,
                    &boot.init,
                    &boot.fs_image,
                    now,
                    debug,
                );
            }
        }
        self.launch_cache = Some(cache);
        true
    }

    fn store_launch_cache(&mut self, config: &RunConfig, launch: CachedLaunch) {
        let fingerprint =
            full_stack::session::config_source_fingerprint(self.workspace.fs(), config);
        self.launch_cache = Some(LaunchCache {
            config: config.clone(),
            fingerprint,
            launch,
        });
    }

    fn report_linked_build(&mut self, build: &full_stack::session::BuildOutcome) {
        // Retain the outcome for the inspector's Artifacts/Symbols panes.
        self.last_build = Some(build.clone());
        self.terminal.push(
            ConsoleStream::Success,
            format!(
                "Linked {}, target {:?}.",
                diagnostics::tally(build.artifacts.units.len() as u64, "unit"),
                build.target
            ),
        );
        if let Some(dir) = &build.artifact_dir {
            self.terminal.push(
                ConsoleStream::Info,
                format!("Wrote .ir/.s/.elf artifacts to {}", dir.display()),
            );
        }
    }

    /// Boot the built program as a resident session and focus the machine console.
    fn start_hosted_live_session(
        &mut self,
        config: &RunConfig,
        artifacts: &full_stack::build::BuildArtifacts,
        now: f64,
        debug: bool,
    ) {
        let tab_label = self.machine_tab_label_for(config);
        match LiveMachine::new_hosted(artifacts) {
            Ok(mut live) => {
                if !config.run.autorun.is_empty() {
                    live.push_uart_line(&config.run.autorun);
                }
                self.live = Some(live);
                self.machine_tab_label = Some(tab_label);
                self.apply_persisted_breakpoints(config);
                self.activate_machine_terminal_for_run();
                self.machine_view = self.default_machine_view();
                self.layer = Self::source_layer_for_kind(self.active_source_kind());
                self.build_glow_start = Some(now);
                self.record_build_lit(Some(Layer::Vm));
                self.current_launch_debug = debug;
                if debug {
                    self.enter_debug_pause();
                }
            }
            Err(err) => {
                self.status_message = Some("run failed".to_owned());
                self.terminal.push(ConsoleStream::Error, err);
            }
        }
    }

    fn start_kernel_live_session(
        &mut self,
        config: &RunConfig,
        artifacts: &full_stack::build::BuildArtifacts,
        init: &AssembledOutput,
        fs_image: &[u8],
        now: f64,
        debug: bool,
    ) {
        let tab_label = self.machine_tab_label_for(config);
        match LiveMachine::new_kernel(artifacts, Some(init), Some(fs_image), &config.run.autorun) {
            Ok(live) => {
                self.live = Some(live);
                self.machine_tab_label = Some(tab_label);
                self.apply_persisted_breakpoints(config);
                self.activate_machine_terminal_for_run();
                self.machine_view = self.default_machine_view();
                self.focus_machine_tab();
                self.build_glow_start = Some(now);
                self.record_build_lit(Some(Layer::Vm));
                self.current_launch_debug = debug;
                if debug {
                    self.enter_debug_pause();
                }
            }
            Err(err) => {
                self.status_message = Some("kernel boot failed".to_owned());
                self.terminal.push(ConsoleStream::Error, err);
            }
        }
    }

    /// Debug launch: hold the fresh session at its entry point and focus the
    /// disassembly so the first step is visible.
    fn enter_debug_pause(&mut self) {
        if let Some(live) = &mut self.live {
            live.pause();
        }
        // Source-mapped sessions land on the pc-following Code view; images
        // without a line table fall back to the disassembly.
        self.machine_view = if self.live.as_ref().is_some_and(LiveMachine::has_line_table) {
            debug_view::MachineView::Code
        } else {
            debug_view::MachineView::Disasm
        };
        self.focus_machine_tab();
        self.terminal.push(
            ConsoleStream::Info,
            "Paused at entry. Step over (F10), step in (F11), or resume (F5).",
        );
    }

    fn activate_machine_terminal_for_run(&mut self) {
        if self.settings.focus_machine_console_on_run {
            self.terminal.set_active_machine();
        }
    }

    /// Per-frame drive of the resident session; on exit/fault/stop it produces
    /// the post-mortem `vm_result` the dock, Inspect, and terminal already read.
    fn drive_live_machine(&mut self, ctx: &egui::Context) {
        let budget_steps = self
            .selected_config()
            .map(|config| config.run.max_steps_per_frame)
            .unwrap_or(self.settings.default_steps_per_frame)
            .max(1);
        let wall_budget =
            std::time::Duration::from_millis(self.settings.frame_budget_ms.clamp(1, 32));
        let Some(live) = &mut self.live else {
            return;
        };
        if live.state == MachineState::Running {
            live.drive(budget_steps, wall_budget);
            ctx.request_repaint();
        }
        self.latch_live_result();
    }

    /// Record the final result and terminal summary once the session stops
    /// being alive; shared by the frame driver and the [debug] tab close.
    fn latch_live_result(&mut self) {
        let Some(live) = &self.live else {
            return;
        };
        if live.state.is_alive() || self.vm_result.is_some() {
            return;
        }
        self.vm_result = Some(live.result());
        let (stream, summary) = match &live.state {
            MachineState::Exited(0) => (
                ConsoleStream::Success,
                format!("exit 0; {}.", diagnostics::tally(live.steps, "VM step")),
            ),
            MachineState::Exited(code) => (
                ConsoleStream::Warning,
                format!(
                    "exit {code}; {}.",
                    diagnostics::tally(live.steps, "VM step")
                ),
            ),
            MachineState::Stopped => (
                ConsoleStream::Info,
                format!("stopped; {}.", diagnostics::tally(live.steps, "VM step")),
            ),
            MachineState::Faulted(msg) => {
                let at = live
                    .source_for_pc(live.registers().pc)
                    .map(|(file, line)| format!(" ({file}:{line})"))
                    .unwrap_or_default();
                (ConsoleStream::Error, format!("fault: {msg}{at}"))
            }
            MachineState::Running | MachineState::Paused => unreachable!("checked above"),
        };
        self.status_message = Some(summary.clone());
        self.terminal.push(stream, summary);
    }

    /// Dock transport: stop the session, keeping its final state inspectable.
    pub(super) fn stop_live_machine(&mut self) {
        if let Some(live) = &mut self.live {
            live.stop();
        }
    }

    /// The [debug] tab is focused when the machine surface owns the canvas.
    pub(super) fn machine_tab_focused(&self) -> bool {
        self.live.is_some() && self.layer == Layer::Vm
    }

    /// Bring the machine surface to the canvas (the [debug] tab, or the bare
    /// L4 rung when no session exists).
    pub(super) fn focus_machine_tab(&mut self) {
        self.layer = Layer::Vm;
        self.settings_open = false;
    }

    /// Close the [debug] tab: end the session and return to the active file.
    pub(super) fn close_machine_tab(&mut self) {
        self.stop_live_machine();
        self.latch_live_result();
        self.live = None;
        self.reset_machine_views();
        self.reset_layer_for_active_file();
    }

    /// F10: single-step the live machine using the dock's step unit. Instruction
    /// stepping steps over calls; use F11 / Shift+F11 to step in / out.
    pub(super) fn step_selected_live_machine(&mut self) {
        let Some(live) = &mut self.live else {
            return;
        };
        match self.step_unit {
            machine_dock::StepUnit::Line => live.step_line(),
            machine_dock::StepUnit::Instruction => live.step_over(),
            machine_dock::StepUnit::Cycle => live.step_cycle(),
        }
    }

    pub(super) fn persist_live_breakpoints(&mut self) {
        if !self.settings.persist_breakpoints {
            return;
        }
        let Some(live) = &self.live else {
            return;
        };
        let mut specs = live
            .breakpoints
            .iter()
            .filter_map(|addr| live.breakpoint_spec_for_addr(*addr))
            .collect::<Vec<_>>();
        specs.sort();
        specs.dedup();
        let Some(config) = self
            .workspace
            .run_configs_mut()
            .configs
            .get_mut(self.selected_config)
        else {
            return;
        };
        if config.debug.breakpoints != specs {
            config.debug.breakpoints = specs;
            self.persist_run_configs();
        }
    }

    fn apply_persisted_breakpoints(&mut self, config: &RunConfig) {
        let Some(live) = &mut self.live else {
            return;
        };
        for spec in &config.debug.breakpoints {
            match live.resolve_breakpoint(spec) {
                Ok(addr) => {
                    live.breakpoints.insert(addr);
                }
                Err(err) => {
                    self.terminal.push(
                        ConsoleStream::Warning,
                        format!("Breakpoint `{spec}` was not restored: {err}."),
                    );
                }
            }
        }
    }

    /// Diagnostic markers for the active editor, valid only while the buffer
    /// matches the compiled source (edits clear the stage cache, so no drift).
    pub(super) fn active_diagnostic_marks(&self, text: &str) -> Vec<diagnostics::DiagMark> {
        let (Some(cache), Some(target), Some(active)) = (
            &self.stage_cache,
            &self.active_build_target,
            self.active_path(),
        ) else {
            return Vec::new();
        };
        if active != target.root_path || cache.source != text {
            return Vec::new();
        }
        diagnostics::marks_for_source(&cache.result.diagnostics, &cache.source)
    }

    /// Clear per-session debug/inspect state when the session goes away.
    fn reset_machine_views(&mut self) {
        self.machine_view = debug_view::MachineView::Console;
        self.inspector.clear_session();
        self.code_follow = None;
        self.code_follow_scrolled = None;
    }

    fn default_machine_view(&self) -> debug_view::MachineView {
        if self.config_has_screen() {
            debug_view::MachineView::Screen
        } else {
            debug_view::MachineView::Console
        }
    }

    /// True when the selected config presents a framebuffer screen.
    pub(super) fn config_has_screen(&self) -> bool {
        use full_stack::runconfig::PresentMode;
        self.selected_config().is_some_and(|config| {
            matches!(
                config.run.present,
                PresentMode::Framebuffer | PresentMode::Both
            )
        })
    }

    fn find_nearest_build_manifest(&self, path: &VPath) -> Option<VPath> {
        let mut dir = path.parent()?;
        loop {
            let entries = self.workspace.fs().read_dir(&dir).ok()?;
            let mut builds = entries
                .into_iter()
                .filter(|entry| entry.name.ends_with(".build"))
                .map(|entry| entry.path)
                .collect::<Vec<_>>();
            builds.sort();
            if let Some(build) = builds.into_iter().next() {
                return Some(build);
            }
            dir = dir.parent()?;
        }
    }

    fn stdlib_objects_for(&mut self, mode: TargetMode) -> Option<&[(String, AssembledOutput)]> {
        if !self.stdlib_objects.iter().any(|(m, _)| *m == mode) {
            match CompilationPipeline::compile_stdlib_objects(mode) {
                Ok(objects) => self.stdlib_objects.push((mode, objects)),
                Err(err) => {
                    self.status_message = Some(format!("stdlib build failed: {err}"));
                    return None;
                }
            }
        }
        self.stdlib_objects
            .iter()
            .find(|(m, _)| *m == mode)
            .map(|(_, objects)| objects.as_slice())
    }
}

impl FullStackApp {
    /// Global keyboard shortcuts: save/build/run, debug transport, and the
    /// find/search/quick-open surfaces. Runs once per frame before layout.
    fn handle_shortcuts(&mut self, ui: &egui::Ui, now: f64) {
        if self.guest_input_active() && ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            return;
        }
        if ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_all_documents();
        } else if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_active_document();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::B)) {
            self.build_selected_config(now);
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::R)) {
            self.run_selected_config(now);
        }
        // Debug transport: F5 resume-or-run (shift: debug), F6 pause, F10 uses
        // the dock's step selectors. F11 enters calls; Shift+F11 steps out.
        if ui.input(|i| i.key_pressed(egui::Key::F5)) {
            let shift = ui.input(|i| i.modifiers.shift);
            match &mut self.live {
                Some(live) if live.state == MachineState::Paused => live.resume(),
                Some(live) if live.state.is_alive() => {}
                _ if shift => self.debug_selected_config(now),
                _ => self.run_selected_config(now),
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::F6))
            && let Some(live) = &mut self.live
        {
            live.pause();
        }
        if ui.input(|i| i.key_pressed(egui::Key::F10)) && self.live.is_some() {
            self.step_selected_live_machine();
        }
        if ui.input(|i| i.key_pressed(egui::Key::F11))
            && let Some(live) = &mut self.live
        {
            if ui.input(|i| i.modifiers.shift) {
                live.step_out();
            } else if self.step_unit == machine_dock::StepUnit::Line {
                live.step_line_in();
            } else {
                live.step_in();
            }
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::J)) {
            self.terminal_open = !self.terminal_open;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::W)) {
            self.close_active_path();
        }
        if ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::F)) {
            self.tool = Tool::Search;
            self.search.request_focus();
        } else if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            self.find.request_open();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::P)) {
            self.quick_open = Some(quick_open::QuickOpenState::default());
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.settings_open = !self.settings_open;
        }
        if self.settings_open && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }
    }

    fn guest_input_active(&self) -> bool {
        let alive = self.live.as_ref().is_some_and(|live| live.state.is_alive());
        alive
            && ((self.terminal_open && self.terminal.active == TerminalKind::Machine)
                || (self.layer == Layer::Vm
                    && self.machine_view == debug_view::MachineView::Screen))
    }
}

fn build_summary(result: &PipelineResult, produced_layer: Option<Layer>) -> String {
    let stage = produced_layer
        .map(|layer| format!("{} {}", layer.tag(), layer.name()))
        .unwrap_or_else(|| "no stage".to_owned());
    let sections = result
        .binary
        .as_ref()
        .map(|binary| binary.assembled.sections_iter().count())
        .unwrap_or(0);
    if sections > 0 {
        format!(
            "Build completed through {stage}; linked {}.",
            diagnostics::tally(sections as u64, "section")
        )
    } else {
        format!("Build completed through {stage}.")
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
        let bezel_band = egui::Frame::new().fill(theme::BEZEL);
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
            .frame(bezel_band)
            .show_inside(ui, |ui| self.activity_rail(ui));

        egui::Panel::right("machine_dock")
            .resizable(true)
            .default_size(metrics::MACHINE_DOCK_DEFAULT)
            .size_range(metrics::MACHINE_DOCK_MIN..=metrics::MACHINE_DOCK_MAX)
            .frame(well_band)
            .show_inside(ui, |ui| self.machine_dock(ui));

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
    }
}

/// Shared foreground layer so chrome hairlines and column separators paint above
/// panel fills without being clipped to a single panel.
fn separator_painter(ui: &egui::Ui) -> egui::Painter {
    ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("shell_separators"),
    ))
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

/// Native prefers the real on-disk demo workspace; the browser opens the same
/// tree as an in-memory sample.
fn open_default_workspace() -> IdeWorkspace {
    open_native_demo_workspace()
        .or_else(|| IdeWorkspace::sample().ok())
        .unwrap_or_default()
}

/// The workspace to open on startup: the last folder when reopen is enabled and
/// it still exists (native only), otherwise the bundled sample workspace.
fn open_startup_workspace(settings: &settings::AppSettings) -> IdeWorkspace {
    if let Some(workspace) = reopen_last_workspace(settings) {
        return workspace;
    }
    open_default_workspace()
}

#[cfg(not(target_arch = "wasm32"))]
fn reopen_last_workspace(settings: &settings::AppSettings) -> Option<IdeWorkspace> {
    if !settings.reopen_last_workspace {
        return None;
    }
    let path = settings.last_workspace.as_ref()?;
    IdeWorkspace::new(Box::new(NativeFs::open(path).ok()?)).ok()
}

#[cfg(target_arch = "wasm32")]
fn reopen_last_workspace(_settings: &settings::AppSettings) -> Option<IdeWorkspace> {
    None
}

/// Open `programs/demo_workspace` as a real folder, materializing any missing
/// bundled sources and run configs first.
#[cfg(not(target_arch = "wasm32"))]
fn open_native_demo_workspace() -> Option<IdeWorkspace> {
    let root = std::path::PathBuf::from(DEMO_WORKSPACE_DIR);
    std::fs::create_dir_all(&root).ok()?;
    let mut fs = NativeFs::open(root).ok()?;
    full_stack::workspace::populate_demo_workspace(&mut fs).ok()?;
    IdeWorkspace::new(Box::new(fs)).ok()
}

#[cfg(target_arch = "wasm32")]
fn open_native_demo_workspace() -> Option<IdeWorkspace> {
    None
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use full_stack::workspace::MemoryFs;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // A hosted preview must resolve `_start`->`main`, and a kernel preview must link
    // against the kernel stdlib (`_kernel_start`/`kmain`) with no phantom undefined `main`.
    #[test]
    fn preview_links_hosted_and_kernel_without_phantom_main() {
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));

        let hosted = BuildManifest::hosted("prev_hosted", "main: () -> i32 { return 0 }");
        let hosted_bin = app
            .build_preview_binary(&hosted, "prev_hosted.hll", "main: () -> i32 { return 0 }")
            .expect("hosted preview links");
        assert!(hosted_bin.assembled.has_symbol("main"));

        let kernel_build =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/os-runtime/kernel/kernel.build");
        let manifest = BuildManifest::from_file(&kernel_build).expect("kernel manifest");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("crates/os-runtime/kernel/core/my_kernel.hll");
        let source = fs::read_to_string(&root).expect("read kernel root");
        let kernel_bin = app
            .build_preview_binary(&manifest, &root.display().to_string(), &source)
            .expect("kernel preview links without phantom main");
        assert!(kernel_bin.assembled.has_symbol("kmain"));
    }

    #[test]
    fn open_native_workspace_replaces_tree_configs_and_transient_state() {
        let root = temp_root("open_workspace_replaces");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.vm_result = Some(VmExecutionResult {
            uart_output: "old".to_owned(),
            exit_code: Some(0),
            steps: 1,
            max_steps_reached: false,
            registers: None,
        });
        app.machine_texture_fill_count = Some(7);
        app.manifest_cache = Some((1.0, vec![VPath::parse("old.build").expect("path")]));
        app.quick_open = Some(quick_open::QuickOpenState::default());
        app.find.request_open();
        app.current_launch_debug = true;
        app.selected_config = 12;

        app.open_native_workspace_folder(&root);

        assert_eq!(app.configs().len(), 1);
        assert_eq!(app.selected_config, 0);
        assert_eq!(
            app.active_path().expect("active root").as_str(),
            "programs/demo/main.hll"
        );
        assert!(
            app.workspace
                .tree()
                .flatten()
                .iter()
                .any(|entry| entry.path.as_str() == "programs/demo/demo.build")
        );
        assert!(app.vm_result.is_none());
        assert!(app.machine_texture.is_none());
        assert!(app.machine_texture_fill_count.is_none());
        assert!(app.manifest_cache.is_none());
        assert!(app.quick_open.is_none());
        assert!(!app.find.open);
        assert!(!app.current_launch_debug);
        assert!(matches!(app.machine_view, debug_view::MachineView::Console));
        assert!(app.status_message.expect("status").starts_with("opened "));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reopen_last_workspace_restores_the_folder_only_when_enabled() {
        let root = temp_root("reopen_last");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));

        // Opening a folder records it as the last workspace. The stored path is
        // the canonicalized root, so match on the unique temp-folder name.
        app.open_native_workspace_folder(&root);
        let label = app.settings.last_workspace.clone().expect("last recorded");
        let dir_name = root.file_name().expect("dir name").to_string_lossy();
        assert!(label.contains(dir_name.as_ref()), "stored path was {label}");

        // Off (the default): startup falls back to the bundled sample workspace.
        let mut settings = settings::AppSettings {
            last_workspace: Some(label.clone()),
            ..settings::AppSettings::default()
        };
        assert!(reopen_last_workspace(&settings).is_none());

        // On with a live path: startup reopens that folder's configs.
        settings.reopen_last_workspace = true;
        let reopened = reopen_last_workspace(&settings).expect("reopened");
        assert_eq!(reopened.run_configs().configs.len(), 1);

        // On but the path is gone: fall back rather than fail startup.
        fs::remove_dir_all(&root).expect("cleanup");
        assert!(reopen_last_workspace(&settings).is_none());
    }

    #[test]
    fn open_native_workspace_blocks_when_documents_are_dirty() {
        let root = temp_root("open_workspace_dirty");
        write_native_workspace(&root);
        let mut fs = MemoryFs::new();
        let old_path = VPath::parse("old/main.hll").expect("path");
        fs.write_text(&old_path, "old").expect("write old");
        let mut app = test_app(IdeWorkspace::new(Box::new(fs)).expect("workspace"));
        app.workspace
            .open_document(old_path.clone())
            .expect("open old");
        app.workspace.set_active_text("dirty").expect("edit old");

        app.open_native_workspace_folder(&root);

        assert_eq!(
            app.status_message.as_deref(),
            Some("save or close unsaved files before opening a folder")
        );
        assert_eq!(
            app.active_path().expect("active old").as_str(),
            old_path.as_str()
        );
        assert_eq!(app.workspace.dirty_documents().count(), 1);
        assert!(app.configs().is_empty());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn dirty_close_confirmation_blocks_discard_until_confirmed() {
        let mut fs = MemoryFs::new();
        let path = VPath::parse("main.hll").expect("path");
        fs.write_text(&path, "main: () -> i32 { return 0 }")
            .expect("write source");
        let mut app = test_app(IdeWorkspace::new(Box::new(fs)).expect("workspace"));
        app.workspace
            .open_document(path.clone())
            .expect("open source");
        app.workspace
            .set_active_text("main: () -> i32 { return 1 }")
            .expect("edit source");
        app.settings.confirm_discard_on_close = true;

        app.close_path(&path);

        assert_eq!(app.pending_close.as_ref(), Some(&path));
        assert!(app.workspace.document(&path).is_some());

        app.close_path_confirmed(&path);

        assert!(app.pending_close.is_none());
        assert!(app.workspace.document(&path).is_none());
    }

    #[test]
    fn open_terminal_on_run_setting_can_keep_terminal_closed() {
        let root = temp_root("terminal_closed");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);
        app.settings.open_terminal_on_run = false;

        app.run_selected_config(1.0);

        assert!(!app.terminal_open);
        assert!(app.live.is_some(), "run still starts a machine session");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn restart_reuses_the_last_build_until_sources_change() {
        let root = temp_root("restart_reuse");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);

        app.run_selected_config(1.0);
        assert!(app.launch_cache.is_some(), "first run fills the cache");
        assert_eq!(drive_app_to_exit(&mut app), 0);

        app.restart_selected_config(2.0);
        let output = app.terminal.output_text();
        assert!(
            output.contains("Sources unchanged; restarting from the last build."),
            "restart reused the build: {output}"
        );
        assert!(!output.contains("Building run target."));
        assert_eq!(drive_app_to_exit(&mut app), 0, "cached artifacts still run");

        fs::write(
            root.join("programs/demo/main.hll"),
            "main: () -> i32 { return 9 }",
        )
        .expect("edit source");
        app.restart_selected_config(3.0);
        let output = app.terminal.output_text();
        assert!(
            output.contains("Building run target."),
            "source edit forces a rebuild: {output}"
        );
        assert_eq!(drive_app_to_exit(&mut app), 9, "rebuild picked up the edit");

        fs::remove_dir_all(root).expect("cleanup");
    }

    // [debug] tab lifecycle: launch focuses it, file tabs unfocus it without
    // ending the session, closing it ends the session and restores the file.
    #[test]
    fn debug_tab_focuses_switches_and_closes() {
        let root = temp_root("debug_tab_lifecycle");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);

        app.debug_selected_config(1.0);
        assert!(app.live.is_some(), "debug launch starts a session");
        assert!(app.machine_tab_focused());
        assert_eq!(app.machine_tab_label.as_deref(), Some("main.hll"));

        app.document_focus_changed();
        assert!(!app.machine_tab_focused(), "file tab takes the canvas back");
        assert!(app.live.is_some(), "switching tabs keeps the session");

        app.focus_machine_tab();
        assert!(app.machine_tab_focused());

        app.close_machine_tab();
        assert!(app.live.is_none(), "closing the tab ends the session");
        assert!(app.vm_result.is_some(), "the final result is latched");
        assert!(app.layer != Layer::Vm, "the canvas returns to the file");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn restart_reuse_setting_off_forces_rebuild() {
        let root = temp_root("restart_reuse_off");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);

        app.run_selected_config(1.0);
        assert!(app.launch_cache.is_some());
        assert_eq!(drive_app_to_exit(&mut app), 0);

        app.settings.restart_reuses_build = false;
        app.restart_selected_config(2.0);

        let output = app.terminal.output_text();
        assert!(
            output.contains("Building run target."),
            "restart should rebuild when reuse is disabled: {output}"
        );
        assert!(
            !output.contains("Sources unchanged; restarting from the last build."),
            "reuse message should not appear when disabled: {output}"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn persist_breakpoints_setting_off_leaves_run_config_unchanged() {
        let root = temp_root("persist_breakpoints_off");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);
        app.settings.persist_breakpoints = false;

        app.debug_selected_config(1.0);
        let live = app.live.as_mut().expect("live session");
        let main = live.resolve_breakpoint("main").expect("main symbol");
        live.breakpoints.insert(main);
        app.persist_live_breakpoints();

        assert!(
            app.selected_config()
                .expect("selected config")
                .debug
                .breakpoints
                .is_empty()
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn idle_check_compiles_the_dirty_buffer_without_saving() {
        let root = temp_root("idle_check");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);
        app.workspace
            .open_document(VPath::parse("programs/demo/main.hll").expect("path"))
            .expect("open");
        app.workspace
            .set_active_text("main: () -> i32 {\n    return )\n}\n")
            .expect("edit");

        let ctx = egui::Context::default();
        app.poll_idle_check(&ctx, 0.0);
        assert!(app.stage_cache.is_none(), "debounce holds until idle");
        app.poll_idle_check(&ctx, 2.0);
        let cache = app.stage_cache.as_ref().expect("idle check ran");
        assert!(cache.result.has_errors(), "unsaved parse error detected");

        // The same edit is not rechecked; a new edit re-arms the debounce.
        app.stage_cache = None;
        app.poll_idle_check(&ctx, 4.0);
        assert!(app.stage_cache.is_none(), "unchanged buffer not rechecked");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn build_failure_marks_the_editor_and_links_output() {
        let root = temp_root("diag_markers");
        write_native_workspace(&root);
        fs::write(
            root.join("programs/demo/main.hll"),
            "main: () -> i32 {\n    return )\n}\n",
        )
        .expect("break source");
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);

        app.build_selected_config(1.0);
        assert_eq!(app.status_message.as_deref(), Some("build failed"));
        let text = app
            .workspace
            .active_document()
            .expect("active doc")
            .text()
            .to_owned();
        let marks = app.active_diagnostic_marks(&text);
        assert!(!marks.is_empty(), "parse error produced editor marks");
        assert!(marks[0].error);
        assert_eq!(marks[0].line, 2, "span points at the broken line");
        let output = app.terminal.output_text();
        assert!(
            output.contains("programs/demo/main.hll:2:"),
            "output line carries the location: {output}"
        );
        assert!(
            output.contains("return )") && output.contains('^'),
            "output shows the rendered block with source line and caret: {output}"
        );
        assert!(
            !output.contains("Freestanding errors:"),
            "the stringified compile error does not repeat the block: {output}"
        );

        // An Output link click routes through jump_to_source without reopening
        // the already-active file (the stage cache and marks must survive).
        app.jump_to_source("programs/demo/main.hll", 2);
        assert_eq!(
            app.pending_jump
                .as_ref()
                .map(|(path, line)| (path.as_str(), *line)),
            Some(("programs/demo/main.hll", 2))
        );
        assert!(!app.active_diagnostic_marks(&text).is_empty());

        fs::remove_dir_all(root).expect("cleanup");
    }

    // A build without a run must leave artifacts inspectable, and a pane that
    // lost its backing state must fall back to Overview instead of going dead.
    #[test]
    fn build_retains_artifacts_and_inspector_falls_back() {
        let root = temp_root("inspector_last_build");
        write_native_workspace(&root);
        let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
        app.open_native_workspace_folder(&root);
        assert!(app.last_build.is_none());

        app.build_selected_config(1.0);
        let build = app.last_build.as_ref().expect("build retained");
        assert!(build.artifacts.linked.has_symbol("main"));

        app.inspector.pane = inspector::InspectorPane::Processes;
        app.ensure_inspector_pane_available();
        assert_eq!(app.inspector.pane, inspector::InspectorPane::Overview);
        app.inspector.pane = inspector::InspectorPane::Artifacts;
        app.ensure_inspector_pane_available();
        assert_eq!(app.inspector.pane, inspector::InspectorPane::Artifacts);

        fs::remove_dir_all(root).expect("cleanup");
    }

    fn drive_app_to_exit(app: &mut FullStackApp) -> i32 {
        let live = app.live.as_mut().expect("live session");
        for _ in 0..1_000 {
            match live.state {
                MachineState::Exited(code) => return code,
                MachineState::Running => {
                    live.drive(10_000, std::time::Duration::from_secs(5));
                }
                _ => break,
            }
        }
        panic!("session did not exit: {:?}", live.state);
    }

    fn test_app(workspace: IdeWorkspace) -> FullStackApp {
        FullStackApp {
            workspace,
            tool: Tool::Files,
            layer: Layer::Hll,
            layout: ShellLayout::default(),
            stage_cache: None,
            stdlib_objects: Vec::new(),
            status_message: None,
            build_glow_start: Some(1.0),
            build_lit_until: Some(Layer::Vm),
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
            cursor_pos: Some((1, 1)),
            machine_view: debug_view::MachineView::Disasm,
            machine_tab_label: None,
            memory_view: debug_view::MemoryViewState::default(),
            code_follow: None,
            code_follow_scrolled: None,
            current_launch_debug: false,
            step_count: 100,
            step_unit: machine_dock::StepUnit::Instruction,
            time_scrub: 0,
            inspector: inspector::InspectorState {
                pane: inspector::InspectorPane::Trace,
                pid: Some(1),
                inode: Some(2),
                ..Default::default()
            },
            last_build: None,
            settings: settings::AppSettings::default(),
            settings_open: false,
        }
    }

    fn write_native_workspace(root: &Path) {
        fs::create_dir_all(root.join(".fullstack")).expect("create config dir");
        fs::create_dir_all(root.join("programs/demo")).expect("create program dir");
        fs::write(
            root.join(".fullstack/run.toml"),
            "\
[[config]]
name = \"Temp Demo\"
type = \"hosted\"
build = \"programs/demo/demo.build\"
present = \"terminal\"
max_steps_per_frame = 200000
debug = false
",
        )
        .expect("write run config");
        fs::write(
            root.join("programs/demo/demo.build"),
            "\
name = \"temp_demo\"
target = \"hosted\"
root = \"main.hll\"
stdlib = \"hosted\"
",
        )
        .expect("write manifest");
        fs::write(
            root.join("programs/demo/main.hll"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("full_stack_{name}_{nanos}"))
    }
}
