use crate::program_catalog::{ProgramCatalog, ProgramFile, ProgramKind};
use crate::runconfig::{FsMount, PresentMode, RunConfig, RunConfigStore, RunKind};
use crate::workspace::{MemoryFs, VPath, WorkspaceFs as _};
use std::io;

const KERNEL_BUILD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/os-runtime/kernel/kernel.build"
));
const STDLIB_BUILD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/os-runtime/stdlib/stdlib.build"
));

/// Build the initial browser/demo workspace from bundled repo sources.
pub fn sample_workspace() -> io::Result<MemoryFs> {
    let mut fs = MemoryFs::new();
    let catalog = ProgramCatalog::default();
    write_text(&mut fs, "README.md", SAMPLE_README)?;
    write_text(
        &mut fs,
        ".fullstack/run.toml",
        &sample_run_configs(&catalog),
    )?;
    write_text(
        &mut fs,
        "crates/os-runtime/kernel/kernel.build",
        KERNEL_BUILD,
    )?;
    write_text(
        &mut fs,
        "crates/os-runtime/stdlib/stdlib.build",
        STDLIB_BUILD,
    )?;

    for program in catalog.all_programs() {
        match program.kind {
            ProgramKind::Custom => {}
            ProgramKind::Stdlib if program.parent_id.is_none() => {}
            ProgramKind::Os if program.parent_id.is_none() && program.source_path.is_none() => {}
            ProgramKind::Stdlib | ProgramKind::Os | ProgramKind::User | ProgramKind::Example => {
                if let Some(path) = workspace_key(program.source_path.as_deref()) {
                    write_text(&mut fs, &path, &program.source)?;
                }
                if let Some(path) = workspace_key(program.build_path.as_deref())
                    && !has_file(&fs, &path)?
                {
                    write_build_manifest(&mut fs, &path, program)?;
                }
            }
        }
    }

    Ok(fs)
}

/// The default run.toml: the kernel boot first, then one hosted config per
/// bundled example program, in catalog order.
fn sample_run_configs(catalog: &ProgramCatalog) -> String {
    let mut store = RunConfigStore::default();

    if let Ok(build) = VPath::parse("crates/os-runtime/kernel/kernel.build") {
        let mut kernel = RunConfig::hosted("Boot kernel + shell", build);
        kernel.kind = RunKind::KernelBoot;
        if let Ok(host) = VPath::parse("programs/user") {
            kernel.fs.mounts.push(FsMount {
                host,
                guest: "/home/demo".to_owned(),
            });
        }
        store.configs.push(kernel);
    }

    for program in catalog.all_programs() {
        if program.kind != ProgramKind::Example {
            continue;
        }
        let Some(build) = workspace_key(program.build_path.as_deref()) else {
            continue;
        };
        let Ok(build) = VPath::parse(&build) else {
            continue;
        };
        let mut config = RunConfig::hosted(program.name.clone(), build);
        config.run.present = PresentMode::Terminal;
        store.configs.push(config);
    }

    store.to_toml()
}

fn write_build_manifest(fs: &mut MemoryFs, path: &str, program: &ProgramFile) -> io::Result<()> {
    let root = manifest_relative_root(path, program.source_path.as_deref()).unwrap_or_else(|| {
        format!(
            "{}.hll",
            program
                .id
                .rsplit_once('-')
                .map(|(_, name)| name)
                .unwrap_or(program.id.as_str())
        )
    });
    let name = root.trim_end_matches(".hll");
    let target = if program.is_os() { "kernel" } else { "hosted" };
    let stdlib = if program.is_os() { "kernel" } else { "hosted" };
    let text = format!(
        "name = \"{name}\"\ntarget = \"{target}\"\nroot = \"{root}\"\nstdlib = \"{stdlib}\"\nimport_closure = true\nmangle_symbols = {}\n",
        if program.is_os() { "false" } else { "true" }
    );
    write_text(fs, path, &text)
}

fn write_text(fs: &mut MemoryFs, path: &str, text: &str) -> io::Result<()> {
    fs.write_text(&VPath::parse(path).map_err(invalid_path)?, text)
}

fn has_file(fs: &MemoryFs, path: &str) -> io::Result<bool> {
    match fs.read_file(&VPath::parse(path).map_err(invalid_path)?) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn workspace_key(path: Option<&str>) -> Option<String> {
    let path = path?;
    let normalized = path.replace('\\', "/");
    for marker in [
        "programs/example/",
        "programs/user/",
        "crates/os-runtime/kernel/",
        "crates/os-runtime/stdlib/",
    ] {
        if let Some(index) = normalized.find(marker) {
            return Some(normalized[index..].to_owned());
        }
    }
    None
}

fn manifest_relative_root(build_path: &str, source_path: Option<&str>) -> Option<String> {
    let source = workspace_key(source_path)?;
    let build_dir = build_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    source
        .strip_prefix(&format!("{build_dir}/"))
        .map(str::to_owned)
}

fn invalid_path(err: crate::workspace::PathError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, err)
}

const SAMPLE_README: &str = r#"# Full-Stack Sample Workspace

This workspace is generated from the bundled compiler, OS, stdlib, user tools, and examples.
It is intended for the browser/demo IDE path and mirrors the repo layout closely enough for
workspace-backed editing, manifest validation, and run configuration work.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_workspace_contains_kernel_stdlib_user_and_examples() {
        let fs = sample_workspace().expect("sample workspace");

        for path in [
            ".fullstack/run.toml",
            "crates/os-runtime/kernel/kernel.build",
            "crates/os-runtime/stdlib/stdlib.build",
            "programs/user/bin/shell/shell.build",
            "programs/example/core_basics/core_basics.hll",
        ] {
            assert!(fs.read_file(&VPath::parse(path).expect("path")).is_ok());
        }
    }

    #[test]
    fn sample_run_configs_cover_the_kernel_and_every_example() {
        let fs = sample_workspace().expect("sample workspace");
        let store = RunConfigStore::load(&fs).expect("run configs parse");

        assert_eq!(store.configs[0].kind, RunKind::KernelBoot, "kernel first");
        assert_eq!(store.configs[0].fs.mounts[0].guest, "/home/demo");

        let catalog = ProgramCatalog::default();
        let examples = catalog
            .all_programs()
            .iter()
            .filter(|p| p.kind == ProgramKind::Example)
            .count();
        assert_eq!(store.configs.len(), examples + 1);

        // Every example config points at a manifest that exists in the workspace.
        for config in &store.configs[1..] {
            assert_eq!(config.kind, RunKind::Hosted);
            let crate::runconfig::BuildRef::Manifest(path) = &config.build else {
                panic!("example config must reference a manifest");
            };
            assert!(
                fs.read_file(path).is_ok(),
                "missing manifest {} for config {}",
                path,
                config.name
            );
        }
    }
}
