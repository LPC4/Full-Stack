// Keeps the wasm bundle in step with its sources so one command is enough.
// A no-op wasm-pack run still costs ~8s, so it is gated on an mtime check.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

pub enum Outcome {
    Fresh,
    Rebuilt,
    NoToolchain,
}

fn newest(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_dir() {
        return meta.modified().ok();
    }
    std::fs::read_dir(path)
        .ok()?
        .filter_map(|entry| newest(&entry.ok()?.path()))
        .max()
}

// The wasm crate and every local crate it pulls in.
fn sources(root: &Path) -> Vec<PathBuf> {
    let wasm = root.join("wasm");
    let mut paths = vec![wasm.join("src"), wasm.join("Cargo.toml")];
    if let Some(repo) = root.parent() {
        paths.push(repo.join("crates/fs-utils/src/syntax_highlight"));
    }
    paths
}

fn is_stale(root: &Path) -> bool {
    let bundle = root.join("wasm/pkg/fsbook_wasm_bg.wasm");
    let Some(built) = newest(&bundle) else {
        return true;
    };
    sources(root)
        .iter()
        .filter_map(|path| newest(path))
        .any(|source| source > built)
}

pub fn ensure(root: &Path, force: bool) -> Result<Outcome, String> {
    if !force && !is_stale(root) {
        return Ok(Outcome::Fresh);
    }
    let wasm = root.join("wasm");
    if !wasm.join("Cargo.toml").is_file() {
        return Ok(Outcome::Fresh);
    }

    println!("building sandbox with wasm-pack...");
    let status = Command::new("wasm-pack")
        .args(["build"])
        .arg(&wasm)
        .args([
            "--target",
            "web",
            "--release",
            "--out-dir",
            "pkg",
            "--out-name",
            "fsbook_wasm",
        ])
        .status();

    match status {
        Ok(status) if status.success() => Ok(Outcome::Rebuilt),
        Ok(status) => Err(format!("wasm-pack failed ({status})")),
        // Missing toolchain is not fatal: the pages still render without it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Outcome::NoToolchain),
        Err(error) => Err(format!("cannot run wasm-pack: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_bundle_is_stale() {
        assert!(is_stale(Path::new("does-not-exist")));
    }

    #[test]
    fn sources_cover_the_shared_highlighter() {
        let paths = sources(Path::new("book"));
        assert!(paths.iter().any(|p| p.ends_with("wasm/src")));
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("crates/fs-utils/src/syntax_highlight"))
        );
    }
}
