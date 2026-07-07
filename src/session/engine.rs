use crate::build::{BuildArtifacts, BuildExecutor, BuildManifest, BuildSource};
use crate::compilation_pipeline::{FsEntry, TargetMode, assembled_to_elf_file, build_fs_image};
use crate::runconfig::{BuildRef, RunConfig};
use crate::userspace::{self, UserProgram};
use crate::workspace::{
    EntryKind, VPath, WorkspaceFs, WorkspaceManifestDiagnostic, load_build_manifest,
};
use asm_to_binary::AssembledOutput;
use std::collections::BTreeSet;
use std::hash::{Hash as _, Hasher};
use std::io;
use std::path::PathBuf;

pub struct SessionEngine<'a> {
    fs: &'a dyn WorkspaceFs,
    artifact_root: Option<PathBuf>,
}

#[derive(Debug)]
pub enum SessionEngineError {
    Io(io::Error),
    Utf8(std::string::FromUtf8Error),
    Manifest(Vec<WorkspaceManifestDiagnostic>),
    Build(crate::build::BuildError),
}

impl std::fmt::Display for SessionEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Utf8(err) => write!(f, "source is not valid UTF-8: {err}"),
            Self::Manifest(diags) => {
                let joined = diags
                    .iter()
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                write!(f, "manifest error:\n{joined}")
            }
            Self::Build(err) => write!(f, "{err}"),
        }
    }
}

pub struct BuildOutcome {
    pub config_name: String,
    pub manifest_name: String,
    pub target: TargetMode,
    pub root: String,
    pub artifacts: BuildArtifacts,
    /// Directory the `.ir`/`.s`/`.elf` artifacts were written to, when enabled.
    pub artifact_dir: Option<PathBuf>,
}

pub struct HostedRunOutcome {
    pub build: BuildOutcome,
    pub vm: crate::session::vm::VmExecutionResult,
}

pub struct KernelBootOutcome {
    pub build: BuildOutcome,
    pub init: AssembledOutput,
    pub fs_image: Vec<u8>,
    pub fs_entries: Vec<OwnedFsEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedFsEntry {
    Dir { path: String },
    File { path: String, data: Vec<u8> },
}

impl<'a> SessionEngine<'a> {
    pub fn new(fs: &'a dyn WorkspaceFs) -> Self {
        Self {
            fs,
            artifact_root: None,
        }
    }

    /// Emit build artifacts (`.ir`/`.s`/`.elf`) under `root` when building.
    pub fn with_artifact_root(mut self, root: Option<PathBuf>) -> Self {
        self.artifact_root = root;
        self
    }

    pub fn build_config(&self, config: &RunConfig) -> Result<BuildOutcome, SessionEngineError> {
        let mut manifest = self.manifest_for_config(config)?;
        let artifact_dir = self.apply_artifact_root(&mut manifest);
        let plan = BuildExecutor::plan(&manifest);
        let artifacts = BuildExecutor::build(&manifest).map_err(SessionEngineError::Build)?;
        Ok(BuildOutcome {
            config_name: config.name.clone(),
            manifest_name: plan.name,
            target: plan.target,
            root: plan.root,
            artifacts,
            artifact_dir,
        })
    }

    /// Route artifact writes to the engine's `out` directory unless the manifest
    /// already pins its own. Returns the directory that will receive artifacts.
    fn apply_artifact_root(&self, manifest: &mut BuildManifest) -> Option<PathBuf> {
        if let Some(existing) = &manifest.artifact_root {
            return manifest.write_artifacts.then(|| existing.clone());
        }
        let root = self.artifact_root.clone()?;
        manifest.artifact_root = Some(root.clone());
        manifest.write_artifacts = true;
        Some(root)
    }

    pub fn run_hosted_config(
        &self,
        config: &RunConfig,
        max_steps: u64,
    ) -> Result<HostedRunOutcome, SessionEngineError> {
        let build = self.build_config(config)?;
        let result = run_hosted_artifact(&build.artifacts, max_steps.max(1));
        Ok(HostedRunOutcome { build, vm: result })
    }

    pub fn boot_kernel_config(
        &self,
        config: &RunConfig,
    ) -> Result<KernelBootOutcome, SessionEngineError> {
        let build = self.build_config(config)?;
        let init = compile_user_program("shell")?;
        let fs_entries = self.kernel_fs_entries(config)?;
        let fs_image = build_owned_fs_image(&fs_entries);
        Ok(KernelBootOutcome {
            build,
            init,
            fs_image,
            fs_entries,
        })
    }

    pub fn kernel_fs_entries(
        &self,
        config: &RunConfig,
    ) -> Result<Vec<OwnedFsEntry>, SessionEngineError> {
        let mut entries = Vec::new();
        let mut dirs = BTreeSet::new();
        for program in userspace::PROGRAMS {
            add_catalog_install(program, &mut entries, &mut dirs)?;
        }
        for mount in &config.fs.mounts {
            collect_mount_entries(self.fs, &mount.host, &mount.guest, &mut entries, &mut dirs)?;
        }
        let mut out = dirs
            .into_iter()
            .map(|path| OwnedFsEntry::Dir { path })
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| a.path().cmp(b.path()));
        out.extend(entries);
        Ok(out)
    }

    fn manifest_for_config(&self, config: &RunConfig) -> Result<BuildManifest, SessionEngineError> {
        match &config.build {
            BuildRef::Manifest(path) => {
                load_build_manifest(self.fs, path).map_err(SessionEngineError::Manifest)
            }
            BuildRef::SourceWithDefaults { root, target } => {
                let source = read_text(self.fs, root)?;
                let name = root
                    .file_name()
                    .unwrap_or("program")
                    .trim_end_matches(".hll")
                    .to_owned();
                let mut manifest = match target {
                    TargetMode::Kernel => BuildManifest::kernel(name.clone(), ""),
                    TargetMode::Hosted | TargetMode::Freestanding => {
                        BuildManifest::hosted(name.clone(), "")
                    }
                };
                manifest.target = *target;
                manifest.root = BuildSource::inline_with_path(name, source, root.as_str());
                Ok(manifest)
            }
        }
    }
}

impl OwnedFsEntry {
    fn path(&self) -> &str {
        match self {
            Self::Dir { path } | Self::File { path, .. } => path,
        }
    }
}

/// Fingerprint of the workspace files a config's build and boot FS read (build
/// subtree + mounts); Restart reuses the last artifacts while it is unchanged.
pub fn config_source_fingerprint(fs: &dyn WorkspaceFs, config: &RunConfig) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let build_dir = match &config.build {
        BuildRef::Manifest(path) => path.parent(),
        BuildRef::SourceWithDefaults { root, .. } => root.parent(),
    };
    if let Some(dir) = build_dir {
        hash_tree(fs, &dir, &mut hasher);
    }
    for mount in &config.fs.mounts {
        hash_tree(fs, &mount.host, &mut hasher);
    }
    hasher.finish()
}

/// Hash paths and file contents under `path`, skipping dot entries and
/// generated `out/` trees so artifact and run-config writes stay invisible.
fn hash_tree(fs: &dyn WorkspaceFs, path: &VPath, hasher: &mut impl Hasher) {
    let Ok(meta) = fs.metadata(path) else {
        // A missing path still perturbs the hash so its appearance is a change.
        path.as_str().hash(hasher);
        return;
    };
    match meta.kind {
        EntryKind::File => {
            path.as_str().hash(hasher);
            if let Ok(data) = fs.read_file(path) {
                data.hash(hasher);
            }
        }
        EntryKind::Dir => {
            let Ok(mut entries) = fs.read_dir(path) else {
                return;
            };
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            for entry in entries {
                if entry.name.starts_with('.') || entry.name == "out" {
                    continue;
                }
                hash_tree(fs, &entry.path, hasher);
            }
        }
    }
}

fn compile_user_program(name: &str) -> Result<AssembledOutput, SessionEngineError> {
    let program = userspace::program(name).ok_or_else(|| {
        SessionEngineError::Build(crate::build::BuildError::Manifest(format!(
            "bundled user program `{name}` not found"
        )))
    })?;
    BuildExecutor::build(&BuildManifest::from_user_program(program))
        .map(|artifacts| artifacts.linked)
        .map_err(SessionEngineError::Build)
}

fn add_catalog_install(
    program: &UserProgram,
    entries: &mut Vec<OwnedFsEntry>,
    dirs: &mut BTreeSet<String>,
) -> Result<(), SessionEngineError> {
    let Some(path) = program.install_path else {
        return Ok(());
    };
    add_parent_dirs(path, dirs);
    let data = if program.is_compiled() {
        assembled_to_elf_file(&compile_user_program(program.name)?)
    } else {
        program.source.as_bytes().to_vec()
    };
    entries.push(OwnedFsEntry::File {
        path: normalize_guest_path(path),
        data,
    });
    Ok(())
}

fn collect_mount_entries(
    fs: &dyn WorkspaceFs,
    host: &VPath,
    guest: &str,
    entries: &mut Vec<OwnedFsEntry>,
    dirs: &mut BTreeSet<String>,
) -> Result<(), SessionEngineError> {
    let guest = normalize_guest_path(guest);
    add_parent_dirs(&guest, dirs);
    match fs.metadata(host).map_err(SessionEngineError::Io)?.kind {
        EntryKind::Dir => {
            dirs.insert(guest.clone());
            for child in fs.read_dir(host).map_err(SessionEngineError::Io)? {
                let child_guest = join_guest_path(&guest, &child.name);
                collect_mount_entries(fs, &child.path, &child_guest, entries, dirs)?;
            }
        }
        EntryKind::File => {
            let data = fs.read_file(host).map_err(SessionEngineError::Io)?;
            entries.push(OwnedFsEntry::File { path: guest, data });
        }
    }
    Ok(())
}

fn build_owned_fs_image(entries: &[OwnedFsEntry]) -> Vec<u8> {
    let borrowed = entries
        .iter()
        .map(|entry| match entry {
            OwnedFsEntry::Dir { path } => FsEntry::Dir { path },
            OwnedFsEntry::File { path, data } => FsEntry::File { path, data },
        })
        .collect::<Vec<_>>();
    build_fs_image(&borrowed)
}

fn add_parent_dirs(path: &str, dirs: &mut BTreeSet<String>) {
    let path = normalize_guest_path(path);
    let mut cursor = 0usize;
    while let Some(offset) = path[cursor + 1..].find('/') {
        let end = cursor + 1 + offset;
        dirs.insert(path[..end].to_owned());
        cursor = end;
    }
}

fn normalize_guest_path(path: &str) -> String {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.starts_with('/') {
        trimmed
    } else {
        format!("/{trimmed}")
    }
}

fn join_guest_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn run_hosted_artifact(
    artifacts: &BuildArtifacts,
    max_steps: u64,
) -> crate::session::vm::VmExecutionResult {
    use asm_to_binary::assembler::link_layout::LinkLayout;
    use virtual_machine::cpu::StepOutcome;
    use virtual_machine::virtual_machine::VirtualMachine;

    let elf = artifacts
        .linked
        .to_elf_with_entry(LinkLayout::hosted().load_base, "_start");
    let mut vm = match VirtualMachine::from_elf(&elf) {
        Ok(vm) => vm,
        Err(err) => {
            return crate::session::vm::VmExecutionResult {
                uart_output: format!("ELF load failed: {err}"),
                exit_code: None,
                steps: 0,
                max_steps_reached: false,
                registers: None,
            };
        }
    };
    let result = vm.run(max_steps);
    let mut x = [0u64; 32];
    for (r, slot) in x.iter_mut().enumerate() {
        *slot = vm.peek_reg(r);
    }
    let registers = Some(crate::session::vm::RegisterSnapshot {
        pc: vm.peek_pc(),
        x,
    });
    crate::session::vm::VmExecutionResult {
        uart_output: result.uart_output,
        exit_code: match result.outcome {
            StepOutcome::Halted(code) => Some(code as i32),
            StepOutcome::Continue => None,
        },
        steps: result.steps,
        max_steps_reached: matches!(result.outcome, StepOutcome::Continue),
        registers,
    }
}

fn read_text(fs: &dyn WorkspaceFs, path: &VPath) -> Result<String, SessionEngineError> {
    let bytes = fs.read_file(path).map_err(SessionEngineError::Io)?;
    String::from_utf8(bytes).map_err(SessionEngineError::Utf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runconfig::{RunConfig, RunConfigStore};
    use crate::session::{LiveMachine, MachineState};
    use crate::workspace::{MemoryFs, VPath, sample_workspace};
    use std::time::Duration;

    #[test]
    fn builds_hosted_run_config_from_workspace_manifest() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"none\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo/demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");
        let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));

        let outcome = SessionEngine::new(&fs)
            .build_config(&config)
            .expect("build config");
        assert_eq!(outcome.config_name, "Demo");
        assert_eq!(outcome.manifest_name, "demo");
        assert!(outcome.artifacts.linked.has_symbol("main"));
    }

    #[test]
    fn runs_hosted_run_config_in_vm() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"hosted\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo/demo.hll").expect("path"),
            "main: () -> i32 { return 7 }",
        )
        .expect("write source");
        let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));

        let outcome = SessionEngine::new(&fs)
            .run_hosted_config(&config, 20_000)
            .expect("run config");
        assert_eq!(outcome.vm.exit_code, Some(7));
        assert!(!outcome.vm.max_steps_reached);
    }

    #[test]
    fn build_writes_ir_asm_elf_to_artifact_root() {
        let out = std::env::temp_dir().join(format!(
            "full_stack_engine_out_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"none\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo/demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");
        let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));

        let outcome = SessionEngine::new(&fs)
            .with_artifact_root(Some(out.clone()))
            .build_config(&config)
            .expect("build config");

        assert_eq!(outcome.artifact_dir.as_deref(), Some(out.as_path()));
        assert!(out.join("ir/demo.ir").is_file(), "IR artifact written");
        assert!(out.join("asm/demo.s").is_file(), "ASM artifact written");
        assert!(
            out.join("elf/total_demo.elf").is_file(),
            "ELF artifact written"
        );

        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn fingerprint_tracks_build_and_mount_sources_only() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"none\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo/demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");
        fs.write_text(&VPath::parse("data/config.txt").expect("path"), "v1")
            .expect("write mounted file");
        let mut config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));
        config.fs.mounts.push(crate::runconfig::FsMount {
            host: VPath::parse("data").expect("path"),
            guest: "/data".to_owned(),
        });

        let baseline = config_source_fingerprint(&fs, &config);
        assert_eq!(
            baseline,
            config_source_fingerprint(&fs, &config),
            "fingerprint is stable"
        );

        fs.write_text(&VPath::parse("demo/out/asm/demo.s").expect("path"), "asm")
            .expect("write artifact");
        fs.write_text(
            &VPath::parse("demo/.fullstack/run.toml").expect("path"),
            "x",
        )
        .expect("write run config");
        assert_eq!(
            baseline,
            config_source_fingerprint(&fs, &config),
            "out/ and dot entries are invisible"
        );

        fs.write_text(
            &VPath::parse("demo/demo.hll").expect("path"),
            "main: () -> i32 { return 1 }",
        )
        .expect("edit source");
        let edited = config_source_fingerprint(&fs, &config);
        assert_ne!(baseline, edited, "source edits change the fingerprint");

        fs.write_text(&VPath::parse("data/config.txt").expect("path"), "v2")
            .expect("edit mounted file");
        assert_ne!(
            edited,
            config_source_fingerprint(&fs, &config),
            "mounted file edits change the fingerprint"
        );
    }

    #[test]
    fn kernel_boot_builds_fs_from_catalog_and_mounts() {
        let fs = sample_workspace().expect("sample workspace");
        let store = RunConfigStore::load(&fs).expect("run config store");
        let config = store
            .configs
            .iter()
            .find(|config| config.kind == crate::runconfig::RunKind::KernelBoot)
            .expect("kernel config");

        let entries = SessionEngine::new(&fs)
            .kernel_fs_entries(config)
            .expect("fs entries");
        assert!(
            entries.iter().any(|entry| entry.path() == "/bin/as.elf"),
            "compiled bundled tools are installed under /bin"
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.path() == "/home/demo/bin/shell/shell.hll"),
            "workspace mounts are copied into the boot FS"
        );
    }

    #[test]
    fn kernel_config_boots_shell_through_live_machine() {
        let fs = sample_workspace().expect("sample workspace");
        let store = RunConfigStore::load(&fs).expect("run config store");
        let config = store
            .configs
            .iter()
            .find(|config| config.kind == crate::runconfig::RunKind::KernelBoot)
            .expect("kernel config");

        let boot = SessionEngine::new(&fs)
            .boot_kernel_config(config)
            .expect("boot kernel config");
        let mut live = LiveMachine::new_kernel(
            &boot.build.artifacts,
            Some(&boot.init),
            Some(&boot.fs_image),
            "",
        )
        .expect("live kernel");

        for _ in 0..1_000 {
            live.drive(100_000, Duration::from_secs(5));
            if live
                .uart_log
                .contains("HLL shell ready. type 'help' for commands.")
            {
                return;
            }
            assert!(
                matches!(live.state, MachineState::Running | MachineState::Paused),
                "kernel session ended unexpectedly: {:?}\n{}",
                live.state,
                live.uart_log
            );
        }
        panic!("shell banner did not appear; uart={}", live.uart_log);
    }
}
