use crate::program_catalog::{ProgramCatalog, ProgramKind};
use crate::runconfig::{FsMount, PresentMode, RunConfig, RunConfigStore, RunKind};
use crate::workspace::{MemoryFs, VPath, WorkspaceFs};
use std::io;

/// Build the initial browser/demo workspace from bundled repo sources.
pub fn sample_workspace() -> io::Result<MemoryFs> {
    let mut fs = MemoryFs::new();
    populate_demo_workspace(&mut fs)?;
    Ok(fs)
}

/// Write the demo workspace tree into `fs`: kernel + modules, boot `.s`, stdlib,
/// user tools, examples, and a run.toml. Existing files are left alone.
pub fn populate_demo_workspace(fs: &mut dyn WorkspaceFs) -> io::Result<()> {
    write_missing(fs, "README.md", SAMPLE_README)?;
    write_missing(
        fs,
        ".fullstack/run.toml",
        &sample_run_configs(&ProgramCatalog::default()),
    )?;
    for (path, source) in os_runtime::SOURCE_TREE {
        write_missing(fs, &format!("crates/os-runtime/{path}"), source)?;
    }
    for (path, source) in crate::userspace::SOURCE_FILES {
        if path.starts_with("programs/user/") || path.starts_with("programs/example/") {
            write_missing(fs, path, source)?;
        }
    }
    Ok(())
}

/// The default run.toml: the kernel boot first, then one hosted config per
/// bundled example program, in catalog order.
fn sample_run_configs(catalog: &ProgramCatalog) -> String {
    let mut store = RunConfigStore::default();

    if let Ok(build) = VPath::parse("crates/os-runtime/kernel/kernel.build") {
        let mut kernel = RunConfig::hosted("Boot kernel + shell", build);
        kernel.kind = RunKind::KernelBoot;
        // Mount only the demo sources: the full user tree overflows the boot FS image.
        if let Ok(host) = VPath::parse("programs/user/demo") {
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

/// Write `text` at `path` unless the file already exists, creating parent dirs.
fn write_missing(fs: &mut dyn WorkspaceFs, path: &str, text: &str) -> io::Result<()> {
    let path = VPath::parse(path).map_err(invalid_path)?;
    if fs.metadata(&path).is_ok() {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.is_root()
    {
        fs.create_dir(&parent)?;
    }
    fs.write_file(&path, text.as_bytes())
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

fn invalid_path(err: crate::workspace::PathError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, err)
}

const SAMPLE_README: &str = r#"# Full-Stack Demo Workspace

Real copies of the bundled kernel (and its linked modules), boot assembly, standard
library, user tools, and example programs, with run configurations for the kernel and
every example in `.fullstack/run.toml`. Build artifacts land in `out/`. Natively this
folder lives at `programs/demo_workspace`; the browser opens the same tree in memory.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_workspace_contains_kernel_stdlib_boot_user_and_examples() {
        let fs = sample_workspace().expect("sample workspace");

        for path in [
            ".fullstack/run.toml",
            "crates/os-runtime/kernel/kernel.build",
            "crates/os-runtime/kernel/core/my_kernel.hll",
            "crates/os-runtime/kernel/mm/pmm.hll",
            "crates/os-runtime/kernel/trap/trap_entry.hll",
            "crates/os-runtime/boot/startup.s",
            "crates/os-runtime/boot/trap.s",
            "crates/os-runtime/stdlib/stdlib.build",
            "crates/os-runtime/stdlib/core/text.hll",
            "programs/user/bin/shell/shell.build",
            "programs/user/bin/shell/shell.hll",
            "programs/example/core_basics/core_basics.hll",
            "programs/example/core_basics/core_basics.build",
        ] {
            assert!(
                fs.read_file(&VPath::parse(path).expect("path")).is_ok(),
                "missing {path}"
            );
        }
    }

    #[test]
    fn populate_preserves_existing_files() {
        let mut fs = sample_workspace().expect("sample workspace");
        let path = VPath::parse("crates/os-runtime/kernel/mm/pmm.hll").expect("path");
        fs.write_text(&path, "; edited").expect("write");

        populate_demo_workspace(&mut fs).expect("repopulate");
        assert_eq!(fs.read_file(&path).expect("read"), b"; edited");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn populate_materializes_a_real_native_folder() {
        let root = std::env::temp_dir().join(format!(
            "full_stack_demo_workspace_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create root");
        let mut fs = crate::workspace::NativeFs::open(&root).expect("open native fs");

        populate_demo_workspace(&mut fs).expect("populate");
        for path in [
            ".fullstack/run.toml",
            "crates/os-runtime/kernel/mm/pmm.hll",
            "crates/os-runtime/boot/startup.s",
            "programs/example/core_basics/core_basics.build",
            "programs/user/bin/shell/shell.hll",
        ] {
            assert!(root.join(path).is_file(), "missing on disk: {path}");
        }
        // A relaunch repopulates without clobbering existing files.
        populate_demo_workspace(&mut fs).expect("repopulate");

        std::fs::remove_dir_all(root).expect("cleanup");
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
