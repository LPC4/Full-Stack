use crate::build::{BuildError, BuildManifest, BuildSource, decode_build_manifest};
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
    let decoded = decode_build_manifest(&manifest_text)
        .map_err(|err| vec![diagnostic(format!("manifest parse error: {err}"))])?;
    let root_path = resolve_relative(path, &decoded.root)
        .map_err(|err| vec![diagnostic(format!("invalid root path: {err}"))])?;
    let root_source = read_text(fs, &root_path)?;
    let name = decoded.name.clone().unwrap_or_else(|| {
        root_path
            .file_name()
            .unwrap_or("program")
            .trim_end_matches(".hll")
            .trim_end_matches(".s")
            .to_owned()
    });
    let source_prelude = if let Some(prelude_ref) = &decoded.source_prelude {
        let prelude_path = resolve_relative(path, prelude_ref)
            .map_err(|err| vec![diagnostic(format!("invalid source_prelude path: {err}"))])?;
        Some(read_text(fs, &prelude_path)?)
    } else {
        None
    };
    let artifact_root = decoded
        .out
        .as_deref()
        .map(|out_ref| {
            resolve_relative(path, out_ref)
                .map(|path| path.as_str().into())
                .map_err(|err| vec![diagnostic(format!("invalid out path: {err}"))])
        })
        .transpose()?;
    let asm_root = crate::build::is_asm_path(root_path.as_str());
    Ok(decoded.into_manifest(
        name.clone(),
        BuildSource::inline_with_path(name, root_source, root_path.as_str()),
        asm_root,
        source_prelude,
        artifact_root,
    ))
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
    use crate::build::{BuildExecutor, BuildSource, ImportClosurePolicy, StdlibPolicy};
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
    fn asm_root_defaults_to_no_stdlib_and_no_closure() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("t/sum.build").expect("path"),
            "root = \"sum.s\"\ntarget = \"freestanding\"\nentry = \"_start\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("t/sum.s").expect("path"),
            ".globl _start\n_start:\n  li a0, 0\n",
        )
        .expect("write source");

        let manifest =
            load_build_manifest(&fs, &VPath::parse("t/sum.build").expect("path")).expect("parse");
        assert_eq!(manifest.name, "sum");
        assert_eq!(manifest.stdlib, StdlibPolicy::None);
        assert_eq!(manifest.import_closure, ImportClosurePolicy::Disabled);
        assert!(manifest.abi_exports.is_empty());
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

    #[test]
    fn native_and_workspace_manifests_share_decoded_semantics() {
        let text = "name = \"parity\"\nroot = \"main.hll\"\ntarget = \"hosted\"\n\
                    entry = \"main\"\nstdlib = \"none\"\nimport_closure = false\n\
                    mangle_symbols = false\nsource_prelude = \"types.hll\"\n\
                    abi_exports = [\"main\", \"helper\"]\nout = \"artifacts\"\n\
                    latency = \"realistic\"\nlatency_dram = 300\n";
        let source = "main: () -> i32 { return 0 }\n";
        let prelude = "struct Pair { left: i32, right: i32 }\n";

        let root =
            std::env::temp_dir().join(format!("full-stack-manifest-parity-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create fixture");
        std::fs::write(root.join("demo.build"), text).expect("write native manifest");
        std::fs::write(root.join("main.hll"), source).expect("write native source");
        std::fs::write(root.join("types.hll"), prelude).expect("write native prelude");
        let native = BuildManifest::from_file(root.join("demo.build")).expect("native manifest");

        let mut fs = MemoryFs::new();
        for (path, contents) in [
            ("project/demo.build", text),
            ("project/main.hll", source),
            ("project/types.hll", prelude),
        ] {
            fs.write_text(&VPath::parse(path).expect("path"), contents)
                .expect("write workspace fixture");
        }
        let workspace = load_build_manifest(
            &fs,
            &VPath::parse("project/demo.build").expect("manifest path"),
        )
        .expect("workspace manifest");

        assert_eq!(
            BuildExecutor::plan(&native),
            BuildExecutor::plan(&workspace)
        );
        assert_eq!(native.latency, workspace.latency);
        assert_eq!(native.source_prelude, workspace.source_prelude);
        assert_eq!(native.write_artifacts, workspace.write_artifacts);
        assert_eq!(
            native
                .artifact_root
                .as_deref()
                .and_then(std::path::Path::file_name),
            workspace
                .artifact_root
                .as_deref()
                .and_then(std::path::Path::file_name)
        );
        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}
