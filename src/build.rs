use crate::compilation_pipeline::{CompilationError, CompilationPipeline, TargetMode};
use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::{AssembledOutput, LinkerError};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use virtual_machine::bus::MemoryLatencyConfig;

pub type BuildModuleResolver = Rc<dyn Fn(&str) -> Option<String> + 'static>;

/// Build description for one linkable HLL program. Data-only by design:
/// `BuildExecutor` owns object ordering and pipeline setup.
#[derive(Clone)]
pub struct BuildManifest {
    pub name: String,
    pub target: TargetMode,
    pub entry: Option<String>,
    pub root: BuildSource,
    pub stdlib: StdlibPolicy,
    pub import_closure: ImportClosurePolicy,
    pub source_prelude: Option<String>,
    pub string_prefix: Option<String>,
    pub link_layout: Option<LinkLayout>,
    pub module_resolver: Option<BuildModuleResolver>,
    pub artifact_root: Option<PathBuf>,
    pub extra_objects: Vec<BuildObject>,
    pub abi_exports: Vec<String>,
    pub latency: Option<MemoryLatencyConfig>,
    pub run_semantic_analysis: bool,
    pub write_artifacts: bool,
}

impl BuildManifest {
    pub fn hosted(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            root: BuildSource::inline(name.clone(), source),
            name,
            target: TargetMode::Hosted,
            entry: None,
            stdlib: StdlibPolicy::ForTarget,
            import_closure: ImportClosurePolicy::Enabled {
                mangle_symbols: true,
            },
            source_prelude: None,
            string_prefix: Some("_u_".to_owned()),
            link_layout: None,
            module_resolver: None,
            artifact_root: None,
            extra_objects: Vec::new(),
            abi_exports: vec!["main".to_owned()],
            latency: None,
            run_semantic_analysis: false,
            write_artifacts: false,
        }
    }

    pub fn kernel(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            root: BuildSource::inline(name.clone(), source),
            name,
            target: TargetMode::Kernel,
            entry: None,
            stdlib: StdlibPolicy::Explicit(TargetMode::Kernel),
            import_closure: ImportClosurePolicy::Enabled {
                mangle_symbols: false,
            },
            source_prelude: None,
            string_prefix: Some("__kern_str_".to_owned()),
            link_layout: None,
            module_resolver: None,
            artifact_root: None,
            extra_objects: Vec::new(),
            abi_exports: Vec::new(),
            latency: None,
            run_semantic_analysis: true,
            write_artifacts: false,
        }
    }

    pub fn from_user_program(program: &crate::userspace::UserProgram) -> Self {
        let mut manifest = Self::hosted(program.name, program.source);
        manifest.root =
            BuildSource::inline_with_path(program.name, program.source, program.source_path);
        manifest.import_closure = if program.import_closure {
            ImportClosurePolicy::Enabled {
                mangle_symbols: program.mangle_symbols,
            }
        } else {
            ImportClosurePolicy::Disabled
        };
        manifest.source_prelude = (!program.layout.is_empty()).then(|| program.layout.to_owned());
        manifest
    }

    /// Defaults for a raw-assembly root: no stdlib (the hosted runtime glue would
    /// collide on `_start`), no import closure, no HLL ABI exports.
    pub fn apply_asm_root_defaults(&mut self) {
        self.stdlib = StdlibPolicy::None;
        self.import_closure = ImportClosurePolicy::Disabled;
        self.abi_exports.clear();
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self, BuildError> {
        let path = path.into();
        let text = fs::read_to_string(&path).map_err(BuildError::Io)?;
        let base_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let decoded = decode_build_manifest(&text).map_err(BuildError::Manifest)?;
        let root_path = resolve_manifest_path(base_dir, &decoded.root);
        let fallback_name = || {
            root_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("program")
                .to_owned()
        };
        let source_prelude = if let Some(path) = &decoded.source_prelude {
            let path = resolve_manifest_path(base_dir, path);
            Some(fs::read_to_string(path).map_err(BuildError::Io)?)
        } else {
            None
        };
        let artifact_root = decoded
            .out
            .as_deref()
            .map(|out| resolve_manifest_path(base_dir, out));
        let name = decoded.name.clone().unwrap_or_else(fallback_name);
        let asm_root = is_asm_path(&root_path.to_string_lossy());
        Ok(decoded.into_manifest(
            name.clone(),
            BuildSource::path(name, root_path),
            asm_root,
            source_prelude,
            artifact_root,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DecodedBuildManifest {
    pub root: String,
    pub name: Option<String>,
    pub target: TargetMode,
    pub entry: Option<String>,
    pub stdlib: Option<StdlibPolicy>,
    pub import_closure: Option<bool>,
    pub mangle_symbols: Option<bool>,
    pub source_prelude: Option<String>,
    pub abi_exports: Option<Vec<String>>,
    pub out: Option<String>,
    pub latency: Option<MemoryLatencyConfig>,
}

impl DecodedBuildManifest {
    pub(crate) fn into_manifest(
        self,
        name: String,
        root: BuildSource,
        asm_root: bool,
        source_prelude: Option<String>,
        artifact_root: Option<PathBuf>,
    ) -> BuildManifest {
        let mut manifest = match self.target {
            TargetMode::Kernel => BuildManifest::kernel(name.clone(), ""),
            TargetMode::Hosted | TargetMode::Freestanding => {
                BuildManifest::hosted(name.clone(), "")
            }
        };
        manifest.name = name;
        manifest.target = self.target;
        manifest.root = root;
        if asm_root {
            manifest.apply_asm_root_defaults();
        }
        manifest.entry = self.entry;
        manifest.stdlib = self.stdlib.unwrap_or(manifest.stdlib);
        if let Some(enabled) = self.import_closure {
            manifest.import_closure = if enabled {
                ImportClosurePolicy::Enabled {
                    mangle_symbols: !matches!(self.target, TargetMode::Kernel),
                }
            } else {
                ImportClosurePolicy::Disabled
            };
        }
        if let Some(mangle_symbols) = self.mangle_symbols
            && matches!(manifest.import_closure, ImportClosurePolicy::Enabled { .. })
        {
            manifest.import_closure = ImportClosurePolicy::Enabled { mangle_symbols };
        }
        manifest.source_prelude = source_prelude;
        if let Some(exports) = self.abi_exports {
            manifest.abi_exports = exports;
        }
        manifest.write_artifacts = self.out.is_some();
        manifest.artifact_root = artifact_root;
        manifest.latency = self.latency;
        manifest
    }
}

pub(crate) fn decode_build_manifest(text: &str) -> Result<DecodedBuildManifest, String> {
    let values = fs_utils::build_manifest::parse(text).map_err(|err| err.0)?;
    let root = values
        .string("root")
        .ok_or_else(|| "build manifest missing `root`".to_owned())?;
    let target = values
        .get("target")
        .map(parse_target_mode)
        .transpose()?
        .unwrap_or(TargetMode::Hosted);
    let stdlib = values
        .get("stdlib")
        .map(|value| parse_stdlib_policy(value, target))
        .transpose()?;
    let import_closure = values.bool("import_closure").map_err(|err| err.0)?;
    let mangle_symbols = values.bool("mangle_symbols").map_err(|err| err.0)?;
    let abi_exports = values.list("abi_exports").map_err(|err| err.0)?;
    let latency = parse_memory_latency_config(&values)?;
    Ok(DecodedBuildManifest {
        root,
        name: values.string("name"),
        target,
        entry: values.string("entry"),
        stdlib,
        import_closure,
        mangle_symbols,
        source_prelude: values.string("source_prelude"),
        abi_exports,
        out: values.string("out"),
        latency,
    })
}

#[derive(Clone, Debug)]
pub enum BuildSource {
    Inline {
        name: String,
        source: String,
        source_path: Option<PathBuf>,
    },
    Path {
        name: String,
        path: PathBuf,
    },
}

impl BuildSource {
    pub fn inline(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self::Inline {
            name: name.into(),
            source: source.into(),
            source_path: None,
        }
    }

    pub fn inline_with_path(
        name: impl Into<String>,
        source: impl Into<String>,
        source_path: impl Into<PathBuf>,
    ) -> Self {
        Self::Inline {
            name: name.into(),
            source: source.into(),
            source_path: Some(source_path.into()),
        }
    }

    pub fn path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::Path {
            name: name.into(),
            path: path.into(),
        }
    }

    fn load(&self) -> Result<LoadedSource, BuildError> {
        match self {
            Self::Inline {
                name,
                source,
                source_path,
            } => Ok(LoadedSource {
                name: name.clone(),
                source: source.clone(),
                source_path: source_path.clone(),
            }),
            Self::Path { name, path } => Ok(LoadedSource {
                name: name.clone(),
                source: fs::read_to_string(path).map_err(BuildError::Io)?,
                source_path: Some(path.clone()),
            }),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Inline { name, .. } | Self::Path { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuildObject {
    pub name: String,
    pub assembled: AssembledOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdlibPolicy {
    None,
    ForTarget,
    Explicit(TargetMode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportClosurePolicy {
    Disabled,
    Enabled { mangle_symbols: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildPlan {
    pub name: String,
    pub target: TargetMode,
    pub entry: Option<String>,
    pub stdlib: StdlibPolicy,
    pub import_closure: ImportClosurePolicy,
    pub root: String,
    pub extra_objects: Vec<String>,
    pub abi_exports: Vec<String>,
    pub has_source_prelude: bool,
}

#[derive(Clone, Debug)]
pub struct BuiltUnit {
    pub name: String,
    pub assembled: AssembledOutput,
}

#[derive(Clone, Debug)]
pub struct BuildArtifacts {
    pub plan: BuildPlan,
    pub units: Vec<BuiltUnit>,
    pub linked: AssembledOutput,
}

#[derive(Debug)]
pub enum BuildError {
    Io(std::io::Error),
    Manifest(String),
    Compile(CompilationError),
    Assemble(String),
    Link(LinkerError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Manifest(err) => write!(f, "manifest error: {err}"),
            Self::Compile(err) => write!(f, "{err}"),
            Self::Assemble(err) => write!(f, "assembler error: {err}"),
            Self::Link(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for BuildError {}

pub struct BuildExecutor;

impl BuildExecutor {
    pub fn plan(manifest: &BuildManifest) -> BuildPlan {
        BuildPlan {
            name: manifest.name.clone(),
            target: manifest.target,
            entry: manifest.entry.clone(),
            stdlib: manifest.stdlib,
            import_closure: manifest.import_closure,
            root: manifest.root.name().to_owned(),
            extra_objects: manifest
                .extra_objects
                .iter()
                .map(|object| object.name.clone())
                .collect(),
            abi_exports: manifest.abi_exports.clone(),
            has_source_prelude: manifest.source_prelude.is_some(),
        }
    }

    pub fn build(manifest: &BuildManifest) -> Result<BuildArtifacts, BuildError> {
        let plan = Self::plan(manifest);
        let mut units: Vec<BuiltUnit> = Vec::new();

        let mut prunable = std::collections::HashSet::new();
        for (name, assembled) in Self::compile_stdlib(manifest)? {
            prunable.insert(name.clone());
            units.push(BuiltUnit { name, assembled });
        }

        for object in &manifest.extra_objects {
            units.push(BuiltUnit {
                name: object.name.clone(),
                assembled: object.assembled.clone(),
            });
        }

        let root = manifest.root.load()?;
        let mut pipeline = Self::pipeline_for(manifest);
        pipeline.set_current_source_path(root.source_path.clone());

        if root.is_asm() {
            let mut unit = Self::assemble_asm_root(&mut pipeline, &root)?;
            Self::mark_abi_exports(manifest, &mut unit.assembled);
            units.push(unit);
        } else {
            match manifest.import_closure {
                ImportClosurePolicy::Enabled { mangle_symbols } => {
                    pipeline.set_module_mangling(mangle_symbols);
                    for (name, mut assembled) in pipeline
                        .compile_program_closure(&root.name, &root.source)
                        .map_err(BuildError::Compile)?
                    {
                        if name == root.name {
                            Self::mark_abi_exports(manifest, &mut assembled);
                        }
                        units.push(BuiltUnit { name, assembled });
                    }
                }
                ImportClosurePolicy::Disabled => {
                    let mut root_unit = Self::compile_one(&mut pipeline, &root)?;
                    Self::mark_abi_exports(manifest, &mut root_unit.assembled);
                    units.push(root_unit);
                }
            }
        }

        let units = Self::prune_unreferenced_stdlib(units, &prunable, manifest.entry.as_deref());

        let modules: Vec<(&str, &AssembledOutput)> = units
            .iter()
            .map(|unit| (unit.name.as_str(), &unit.assembled))
            .collect();
        let link_pipeline = Self::pipeline_for(manifest);
        let linked = link_pipeline
            .link_assembled_objects_named(&manifest.name, &modules)
            .map_err(BuildError::Link)?;

        Ok(BuildArtifacts {
            plan,
            units,
            linked,
        })
    }

    // Program/extra objects and any object defining an entry symbol are roots.
    fn prune_unreferenced_stdlib(
        units: Vec<BuiltUnit>,
        prunable: &std::collections::HashSet<String>,
        entry: Option<&str>,
    ) -> Vec<BuiltUnit> {
        let entry_roots = ["_start", "_kernel_start"];
        let is_root = |unit: &BuiltUnit| {
            !prunable.contains(&unit.name)
                || unit.assembled.global_symbol_names().any(|symbol| {
                    entry_roots.contains(&symbol) || entry.is_some_and(|entry| entry == symbol)
                })
        };
        let mut kept: Vec<bool> = units.iter().map(is_root).collect();
        let mut needed: std::collections::HashSet<String> = units
            .iter()
            .zip(&kept)
            .filter(|(_, keep)| **keep)
            .flat_map(|(unit, _)| unit.assembled.undefined_symbol_names())
            .map(str::to_owned)
            .collect();
        loop {
            let mut changed = false;
            for (index, unit) in units.iter().enumerate() {
                if kept[index] {
                    continue;
                }
                if unit
                    .assembled
                    .global_symbol_names()
                    .any(|symbol| needed.contains(symbol))
                {
                    kept[index] = true;
                    changed = true;
                    needed.extend(
                        unit.assembled
                            .undefined_symbol_names()
                            .into_iter()
                            .map(str::to_owned),
                    );
                }
            }
            if !changed {
                break;
            }
        }
        units
            .into_iter()
            .zip(kept)
            .filter_map(|(unit, keep)| keep.then_some(unit))
            .collect()
    }

    fn compile_stdlib(
        manifest: &BuildManifest,
    ) -> Result<Vec<(String, AssembledOutput)>, BuildError> {
        let mode = match manifest.stdlib {
            StdlibPolicy::None => return Ok(Vec::new()),
            StdlibPolicy::ForTarget => manifest.target,
            StdlibPolicy::Explicit(mode) => mode,
        };
        CompilationPipeline::compile_stdlib_objects(mode).map_err(BuildError::Compile)
    }

    // The source path feeds the pc-to-source line table so the debugger maps back to it.
    fn assemble_asm_root(
        pipeline: &mut CompilationPipeline,
        source: &LoadedSource,
    ) -> Result<BuiltUnit, BuildError> {
        let file = source
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().replace('\\', "/"));
        let tokens = asm_to_binary::parse_asm_source(file.as_deref(), &source.source);
        pipeline.set_artifact_stem(Some(source.name.clone()));
        let assembled = pipeline
            .assemble_named(&source.name, &tokens)
            .map_err(|err| BuildError::Assemble(err.message))?;
        Ok(BuiltUnit {
            name: source.name.clone(),
            assembled,
        })
    }

    fn compile_one(
        pipeline: &mut CompilationPipeline,
        source: &LoadedSource,
    ) -> Result<BuiltUnit, BuildError> {
        pipeline.set_artifact_stem(Some(source.name.clone()));
        let result = pipeline
            .compile(&source.source)
            .map_err(BuildError::Compile)?;
        let (_, tokens) = pipeline
            .compile_ir_to_assembly_with_tokens(&result.ir_program)
            .map_err(|error| BuildError::Compile(CompilationError::Codegen(error)))?;
        let assembled = pipeline
            .assemble_named(&source.name, &tokens)
            .map_err(|err| BuildError::Assemble(err.message))?;
        Ok(BuiltUnit {
            name: source.name.clone(),
            assembled,
        })
    }

    fn mark_abi_exports(manifest: &BuildManifest, assembled: &mut AssembledOutput) {
        for symbol in &manifest.abi_exports {
            assembled.mark_entry_global(symbol);
        }
    }

    pub fn pipeline_for(manifest: &BuildManifest) -> CompilationPipeline {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(manifest.target);
        pipeline.set_entry_point(manifest.entry.clone());
        pipeline.set_link_layout(manifest.link_layout.clone());
        pipeline.set_run_semantic_analysis(manifest.run_semantic_analysis);
        pipeline.set_write_artifacts(manifest.write_artifacts);
        if let Some(root) = &manifest.artifact_root {
            pipeline.set_artifact_root(root.clone());
        }
        if let Some(prefix) = &manifest.string_prefix {
            pipeline.set_string_prefix(Some(prefix.clone()));
        }
        if let Some(prelude) = &manifest.source_prelude {
            pipeline.set_source_prelude(prelude.clone());
        }
        if let Some(resolver) = &manifest.module_resolver {
            let resolver = Rc::clone(resolver);
            pipeline.set_module_resolver(Some(Box::new(move |name| resolver(name))));
        }
        pipeline
    }
}

pub(crate) fn parse_memory_latency_config(
    values: &fs_utils::build_manifest::Manifest,
) -> Result<Option<MemoryLatencyConfig>, String> {
    const OVERRIDES: [&str; 5] = [
        "latency_l1",
        "latency_l2",
        "latency_l3",
        "latency_dram",
        "latency_uncached",
    ];

    let preset = values.get("latency");
    let first_override = OVERRIDES
        .iter()
        .find(|key| values.get(key).is_some())
        .copied();
    match (preset, first_override) {
        (None | Some("off"), None) => return Ok(None),
        (None, Some(key)) => {
            return Err(format!("`{key}` requires `latency = \"realistic\"`"));
        }
        (Some("off"), Some(key)) => {
            return Err(format!("`{key}` conflicts with `latency = \"off\"`"));
        }
        (Some("realistic"), _) => {}
        (Some(other), _) => return Err(format!("unknown latency preset `{other}`")),
    }

    let mut config = MemoryLatencyConfig::realistic();
    for key in OVERRIDES {
        let Some(raw) = values.get(key) else {
            continue;
        };
        let cycles = raw
            .parse::<u32>()
            .map_err(|err| format!("`{key}` must be a positive integer: {err}"))?;
        if cycles == 0 {
            return Err(format!("`{key}` must be at least 1"));
        }
        match key {
            "latency_l1" => config.l1_hit = cycles,
            "latency_l2" => config.l2_hit = cycles,
            "latency_l3" => config.l3_hit = cycles,
            "latency_dram" => config.dram = cycles,
            "latency_uncached" => config.uncached = cycles,
            _ => unreachable!("override key list is exhaustive"),
        }
    }
    Ok(Some(config))
}

fn parse_target_mode(value: &str) -> Result<TargetMode, String> {
    match value {
        "hosted" => Ok(TargetMode::Hosted),
        "freestanding" => Ok(TargetMode::Freestanding),
        "kernel" => Ok(TargetMode::Kernel),
        _ => Err(format!("unknown target `{value}`")),
    }
}

fn parse_stdlib_policy(value: &str, target: TargetMode) -> Result<StdlibPolicy, String> {
    match value {
        "none" => Ok(StdlibPolicy::None),
        "target" => Ok(StdlibPolicy::Explicit(target)),
        "hosted" => Ok(StdlibPolicy::Explicit(TargetMode::Hosted)),
        "freestanding" => Ok(StdlibPolicy::Explicit(TargetMode::Freestanding)),
        "kernel" => Ok(StdlibPolicy::Explicit(TargetMode::Kernel)),
        _ => Err(format!("unknown stdlib policy `{value}`")),
    }
}

/// True for root paths that are raw assembly rather than HLL.
pub fn is_asm_path(path: &str) -> bool {
    path.ends_with(".s")
}

fn resolve_manifest_path(base_dir: &std::path::Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

#[derive(Clone, Debug)]
struct LoadedSource {
    name: String,
    source: String,
    source_path: Option<PathBuf>,
}

impl LoadedSource {
    fn is_asm(&self) -> bool {
        self.source_path
            .as_ref()
            .is_some_and(|path| is_asm_path(&path.to_string_lossy()))
            || is_asm_path(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BuildExecutor, BuildManifest, BuildSource, ImportClosurePolicy, StdlibPolicy,
        parse_memory_latency_config,
    };
    use crate::compilation_pipeline::TargetMode;

    #[test]
    fn manifest_latency_defaults_to_none() {
        let values = fs_utils::build_manifest::parse("root = \"main.hll\"\n").expect("manifest");
        assert_eq!(parse_memory_latency_config(&values).expect("latency"), None);
    }

    #[test]
    fn manifest_builds_single_hosted_program() {
        let manifest = BuildManifest::hosted(
            "manifest_single",
            r#"
main: () -> i32 {
    return 0
}
"#,
        );

        let artifacts = BuildExecutor::build(&manifest).expect("manifest build");
        assert!(
            artifacts.linked.has_symbol("main"),
            "linked output should contain the entry function"
        );
        assert!(
            artifacts
                .units
                .iter()
                .any(|unit| unit.name == "manifest_single"),
            "root object should be present"
        );
    }

    #[test]
    fn manifest_builds_import_closure_program() {
        let mut manifest = BuildManifest::hosted(
            "manifest_modules",
            r#"
helper := import("helper")

main: () -> i32 {
    return helper.value()
}
"#,
        );
        manifest.root = BuildSource::inline_with_path(
            "manifest_modules",
            r#"
helper := import("helper")

main: () -> i32 {
    return helper.value()
}
"#,
            "manifest_modules.hll",
        );
        manifest.stdlib = StdlibPolicy::Explicit(TargetMode::Hosted);
        manifest.module_resolver = Some(std::rc::Rc::new(|name| {
            (name == "helper").then(|| {
                r#"
export value: () -> i32 {
    return 7
}
"#
                .to_owned()
            })
        }));

        let artifacts = BuildExecutor::build(&manifest).expect("manifest closure build");
        assert!(
            artifacts.linked.has_symbol("helper__value"),
            "linked output should contain imported helper export"
        );
        // The root unit's lines attribute to its own file name (line 5 is the
        // `return` statement), and no entry carries an unusable empty file.
        assert!(
            artifacts
                .linked
                .addr_for_source("manifest_modules.hll", 5)
                .is_some(),
            "root file lines should be in the linked table"
        );
        for entry in artifacts.linked.line_table_iter() {
            if entry.line > 0 {
                let file = artifacts.linked.file_name(entry.file_id).unwrap_or("");
                assert!(!file.is_empty(), "line {} has an empty file", entry.line);
            }
        }
    }

    #[test]
    fn file_manifest_builds_hosted_example() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/example/core_basics/core_basics.build");
        let manifest = BuildManifest::from_file(path).expect("parse hosted build file");
        assert_eq!(manifest.name, "core_basics");
        assert_eq!(manifest.target, TargetMode::Hosted);

        let artifacts = BuildExecutor::build(&manifest).expect("file manifest build");
        assert!(artifacts.linked.has_symbol("main"));
    }

    #[test]
    fn all_example_file_manifests_build() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("programs/example");
        let mut paths = Vec::new();
        collect_build_files(&dir, &mut paths);
        paths.sort();
        assert!(!paths.is_empty(), "expected example build manifests");

        for path in paths {
            let manifest = BuildManifest::from_file(&path)
                .unwrap_or_else(|e| panic!("{}: parse failed: {e}", path.display()));
            let artifacts = BuildExecutor::build(&manifest)
                .unwrap_or_else(|e| panic!("{}: build failed: {e}", path.display()));
            assert!(
                artifacts.linked.has_symbol("main"),
                "{} should link a hosted main",
                path.display()
            );
        }
    }

    fn collect_build_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read examples directory") {
            let path = entry.expect("example directory entry").path();
            if path.is_dir() {
                collect_build_files(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("build") {
                out.push(path);
            }
        }
    }

    // The lesson manifests exercise the raw-asm root path end to end: parse,
    // assemble, link, then run on a bare VM until the spin at `done`.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_sum() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/01_sum/sum.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        assert_eq!(manifest.target, TargetMode::Freestanding);
        assert_eq!(manifest.stdlib, StdlibPolicy::None);
        assert_eq!(manifest.import_closure, ImportClosurePolicy::Disabled);
        assert_eq!(manifest.entry.as_deref(), Some("_start"));

        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        assert!(artifacts.linked.has_symbol("_start"));
        assert!(
            artifacts.units.iter().all(|unit| unit.name == "sum"),
            "asm root should link without stdlib objects"
        );

        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "sum.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(5), 55, "t0 should hold the sum 1..10");
    }

    // Lesson 02 adds .data directives and `la`: the array must survive
    // assembly and linking, and the loop must sum it from memory.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_array_sum() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/02_array_sum/array_sum.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        assert_eq!(manifest.target, TargetMode::Freestanding);
        assert_eq!(manifest.entry.as_deref(), Some("_start"));

        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "array_sum.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(5), 100, "t0 should hold the array sum");
    }

    // Lesson 03 adds stores: the solution must leave the array reversed in
    // memory when it reaches the spin at `done`.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_reverse_array() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/03_reverse_array/reverse_array.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let array = artifacts
            .linked
            .symbol_address("array")
            .expect("array symbol");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "reverse_array.s should still be spinning at done: {:?}",
            result.termination
        );
        let bytes = vm.peek_bytes_raw(virtual_machine::bus::RAM_BASE + array, 32);
        let values: Vec<u64> = bytes
            .chunks(8)
            .map(|c| u64::from_le_bytes(c.try_into().expect("dword chunk")))
            .collect();
        assert_eq!(values, vec![40, 30, 20, 10], "array reversed in place");
    }

    // The lesson 03 exercise template ships blank; it must still assemble,
    // link, and reach the spin at `done` before the reader fills it in.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_template() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/03_reverse_array/reverse_array_template.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm template");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(1_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "reverse_array_template.s should be spinning at done: {:?}",
            result.termination
        );
    }

    // Lesson 04 part 1: jal/ret and the a-register argument convention, with
    // a leaf function called twice.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_max() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/04_functions/max.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "max.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(10), 12, "a0 should hold max(max(7, 12), 3)");
    }

    // Lesson 04 part 2: a non-leaf function must save ra and s0 in its frame;
    // the nested calls must come back with the right result.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_max3() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/04_functions/max3.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "max3.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(10), 9, "a0 should hold max3(4, 9, 6)");
    }

    // Lesson 05: recursion exercises one stack frame per call; 5! must land in
    // a0 at the spin.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_factorial() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/05_stack_frames/factorial_stack.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "factorial_stack.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(10), 120, "a0 should hold 5!");
    }

    // Lesson 05 exercise solution: both operands have to survive a call, so
    // sos(3, 4) only lands on 25 if the frame slots are right.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_sum_of_squares() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/05_stack_frames/sum_of_squares.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm root");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(10_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "sum_of_squares.s should still be spinning at done: {:?}",
            result.termination
        );
        assert_eq!(vm.peek_reg(10), 25, "a0 should hold sos(3, 4)");
    }

    // The lesson 05 template ships with a stub sos that just returns; it must
    // assemble, link, and reach the spin before the reader fills it in.
    #[test]
    fn file_manifest_builds_and_runs_lesson_asm_sum_of_squares_template() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/lessons/05_stack_frames/sum_of_squares_template.build");
        let manifest = BuildManifest::from_file(path).expect("parse lesson build file");
        let artifacts = BuildExecutor::build(&manifest).expect("assemble + link asm template");
        let mut vm = virtual_machine::virtual_machine::VirtualMachine::new(&artifacts.linked);
        let result = vm.run(1_000);
        assert!(
            matches!(
                result.termination,
                virtual_machine::virtual_machine::RunOutcome::TimedOut
            ),
            "sum_of_squares_template.s should be spinning at done: {:?}",
            result.termination
        );
    }

    #[test]
    fn file_manifest_parses_kernel_build() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("crates/os-runtime/kernel/kernel.build");
        let manifest = BuildManifest::from_file(path).expect("parse kernel build file");
        assert_eq!(manifest.name, "my_kernel");
        assert_eq!(manifest.target, TargetMode::Kernel);
        assert_eq!(manifest.entry.as_deref(), Some("_kernel_start"));
        assert_eq!(manifest.abi_exports, vec!["kmain".to_owned()]);
        assert!(matches!(
            manifest.import_closure,
            ImportClosurePolicy::Enabled {
                mangle_symbols: true
            }
        ));
    }

    #[test]
    fn file_manifest_out_enables_artifacts_relative_to_manifest() {
        let root = std::env::temp_dir().join(format!(
            "full_stack_manifest_out_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp manifest dir");
        let build_path = root.join("main.build");
        let source_path = root.join("main.hll");
        std::fs::write(&source_path, "main: () -> i32 { return 0 }").expect("write source");
        std::fs::write(
            &build_path,
            "name = \"main\"\nroot = \"main.hll\"\nout=artifacts\n",
        )
        .expect("write manifest");

        let manifest = BuildManifest::from_file(&build_path).expect("parse manifest");
        assert!(manifest.write_artifacts);
        assert_eq!(
            manifest.artifact_root.as_deref(),
            Some(root.join("artifacts").as_path())
        );

        let _ = std::fs::remove_dir_all(root);
    }

    // The split `ld` tool imports `./ld_link.hll`; building through its `.build` manifest must
    // resolve that relative import from the tool's own directory and link cleanly.
    #[test]
    fn file_manifest_builds_ld_with_relative_import() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("programs/user/bin/ld/ld.build");
        let manifest = BuildManifest::from_file(path).expect("parse ld build file");
        BuildExecutor::build(&manifest).expect("ld builds with its relative import resolved");
    }
}
