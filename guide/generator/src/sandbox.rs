// Keeps the wasm bundle in step with its sources so one command is enough.
// A no-op wasm-pack run still costs ~8s, so it is gated on an mtime check.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::freshness;

pub enum Outcome {
    Fresh,
    Rebuilt,
    NoToolchain,
}

// The wasm crate and every local crate it pulls in.
fn sources(root: &Path) -> Vec<PathBuf> {
    let wasm = root.join("wasm");
    let mut paths = vec![wasm.join("src"), wasm.join("Cargo.toml")];
    if let Some(repo) = root.parent() {
        paths.push(repo.join("Cargo.lock"));
        for name in [
            "asm-to-binary",
            "fs-utils",
            "hll-to-ir",
            "ir-to-asm",
            "os-runtime",
            "virtual-machine",
        ] {
            paths.push(repo.join("crates").join(name));
        }
    }
    paths
}

fn is_stale(root: &Path) -> bool {
    let bundle = root.join("wasm/pkg/full_stack_guide_wasm_bg.wasm");
    let marker = root.join("../target/full-stack-guide-wasm.stamp");
    freshness::is_stale(&marker, &bundle, &sources(root))
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
            "full_stack_guide_wasm",
        ])
        .status();

    match status {
        Ok(status) if status.success() => {
            freshness::mark(&root.join("../target/full-stack-guide-wasm.stamp"))
                .map_err(|error| format!("cannot record sandbox build: {error}"))?;
            Ok(Outcome::Rebuilt)
        }
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
        let paths = sources(Path::new("guide"));
        assert!(paths.iter().any(|p| p.ends_with("wasm/src")));
        assert!(paths.iter().any(|p| p.ends_with("crates/fs-utils")));
        assert!(paths.iter().any(|p| p.ends_with("crates/hll-to-ir")));
        assert!(paths.iter().any(|p| p.ends_with("crates/ir-to-asm")));
    }
}
