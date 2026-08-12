use std::path::{Component, Path, PathBuf};

fn include_path(line: &str) -> Option<&str> {
    line.trim().strip_prefix("{{#include ")?.strip_suffix("}}")
}

fn resolve_include(repo: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("invalid Guide include path `{path}`"));
    }
    let resolved = repo
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("cannot resolve Guide include `{path}`: {error}"))?;
    if !resolved.starts_with(repo) || !resolved.is_file() {
        return Err(format!("Guide include `{path}` escaped the repository"));
    }
    Ok(resolved)
}

fn main() -> Result<(), String> {
    let crate_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("manifest dir")?);
    let repo = crate_dir
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository: {error}"))?;
    let guide = repo.join("guide/src/written.md");
    println!("cargo:rerun-if-changed={}", guide.display());
    let authored = std::fs::read_to_string(&guide)
        .map_err(|error| format!("cannot read {}: {error}", guide.display()))?;
    let mut expanded = String::with_capacity(authored.len());
    for line in authored.split_inclusive('\n') {
        if let Some(path) = include_path(line.trim_end_matches(['\r', '\n'])) {
            let source = resolve_include(&repo, path)?;
            println!("cargo:rerun-if-changed={}", source.display());
            let contents = std::fs::read_to_string(&source)
                .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
            expanded.push_str(&contents);
            if line.ends_with('\n') && !contents.ends_with('\n') {
                expanded.push('\n');
            }
        } else {
            expanded.push_str(line);
        }
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("output dir")?);
    std::fs::write(out.join("guide.md"), expanded)
        .map_err(|error| format!("cannot write expanded Guide: {error}"))
}
