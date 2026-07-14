use crate::build::{BuildError, BuildManifest, BuildSource, ImportClosurePolicy, StdlibPolicy};
use crate::compilation_pipeline::TargetMode;
use crate::workspace::{VPath, WorkspaceFs};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceManifestDiagnostic {
    pub message: String,
}

pub fn load_build_manifest(
    fs: &dyn WorkspaceFs,
    path: &VPath,
) -> Result<BuildManifest, Vec<WorkspaceManifestDiagnostic>> {
    let manifest_text = read_text(fs, path)?;
    let values = build_manifest::parse(&manifest_text)
        .map_err(|err| vec![diagnostic(format!("manifest parse error: {}", err.0))])?;

    let root_ref = values
        .get("root")
        .ok_or_else(|| vec![diagnostic("build manifest missing `root`")])?;
    let root_path = resolve_relative(path, root_ref)
        .map_err(|err| vec![diagnostic(format!("invalid root path: {err}"))])?;
    let root_source = read_text(fs, &root_path)?;
    let name = values.string("name").unwrap_or_else(|| {
        root_path
            .file_name()
            .unwrap_or("program")
            .trim_end_matches(".hll")
            .to_owned()
    });
    let target = values
        .get("target")
        .map(parse_target_mode)
        .transpose()
        .map_err(|err| vec![diagnostic(err)])?
        .unwrap_or(TargetMode::Hosted);

    let mut manifest = match target {
        TargetMode::Kernel => BuildManifest::kernel(name.clone(), ""),
        TargetMode::Hosted | TargetMode::Freestanding => BuildManifest::hosted(name.clone(), ""),
    };
    manifest.name = name.clone();
    manifest.target = target;
    manifest.root = BuildSource::inline_with_path(name, root_source, root_path.as_str());
    manifest.entry = values.string("entry");
    manifest.stdlib = values
        .get("stdlib")
        .map(|value| parse_stdlib_policy(value, target))
        .transpose()
        .map_err(|err| vec![diagnostic(err)])?
        .unwrap_or(StdlibPolicy::ForTarget);

    if let Some(enabled) = values
        .bool("import_closure")
        .map_err(|err| vec![diagnostic(err.0)])?
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
        .bool("mangle_symbols")
        .map_err(|err| vec![diagnostic(err.0)])?
        && matches!(manifest.import_closure, ImportClosurePolicy::Enabled { .. })
    {
        manifest.import_closure = ImportClosurePolicy::Enabled { mangle_symbols };
    }
    if let Some(prelude_ref) = values.get("source_prelude") {
        let prelude_path = resolve_relative(path, prelude_ref)
            .map_err(|err| vec![diagnostic(format!("invalid source_prelude path: {err}"))])?;
        manifest.source_prelude = Some(read_text(fs, &prelude_path)?);
    }
    if let Some(exports) = values
        .list("abi_exports")
        .map_err(|err| vec![diagnostic(err.0)])?
    {
        manifest.abi_exports = exports;
    }
    if let Some(out_ref) = values.string("out") {
        let out_path = resolve_relative(path, &out_ref)
            .map_err(|err| vec![diagnostic(format!("invalid out path: {err}"))])?;
        manifest.artifact_root = Some(out_path.as_str().into());
        manifest.write_artifacts = true;
    }
    manifest.latency =
        crate::build::parse_memory_latency_config(&values).map_err(|err| vec![diagnostic(err)])?;

    Ok(manifest)
}

fn read_text(
    fs: &dyn WorkspaceFs,
    path: &VPath,
) -> Result<String, Vec<WorkspaceManifestDiagnostic>> {
    let bytes = fs
        .read_file(path)
        .map_err(|err| vec![diagnostic(format!("{}: {err}", path.as_str()))])?;
    String::from_utf8(bytes)
        .map_err(|err| vec![diagnostic(format!("{} is not UTF-8: {err}", path.as_str()))])
}

fn resolve_relative(
    base_file: &VPath,
    relative: &str,
) -> Result<VPath, crate::workspace::PathError> {
    if relative.starts_with('/') {
        return VPath::parse(relative);
    }
    match base_file.parent() {
        Some(parent) => parent.join(relative),
        None => VPath::parse(relative),
    }
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

fn diagnostic(message: impl Into<String>) -> WorkspaceManifestDiagnostic {
    WorkspaceManifestDiagnostic {
        message: message.into(),
    }
}

impl From<Vec<WorkspaceManifestDiagnostic>> for BuildError {
    fn from(diagnostics: Vec<WorkspaceManifestDiagnostic>) -> Self {
        Self::Manifest(
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{BuildExecutor, BuildSource};
    use crate::workspace::MemoryFs;

    #[test]
    fn loads_manifest_and_inlines_workspace_root() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("programs/demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"hosted\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("programs/demo/demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");

        let manifest = load_build_manifest(
            &fs,
            &VPath::parse("programs/demo/demo.build").expect("path"),
        )
        .expect("manifest");
        assert!(manifest.latency.is_none());
        assert_eq!(BuildExecutor::plan(&manifest).root, "demo");
        match manifest.root {
            BuildSource::Inline { source_path, .. } => {
                assert_eq!(
                    source_path.expect("source path").display().to_string(),
                    "programs/demo/demo.hll"
                );
            }
            BuildSource::Path { .. } => panic!("workspace roots should be inlined"),
        }
    }

    #[test]
    fn reports_missing_root_source() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo.build").expect("path"),
            "name = \"demo\"\nroot = \"missing.hll\"\n",
        )
        .expect("write manifest");

        let diagnostics = match load_build_manifest(&fs, &VPath::parse("demo.build").expect("path"))
        {
            Ok(_) => panic!("missing root should fail"),
            Err(diagnostics) => diagnostics,
        };
        assert!(diagnostics[0].message.contains("missing.hll"));
    }

    #[test]
    fn latency_preset_and_overrides_parse() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo.build").expect("path"),
            "root = \"demo.hll\"\nlatency = \"realistic\"\nlatency_dram = 300\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");

        let manifest =
            load_build_manifest(&fs, &VPath::parse("demo.build").expect("path")).expect("manifest");
        let latency = manifest.latency.expect("latency enabled");
        assert!(latency.enabled);
        assert_eq!(latency.dram, 300);
        // Untouched levels keep the realistic preset.
        assert_eq!(
            latency.l1_hit,
            virtual_machine::bus::MemoryLatencyConfig::realistic().l1_hit
        );
    }

    #[test]
    fn latency_override_conflicts_with_off() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo.build").expect("path"),
            "root = \"demo.hll\"\nlatency = \"off\"\nlatency_dram = 300\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");

        let diagnostics = match load_build_manifest(&fs, &VPath::parse("demo.build").expect("path"))
        {
            Ok(_) => panic!("conflicting latency keys should fail"),
            Err(diagnostics) => diagnostics,
        };
        assert!(diagnostics[0].message.contains("latency_dram"));
    }

    #[test]
    fn out_enables_artifacts_relative_to_manifest() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("programs/demo/demo.build").expect("path"),
            "name = \"demo\"\nroot = \"demo.hll\"\nout=build\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("programs/demo/demo.hll").expect("path"),
            "main: () -> i32 { return 0 }",
        )
        .expect("write source");

        let manifest = load_build_manifest(
            &fs,
            &VPath::parse("programs/demo/demo.build").expect("path"),
        )
        .expect("manifest");
        assert!(manifest.write_artifacts);
        assert_eq!(
            manifest.artifact_root.expect("out").display().to_string(),
            "programs/demo/build"
        );
    }
}
