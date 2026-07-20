use crate::program_catalog::{ProgramCatalog, ProgramKind};
use crate::runconfig::{FsMount, PresentMode, RunConfig, RunConfigStore, RunKind};
use crate::workspace::{MemoryFs, VPath, WorkspaceFs};
use std::collections::HashMap;
use std::io;

const BUNDLE_STATE_PATH: &str = ".fullstack/bundle-state";

/// Build the initial browser/demo workspace from bundled repo sources.
pub fn sample_workspace() -> io::Result<MemoryFs> {
    let mut fs = MemoryFs::new();
    populate_demo_workspace(&mut fs)?;
    Ok(fs)
}

/// Build the in-memory lessons workspace: just the lessons and their run configs.
pub fn lessons_workspace() -> io::Result<MemoryFs> {
    let mut fs = MemoryFs::new();
    populate_lessons_workspace(&mut fs)?;
    Ok(fs)
}

/// Write the demo workspace tree into `fs`: canonical examples are refreshed,
/// edits to the other materialized sources are preserved.
pub fn populate_demo_workspace(fs: &mut dyn WorkspaceFs) -> io::Result<()> {
    write_missing(fs, "README.md", SAMPLE_README)?;
    write_missing(
        fs,
        ".fullstack/run.toml",
        &sample_run_configs(&ProgramCatalog::default()),
    )?;
    let previous = read_bundle_state(fs);
    let mut current = Vec::new();
    for (path, source) in os_runtime::SOURCE_TREE {
        let path = format!("crates/os-runtime/{path}");
        sync_bundled_file(fs, &path, source, previous.get(&path).copied(), false)?;
        current.push((path, content_hash(source.as_bytes())));
    }
    for (path, source) in crate::userspace::SOURCE_FILES {
        if path.starts_with("programs/user/") || path.starts_with("programs/example/") {
            let refresh = path.starts_with("programs/example/");
            sync_bundled_file(fs, path, source, previous.get(*path).copied(), refresh)?;
            current.push(((*path).to_owned(), content_hash(source.as_bytes())));
        }
    }
    write_bundle_state(fs, &current)?;
    Ok(())
}

/// Write the bundled lessons into `fs` with the lesson folders at the
/// workspace root, so each lesson shows only its source and `.build` file.
pub fn populate_lessons_workspace(fs: &mut dyn WorkspaceFs) -> io::Result<()> {
    write_missing(fs, "README.md", LESSONS_README)?;
    write_missing(fs, ".fullstack/run.toml", &lesson_run_configs())?;
    let previous = read_bundle_state(fs);
    let mut current = Vec::new();
    for (path, source) in crate::userspace::SOURCE_FILES {
        let Some(local) = path.strip_prefix("programs/lessons/") else {
            continue;
        };
        sync_bundled_file(fs, local, source, previous.get(local).copied(), false)?;
        current.push((local.to_owned(), content_hash(source.as_bytes())));
    }
    write_bundle_state(fs, &current)?;
    Ok(())
}

/// The lessons run.toml: one freestanding config per lesson, in lesson order.
fn lesson_run_configs() -> String {
    let mut store = RunConfigStore::default();
    // Titles mirror the `title` field of each lesson's .build manifest.
    let lessons = [
        (
            "01 sum: add 1..10 in assembly",
            "01_sum/sum.build",
            "loop",
            false,
        ),
        (
            "02 array sum: sum array from memory",
            "02_array_sum/array_sum.build",
            "",
            false,
        ),
        (
            "03 reverse array (template): your turn",
            "03_reverse_array/reverse_array_template.build",
            "",
            false,
        ),
        (
            "03 reverse array (solution)",
            "03_reverse_array/reverse_array.build",
            "",
            false,
        ),
        (
            "04 max: call a leaf function",
            "04_functions/max.build",
            "max",
            false,
        ),
        (
            "04 max3: stack frames",
            "04_functions/max3.build",
            "",
            false,
        ),
        (
            "05 factorial (stack frames): factorial with fp",
            "05_stack_frames/factorial_stack.build",
            "fact",
            true,
        ),
        (
            "05 sum of squares (template): your turn",
            "05_stack_frames/sum_of_squares_template.build",
            "",
            false,
        ),
        (
            "05 sum of squares (solution)",
            "05_stack_frames/sum_of_squares.build",
            "sos",
            true,
        ),
        (
            "06 hello (raw): write to UART",
            "06_uart/hello_raw.build",
            "",
            false,
        ),
        (
            "06 print_char: wrap the UART write",
            "06_uart/print_char.build",
            "",
            true,
        ),
        (
            "06 print_string: hello world",
            "06_uart/print_string.build",
            "",
            true,
        ),
    ];
    for (title, build, breakpoint, debug) in lessons {
        let Ok(build) = VPath::parse(build) else {
            continue;
        };
        let mut config = RunConfig::hosted(title, build);
        config.kind = RunKind::Freestanding;
        config.run.present = PresentMode::Terminal;
        if !breakpoint.is_empty() {
            config.debug.breakpoints.push(breakpoint.to_owned());
        }
        config.debug.enabled = debug;
        store.configs.push(config);
    }
    store.to_toml()
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

/// Refresh an untouched materialized source while preserving edits made in the workspace.
fn sync_bundled_file(
    fs: &mut dyn WorkspaceFs,
    path: &str,
    source: &str,
    previous_hash: Option<u64>,
    always_refresh: bool,
) -> io::Result<()> {
    let path = VPath::parse(path).map_err(invalid_path)?;
    let existing = fs.read_file(&path).ok();
    let may_replace = always_refresh
        || match (&existing, previous_hash) {
            (None, _) => true,
            (Some(bytes), Some(hash)) => content_hash(bytes) == hash,
            (Some(_), None) => false,
        };
    if !may_replace || existing.as_deref() == Some(source.as_bytes()) {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.is_root()
    {
        fs.create_dir(&parent)?;
    }
    fs.write_file(&path, source.as_bytes())
}

fn read_bundle_state(fs: &dyn WorkspaceFs) -> HashMap<String, u64> {
    let Ok(path) = VPath::parse(BUNDLE_STATE_PATH) else {
        return HashMap::new();
    };
    let Ok(bytes) = fs.read_file(&path) else {
        return HashMap::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let (hash, path) = line.split_once(' ')?;
            Some((path.to_owned(), u64::from_str_radix(hash, 16).ok()?))
        })
        .collect()
}

fn write_bundle_state(fs: &mut dyn WorkspaceFs, entries: &[(String, u64)]) -> io::Result<()> {
    let mut state = String::new();
    let mut entries = entries.to_vec();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, hash) in entries {
        state.push_str(&format!("{hash:016x} {path}\n"));
    }
    let path = VPath::parse(BUNDLE_STATE_PATH).map_err(invalid_path)?;
    if let Some(parent) = path.parent() {
        fs.create_dir(&parent)?;
    }
    fs.write_file(&path, state.as_bytes())
}

// Stable FNV-1a is sufficient for detecting local edits and avoids platform-specific hashes.
fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
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

const LESSONS_README: &str = r#"# Lessons

Small standalone programs for the written guide, one folder per lesson. Each
lesson holds its source and its `.build` manifest, with a matching run
configuration in `.fullstack/run.toml`. Natively this folder lives at
`programs/lessons`; the browser opens the same tree in memory.
"#;

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

        let showcase = String::from_utf8(
            fs.read_file(
                &VPath::parse(
                    "programs/example/hll2_completion_showcase/hll2_completion_showcase.hll",
                )
                .expect("showcase path"),
            )
            .expect("showcase source"),
        )
        .expect("utf-8 showcase");
        assert!(showcase.contains("dyn Measurable"));
    }

    #[test]
    fn lessons_workspace_holds_lessons_and_a_ready_run_config() {
        let fs = lessons_workspace().expect("lessons workspace");

        for path in [
            "README.md",
            ".fullstack/run.toml",
            "01_sum/sum.s",
            "01_sum/sum.build",
            "02_array_sum/array_sum.s",
            "02_array_sum/array_sum.build",
            "03_reverse_array/reverse_array.s",
            "03_reverse_array/reverse_array.build",
            "03_reverse_array/reverse_array_template.s",
            "03_reverse_array/reverse_array_template.build",
            "04_functions/max.s",
            "04_functions/max.build",
            "04_functions/max3.s",
            "04_functions/max3.build",
            "05_stack_frames/factorial_stack.s",
            "05_stack_frames/factorial_stack.build",
            "05_stack_frames/sum_of_squares.s",
            "05_stack_frames/sum_of_squares.build",
            "05_stack_frames/sum_of_squares_template.s",
            "05_stack_frames/sum_of_squares_template.build",
        ] {
            assert!(
                fs.read_file(&VPath::parse(path).expect("path")).is_ok(),
                "missing {path}"
            );
        }
        // Nothing from the demo tree leaks in; lessons stay easy to see.
        assert!(
            fs.read_file(&VPath::parse("crates/os-runtime/kernel/kernel.build").expect("path"))
                .is_err()
        );

        let store = RunConfigStore::load(&fs).expect("run configs parse");
        assert_eq!(store.configs.len(), 12);
        for config in &store.configs {
            assert_eq!(config.kind, RunKind::Freestanding);
            let crate::runconfig::BuildRef::Manifest(path) = &config.build else {
                panic!("lesson config must reference a manifest");
            };
            assert!(fs.read_file(path).is_ok(), "missing manifest {path}");
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

    #[test]
    fn bundled_sync_refreshes_only_untouched_materialized_files() {
        let mut fs = MemoryFs::new();
        let path = "programs/example/sample/sample.hll";

        sync_bundled_file(&mut fs, path, "old", None, false).expect("initial materialization");
        sync_bundled_file(&mut fs, path, "new", Some(content_hash(b"old")), false)
            .expect("refresh untouched file");
        let vpath = VPath::parse(path).expect("path");
        assert_eq!(fs.read_file(&vpath).expect("read"), b"new");

        fs.write_text(&vpath, "; local edit").expect("edit");
        sync_bundled_file(&mut fs, path, "newer", Some(content_hash(b"new")), false)
            .expect("preserve edited file");
        assert_eq!(fs.read_file(&vpath).expect("read"), b"; local edit");
    }

    #[test]
    fn bundled_examples_refresh_even_after_workspace_edits() {
        let mut fs = MemoryFs::new();
        let path = "programs/example/sample/sample.hll";
        let vpath = VPath::parse(path).expect("path");

        sync_bundled_file(&mut fs, path, "old", None, true).expect("materialize");
        fs.write_text(&vpath, "; stale edit").expect("edit");
        sync_bundled_file(&mut fs, path, "new", Some(content_hash(b"old")), true)
            .expect("refresh canonical example");

        assert_eq!(fs.read_file(&vpath).expect("read"), b"new");
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
