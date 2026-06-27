use crate::compilation_pipeline::{CompilationError, CompilationPipeline, TargetMode};
use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::{AssembledOutput, LinkerError};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub type BuildModuleResolver = Arc<dyn Fn(&str) -> Option<String> + 'static>;

/// Build description for one linkable HLL program.
///
/// This is intentionally data-only: callers describe what they want built, and
/// `BuildExecutor` owns the object ordering and pipeline setup.
#[derive(Clone)]
pub struct BuildManifest {
    pub name: String,
    pub target: TargetMode,
    pub entry: Option<String>,
    pub root: BuildSource,
    pub stdlib: StdlibPolicy,
    pub import_closure: ImportClosurePolicy,
    pub legacy_aux: Vec<BuildSource>,
    pub source_prelude: Option<String>,
    pub string_prefix: Option<String>,
    pub link_layout: Option<LinkLayout>,
    pub module_resolver: Option<BuildModuleResolver>,
    pub extra_objects: Vec<BuildObject>,
    pub abi_exports: Vec<String>,
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
            legacy_aux: Vec::new(),
            source_prelude: None,
            string_prefix: Some("_u_".to_owned()),
            link_layout: None,
            module_resolver: None,
            extra_objects: Vec::new(),
            abi_exports: vec!["main".to_owned()],
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
            legacy_aux: Vec::new(),
            source_prelude: None,
            string_prefix: Some("__kern_str_".to_owned()),
            link_layout: None,
            module_resolver: None,
            extra_objects: Vec::new(),
            abi_exports: Vec::new(),
            run_semantic_analysis: true,
            write_artifacts: false,
        }
    }

    pub fn from_user_program(program: &os_runtime::user::UserProgram) -> Self {
        let mut manifest = Self::hosted(program.name, program.source);
        manifest.source_prelude = (!program.layout.is_empty()).then(|| program.layout.to_owned());
        manifest.legacy_aux = program
            .aux_modules()
            .map(|(name, source)| BuildSource::inline(name, source))
            .collect();
        if !manifest.legacy_aux.is_empty() {
            manifest.import_closure = ImportClosurePolicy::Disabled;
        }
        manifest
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self, BuildError> {
        let path = path.into();
        let text = fs::read_to_string(&path).map_err(BuildError::Io)?;
        let base_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let values = parse_manifest_kv(&text)?;

        let root_path = values
            .get("root")
            .ok_or_else(|| BuildError::Manifest("build manifest missing `root`".to_owned()))?;
        let root_path = resolve_manifest_path(base_dir, root_path);
        let name = values.get("name").cloned().unwrap_or_else(|| {
            root_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("program")
                .to_owned()
        });
        let target = values
            .get("target")
            .map(|value| parse_target_mode(value))
            .transpose()?
            .unwrap_or(TargetMode::Hosted);

        let mut manifest = match target {
            TargetMode::Kernel => Self::kernel(name.clone(), ""),
            TargetMode::Hosted | TargetMode::Freestanding => Self::hosted(name.clone(), ""),
        };
        manifest.target = target;
        manifest.name = name.clone();
        manifest.root = BuildSource::path(name, root_path);
        manifest.entry = values.get("entry").cloned();
        manifest.stdlib = values
            .get("stdlib")
            .map(|value| parse_stdlib_policy(value, target))
            .transpose()?
            .unwrap_or(StdlibPolicy::ForTarget);

        if let Some(enabled) = values
            .get("import_closure")
            .map(|value| parse_bool(value))
            .transpose()?
        {
            manifest.import_closure = if enabled {
                ImportClosurePolicy::Enabled {
                    mangle_symbols: !matches!(target, TargetMode::Kernel),
                }
            } else {
                ImportClosurePolicy::Disabled
            };
        }
        if let Some(mangle_symbols) = values
            .get("mangle_symbols")
            .map(|value| parse_bool(value))
            .transpose()?
            && matches!(manifest.import_closure, ImportClosurePolicy::Enabled { .. })
        {
            manifest.import_closure = ImportClosurePolicy::Enabled { mangle_symbols };
        }

        if let Some(path) = values.get("source_prelude") {
            let path = resolve_manifest_path(base_dir, path);
            manifest.source_prelude = Some(fs::read_to_string(path).map_err(BuildError::Io)?);
        }
        if let Some(aux) = values.get("legacy_aux") {
            manifest.legacy_aux = parse_string_list(aux)?
                .into_iter()
                .map(|path| {
                    let path = resolve_manifest_path(base_dir, &path);
                    let name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("aux")
                        .to_owned();
                    BuildSource::path(name, path)
                })
                .collect();
            if !manifest.legacy_aux.is_empty() {
                manifest.import_closure = ImportClosurePolicy::Disabled;
            }
        }
        if let Some(exports) = values.get("abi_exports") {
            manifest.abi_exports = parse_string_list(exports)?;
        }

        Ok(manifest)
    }
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
    pub legacy_aux: Vec<String>,
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
            legacy_aux: manifest
                .legacy_aux
                .iter()
                .map(|source| source.name().to_owned())
                .collect(),
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

        for (name, assembled) in Self::compile_stdlib(manifest)? {
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

        match manifest.import_closure {
            ImportClosurePolicy::Enabled { mangle_symbols } => {
                pipeline.set_module_mangling(mangle_symbols);
                for (name, mut assembled) in pipeline
                    .compile_program_closure(&root.name, &root.source)
                    .map_err(BuildError::Compile)?
                {
                    if name == root.name {
                        Self::mark_abi_exports(manifest, &mut assembled);
                        if !manifest.legacy_aux.is_empty() {
                            Self::mark_all_defined_symbols_global(&mut assembled);
                        }
                    }
                    units.push(BuiltUnit { name, assembled });
                }
            }
            ImportClosurePolicy::Disabled => {
                let mut root_unit = Self::compile_one(&mut pipeline, &root)?;
                Self::mark_abi_exports(manifest, &mut root_unit.assembled);
                if !manifest.legacy_aux.is_empty() {
                    Self::mark_all_defined_symbols_global(&mut root_unit.assembled);
                }
                units.push(root_unit);
            }
        }

        for (index, aux) in manifest.legacy_aux.iter().enumerate() {
            let loaded = aux.load()?;
            let mut aux_pipeline = Self::pipeline_for(manifest);
            aux_pipeline.set_current_source_path(loaded.source_path.clone());
            aux_pipeline.set_string_prefix(Some(format!("{}_aux{index}_str_", manifest.name)));
            let mut aux_unit = Self::compile_one(&mut aux_pipeline, &loaded)?;
            Self::mark_all_defined_symbols_global(&mut aux_unit.assembled);
            units.push(aux_unit);
        }

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

    fn compile_one(
        pipeline: &mut CompilationPipeline,
        source: &LoadedSource,
    ) -> Result<BuiltUnit, BuildError> {
        pipeline.set_artifact_stem(Some(source.name.clone()));
        let result = pipeline
            .compile(&source.source)
            .map_err(BuildError::Compile)?;
        let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
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

    fn mark_all_defined_symbols_global(assembled: &mut AssembledOutput) {
        let symbols: Vec<String> = assembled
            .symbols_iter()
            .filter(|(name, _addr)| is_legacy_export_symbol(name))
            .map(|(name, _addr)| name.to_owned())
            .collect();
        for symbol in symbols {
            assembled.mark_entry_global(&symbol);
        }
    }

    fn pipeline_for(manifest: &BuildManifest) -> CompilationPipeline {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(manifest.target);
        pipeline.set_entry_point(manifest.entry.clone());
        pipeline.set_link_layout(manifest.link_layout.clone());
        pipeline.set_run_semantic_analysis(manifest.run_semantic_analysis);
        pipeline.set_write_artifacts(manifest.write_artifacts);
        pipeline.set_type_prelude(hll_to_ir::stdlib::get_stdlib_type_prelude());
        if let Some(prefix) = &manifest.string_prefix {
            pipeline.set_string_prefix(Some(prefix.clone()));
        }
        if let Some(prelude) = &manifest.source_prelude {
            pipeline.set_source_prelude(prelude.clone());
        }
        if let Some(resolver) = &manifest.module_resolver {
            let resolver = Arc::clone(resolver);
            pipeline.set_module_resolver(Some(Box::new(move |name| resolver(name))));
        }
        pipeline
    }
}

fn is_legacy_export_symbol(name: &str) -> bool {
    !name.starts_with('.') && !name.contains("__")
}

fn parse_manifest_kv(text: &str) -> Result<std::collections::HashMap<String, String>, BuildError> {
    let mut values = std::collections::HashMap::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(head, _)| head)
            .trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(BuildError::Manifest(format!(
                "line {}: expected `key = value`",
                index + 1
            )));
        };
        values.insert(key.trim().to_owned(), parse_scalar(value.trim())?);
    }
    Ok(values)
}

fn parse_scalar(value: &str) -> Result<String, BuildError> {
    if value.starts_with('[') || matches!(value, "true" | "false") {
        return Ok(value.to_owned());
    }
    parse_quoted(value)
}

fn parse_quoted(value: &str) -> Result<String, BuildError> {
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return Err(BuildError::Manifest(format!(
            "expected quoted string, got `{value}`"
        )));
    };
    Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn parse_string_list(value: &str) -> Result<Vec<String>, BuildError> {
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Err(BuildError::Manifest(format!(
            "expected string list, got `{value}`"
        )));
    };
    let mut out = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        out.push(parse_quoted(item)?);
    }
    Ok(out)
}

fn parse_bool(value: &str) -> Result<bool, BuildError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(BuildError::Manifest(format!(
            "expected boolean `true` or `false`, got `{value}`"
        ))),
    }
}

fn parse_target_mode(value: &str) -> Result<TargetMode, BuildError> {
    match value {
        "hosted" => Ok(TargetMode::Hosted),
        "freestanding" => Ok(TargetMode::Freestanding),
        "kernel" => Ok(TargetMode::Kernel),
        _ => Err(BuildError::Manifest(format!("unknown target `{value}`"))),
    }
}

fn parse_stdlib_policy(value: &str, target: TargetMode) -> Result<StdlibPolicy, BuildError> {
    match value {
        "none" => Ok(StdlibPolicy::None),
        "target" => Ok(StdlibPolicy::Explicit(target)),
        "hosted" => Ok(StdlibPolicy::Explicit(TargetMode::Hosted)),
        "freestanding" => Ok(StdlibPolicy::Explicit(TargetMode::Freestanding)),
        "kernel" => Ok(StdlibPolicy::Explicit(TargetMode::Kernel)),
        _ => Err(BuildError::Manifest(format!(
            "unknown stdlib policy `{value}`"
        ))),
    }
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

#[cfg(test)]
mod tests {
    use super::{BuildExecutor, BuildManifest, BuildSource, ImportClosurePolicy, StdlibPolicy};
    use crate::compilation_pipeline::TargetMode;

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
    fn manifest_builds_legacy_aux_program() {
        let mut manifest = BuildManifest::hosted(
            "manifest_aux",
            r#"
external helper: () -> i32

main: () -> i32 {
    return helper()
}
"#,
        );
        manifest.import_closure = ImportClosurePolicy::Disabled;
        manifest.stdlib = StdlibPolicy::Explicit(TargetMode::Hosted);
        manifest.legacy_aux.push(BuildSource::inline(
            "helper_mod",
            r#"
export helper: () -> i32 {
    return 7
}
"#,
        ));

        let artifacts = BuildExecutor::build(&manifest).expect("manifest aux build");
        assert!(
            artifacts.linked.has_symbol("helper"),
            "linked output should contain aux export"
        );
    }

    #[test]
    fn file_manifest_builds_hosted_example() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("programs/example/core_basics.build");
        let manifest = BuildManifest::from_file(path).expect("parse hosted build file");
        assert_eq!(manifest.name, "core_basics");
        assert_eq!(manifest.target, TargetMode::Hosted);

        let artifacts = BuildExecutor::build(&manifest).expect("file manifest build");
        assert!(artifacts.linked.has_symbol("main"));
    }

    #[test]
    fn all_example_file_manifests_build() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("programs/example");
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .expect("read examples directory")
            .map(|entry| entry.expect("example directory entry").path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("build"))
            .collect();
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
                mangle_symbols: false
            }
        ));
    }
}
