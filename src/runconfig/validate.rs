use crate::runconfig::{BuildRef, RunConfig};
use crate::workspace::{EntryKind, WorkspaceFs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunConfigSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfigDiagnostic {
    pub severity: RunConfigSeverity,
    pub message: String,
}

pub fn validate_config(config: &RunConfig, fs: &dyn WorkspaceFs) -> Vec<RunConfigDiagnostic> {
    let mut diagnostics = Vec::new();
    if config.name.trim().is_empty() {
        diagnostics.push(error("run config name is required"));
    }
    match &config.build {
        BuildRef::Manifest(path) => match fs.metadata(path) {
            Ok(meta) if meta.kind != EntryKind::File => {
                diagnostics.push(error("build path must point to a .build file"));
            }
            Ok(_) if !path.as_str().ends_with(".build") => {
                diagnostics.push(error("build path must end in .build"));
            }
            Ok(_) => {}
            Err(_) => diagnostics.push(error("build manifest does not exist")),
        },
        BuildRef::SourceWithDefaults { root, .. } => {
            if fs.metadata(root).is_err() {
                diagnostics.push(error("source root does not exist"));
            }
        }
    }
    for mount in &config.fs.mounts {
        if !mount.guest.starts_with('/') {
            diagnostics.push(error("guest mount path must be absolute"));
        }
        if fs.metadata(&mount.host).is_err() {
            diagnostics.push(error("mount host path does not exist"));
        }
    }
    diagnostics
}

fn error(message: &str) -> RunConfigDiagnostic {
    RunConfigDiagnostic {
        severity: RunConfigSeverity::Error,
        message: message.to_owned(),
    }
}

/// True when another config in `configs` (any index but `skip`) shares this
/// non-empty trimmed name. Used to warn on duplicate run-config names.
pub fn name_conflicts(name: &str, configs: &[RunConfig], skip: usize) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }
    configs
        .iter()
        .enumerate()
        .any(|(i, other)| i != skip && other.name.trim() == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runconfig::RunConfig;
    use crate::workspace::{MemoryFs, VPath};

    #[test]
    fn validates_missing_manifest() {
        let fs = MemoryFs::new();
        let config = RunConfig::hosted("Missing", VPath::parse("missing.build").expect("path"));

        let diagnostics = validate_config(&config, &fs);
        assert_eq!(diagnostics[0].message, "build manifest does not exist");
    }

    #[test]
    fn accepts_existing_manifest() {
        let mut fs = MemoryFs::new();
        let path = VPath::parse("programs/example/demo/demo.build").expect("path");
        fs.write_text(&path, "name = \"demo\"").expect("write");
        let config = RunConfig::hosted("Demo", path);

        assert!(validate_config(&config, &fs).is_empty());
    }

    #[test]
    fn detects_duplicate_names_but_skips_self() {
        let path = VPath::parse("a.build").expect("path");
        let configs = [
            RunConfig::hosted("Demo", path.clone()),
            RunConfig::hosted("Demo", path.clone()),
            RunConfig::hosted("Other", path),
        ];
        assert!(name_conflicts("Demo", &configs, 0));
        assert!(!name_conflicts("Other", &configs, 2));
        assert!(!name_conflicts("  ", &configs, 3));
    }
}
