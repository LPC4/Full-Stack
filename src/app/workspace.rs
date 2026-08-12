use super::{
    BuildRef, BuildSource, BuildTarget, FullStackApp, IdeWorkspace, LaunchMode, MainSurface,
    RunConfig, SourceKind, VPath, debug_view, explorer, find, inspector, search, settings,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{LESSONS_WORKSPACE_DIR, NativeFs, Path};

// The browser opens these as in-memory trees; native materializes them on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspacePreset {
    Demo,
    Lessons,
}

impl WorkspacePreset {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Demo => "demo workspace",
            Self::Lessons => "lessons",
        }
    }

    pub(super) fn memory_workspace(self) -> std::io::Result<IdeWorkspace> {
        match self {
            Self::Demo => IdeWorkspace::sample(),
            Self::Lessons => IdeWorkspace::lessons(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReferenceLocation {
    pub(super) preset: WorkspacePreset,
    pub(super) workspace_path: VPath,
}

pub(super) fn reference_location(repository_path: &VPath) -> Option<ReferenceLocation> {
    let path = repository_path.as_str();
    if let Some(local) = path.strip_prefix("programs/lessons/") {
        return Some(ReferenceLocation {
            preset: WorkspacePreset::Lessons,
            workspace_path: VPath::parse(local).ok()?,
        });
    }
    if path.starts_with("programs/example/")
        || path.starts_with("programs/user/")
        || path.starts_with("crates/os-runtime/")
    {
        return Some(ReferenceLocation {
            preset: WorkspacePreset::Demo,
            workspace_path: repository_path.clone(),
        });
    }
    None
}

impl FullStackApp {
    pub(super) fn open_repository_reference(
        &mut self,
        reference: crate::web_reference::ReferenceDocument,
        status: Option<String>,
    ) {
        if crate::web_reference::is_rust_source(&reference.path) {
            self.status_message = Some(format!(
                "Rust source `{}` is available on GitHub",
                reference.path
            ));
            return;
        }
        let crate::web_reference::ReferenceDocument {
            path: repository_path,
            bytes,
            binary,
            line,
        } = reference;
        let location = reference_location(&repository_path);
        let workspace_path = location.as_ref().map_or_else(
            || repository_path.clone(),
            |location| location.workspace_path.clone(),
        );
        let opened_workspace_document =
            location.is_some() && self.workspace.open_document(workspace_path.clone()).is_ok();
        if !opened_workspace_document {
            self.workspace
                .inject_read_only_document(workspace_path.clone(), bytes, binary);
        }
        self.repository_paths
            .insert(workspace_path.clone(), repository_path.clone());

        if let Some(index) = self.config_index_for_path(&workspace_path) {
            self.select_config(index);
        } else {
            self.selected_config = self.configs().len();
        }
        if let Some(line) = line {
            self.pending_jump = Some((workspace_path.clone(), line));
            self.reference_jump = Some((workspace_path.clone(), line));
        }
        self.activate_path(&workspace_path);
        self.status_message = Some(status.unwrap_or_else(|| {
            if opened_workspace_document {
                format!("opened editable workspace file `{repository_path}`")
            } else {
                format!("opened read-only repository file `{repository_path}`")
            }
        }));
    }

    pub(super) fn active_path(&self) -> Option<VPath> {
        self.workspace
            .active_document()
            .map(|doc| doc.path().clone())
    }

    pub(super) fn open_path(&mut self, path: VPath) {
        if self.workspace.open_document(path).is_ok() {
            self.document_focus_changed();
        }
    }

    pub(super) fn activate_path(&mut self, path: &VPath) {
        self.workspace.activate_document(path);
        self.document_focus_changed();
    }

    // Unsaved edits are discarded; the tab's dirty dot is the only warning.
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
        self.repository_paths.remove(path);
        if self
            .reference_jump
            .as_ref()
            .is_some_and(|(reference, _)| reference == path)
        {
            self.reference_jump = None;
        }
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

    pub(super) fn block_open_folder_for_dirty_documents(&mut self) -> bool {
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

    pub(super) fn open_workspace_preset(&mut self, preset: WorkspacePreset) {
        if self.block_open_folder_for_dirty_documents() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((workspace, root)) = open_materialized_workspace(preset) {
            self.settings.last_workspace = Some(root.display().to_string());
            self.replace_workspace(workspace, preset.label());
            return;
        }
        match preset.memory_workspace() {
            Ok(workspace) => self.replace_workspace(workspace, preset.label()),
            Err(err) => {
                self.status_message = Some(format!("open {} failed: {err}", preset.label()));
            }
        }
    }

    pub(super) fn replace_workspace(&mut self, workspace: IdeWorkspace, label: &str) {
        self.workspace = workspace;
        self.live = None;
        self.launch_cache = None;
        self.pending_jump = None;
        self.stage_locate = None;
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
        self.launch_mode = LaunchMode::Run;
        self.memory_view = debug_view::MemoryViewState::default();
        self.inspector = inspector::InspectorState::default();
        self.last_build = None;
        self.repository_paths.clear();
        self.reference_jump = None;
        self.reset_machine_views();
        self.mark_build_stale();
        self.selected_config = 0;
        self.reset_layer_for_active_file();
        if !self.workspace.run_configs().configs.is_empty() {
            self.select_config(0);
        }
        self.status_message = Some(format!("opened {label}"));
        self.sync_reference_address();
    }

    pub(super) fn document_focus_changed(&mut self) {
        self.main_surface = MainSurface::Document;
        self.reset_layer_for_active_file();
        // Switching tabs must not dim the lit spine; only edits and builds may.
        self.stage_cache = None;
        self.stage_locate = None;
        self.sync_reference_address();
    }

    pub(super) fn reference_address_state(&mut self) -> (Option<VPath>, Option<usize>) {
        let active = self.active_path();
        let file = active
            .as_ref()
            .and_then(|path| self.repository_paths.get(path))
            .cloned();
        if self
            .reference_jump
            .as_ref()
            .is_some_and(|(path, _)| active.as_ref() != Some(path))
        {
            self.reference_jump = None;
        }
        let line = self
            .reference_jump
            .as_ref()
            .filter(|(path, _)| active.as_ref() == Some(path))
            .map(|(_, line)| *line);
        (file, line)
    }

    pub(super) fn sync_reference_address(&mut self) {
        let (file, line) = self.reference_address_state();
        #[cfg(target_arch = "wasm32")]
        crate::web_reference::replace_address_bar(file.as_ref(), line);
        #[cfg(not(target_arch = "wasm32"))]
        let _unused = (file, line);
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

    pub(super) fn refresh_build_target(&mut self) {
        let Some(active_path) = self.active_path() else {
            self.active_build_target = None;
            return;
        };
        self.active_build_target = self.find_build_target_for(&active_path);
    }

    pub(super) fn find_build_target_for(&self, path: &VPath) -> Option<BuildTarget> {
        let manifest_path = if path.as_str().ends_with(".build") {
            path.clone()
        } else {
            self.find_build_manifest_for_source(path)?
        };
        self.build_target_from_manifest(manifest_path)
    }

    pub(super) fn find_build_manifest_for_source(&self, path: &VPath) -> Option<VPath> {
        let parent = path.parent()?;
        let file_name = path.file_name()?;
        let stem = file_name
            .rsplit_once('.')
            .map_or(file_name, |(stem, _extension)| stem);
        let sibling = parent.join(&format!("{stem}.build")).ok()?;
        if self.workspace.fs().metadata(&sibling).is_ok() {
            return Some(sibling);
        }
        self.find_nearest_build_manifest(path)
    }

    pub(super) fn config_index_for_path(&self, path: &VPath) -> Option<usize> {
        let manifest = self.find_build_target_for(path)?.manifest_path;
        self.configs().iter().position(
            |config| matches!(&config.build, BuildRef::Manifest(path) if path == &manifest),
        )
    }

    pub(super) fn build_target_from_manifest(&self, manifest_path: VPath) -> Option<BuildTarget> {
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

    pub(super) fn persist_run_configs(&mut self) {
        if let Err(err) = self.workspace.save_run_configs() {
            self.status_message = Some(format!("saving run configs failed: {err}"));
        }
    }

    // Cached briefly so the Run panel does not rescan the tree every frame.
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

    pub(super) fn session_tab_label_for(&self, config: &RunConfig) -> String {
        self.config_root(config)
            .and_then(|root| root.as_str().rsplit('/').next().map(str::to_owned))
            .unwrap_or_else(|| config.name.clone())
    }

    pub(super) fn config_root(&self, config: &RunConfig) -> Option<VPath> {
        match &config.build {
            BuildRef::Manifest(path) => self
                .build_target_from_manifest(path.clone())
                .map(|target| target.root_path),
            BuildRef::SourceWithDefaults { root, .. } => Some(root.clone()),
        }
    }

    // Opens its root so the spine and stage views reflect what Build/Run act on.
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

    // Flush unsaved edits before build/run so the compiled image matches the editor.
    pub(super) fn find_nearest_build_manifest(&self, path: &VPath) -> Option<VPath> {
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
}

pub(super) fn open_default_workspace() -> IdeWorkspace {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((workspace, _root)) = open_materialized_workspace(WorkspacePreset::Demo) {
        return workspace;
    }
    IdeWorkspace::sample().ok().unwrap_or_default()
}

pub(super) fn open_startup_workspace(settings: &settings::AppSettings) -> IdeWorkspace {
    if let Some(workspace) = reopen_last_workspace(settings) {
        return workspace;
    }
    open_default_workspace()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn reopen_last_workspace(settings: &settings::AppSettings) -> Option<IdeWorkspace> {
    if !settings.reopen_last_workspace {
        return None;
    }
    let path = settings.last_workspace.as_ref()?;
    IdeWorkspace::new(Box::new(NativeFs::open(path).ok()?)).ok()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn reopen_last_workspace(_settings: &settings::AppSettings) -> Option<IdeWorkspace> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn preset_dir(preset: WorkspacePreset) -> Option<std::path::PathBuf> {
    match preset {
        WorkspacePreset::Demo => {
            eframe::storage_dir("Full-Stack").map(|root| demo_workspace_dir(&root))
        }
        WorkspacePreset::Lessons => Some(LESSONS_WORKSPACE_DIR.into()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn demo_workspace_dir(storage_root: &Path) -> std::path::PathBuf {
    storage_root.join("workspaces").join("demo")
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn open_materialized_workspace(
    preset: WorkspacePreset,
) -> Option<(IdeWorkspace, std::path::PathBuf)> {
    let root = preset_dir(preset)?;
    std::fs::create_dir_all(&root).ok()?;
    let mut fs = NativeFs::open(&root).ok()?;
    match preset {
        WorkspacePreset::Demo => full_stack::workspace::populate_demo_workspace(&mut fs).ok()?,
        WorkspacePreset::Lessons => {
            full_stack::workspace::populate_lessons_workspace(&mut fs).ok()?;
        }
    }
    let workspace = IdeWorkspace::new(Box::new(fs)).ok()?;
    Some((workspace, root))
}
