use crate::compilation_pipeline::TargetMode;
use crate::workspace::VPath;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfig {
    pub name: String,
    /// `name` was inherited from the build manifest's `title`, not written in
    /// run.toml; it is resolved on load and left out again on save.
    pub name_from_manifest: bool,
    pub kind: RunKind,
    pub build: BuildRef,
    pub fs: FsImageSpec,
    pub run: RunSpec,
    pub debug: DebugSpec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunKind {
    Hosted,
    KernelBoot,
    Freestanding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildRef {
    Manifest(VPath),
    SourceWithDefaults { root: VPath, target: TargetMode },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FsImageSpec {
    pub mounts: Vec<FsMount>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FsMount {
    pub host: VPath,
    pub guest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunSpec {
    pub present: PresentMode,
    pub autorun: String,
    pub max_steps_per_frame: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentMode {
    Terminal,
    Framebuffer,
    Both,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DebugSpec {
    pub enabled: bool,
    pub breakpoints: Vec<String>,
}

impl Default for RunSpec {
    fn default() -> Self {
        Self {
            present: PresentMode::Both,
            autorun: String::new(),
            max_steps_per_frame: 200_000,
        }
    }
}

impl RunConfig {
    pub fn hosted(name: impl Into<String>, build: VPath) -> Self {
        Self {
            name: name.into(),
            name_from_manifest: false,
            kind: RunKind::Hosted,
            build: BuildRef::Manifest(build),
            fs: FsImageSpec::default(),
            run: RunSpec::default(),
            debug: DebugSpec::default(),
        }
    }
}
