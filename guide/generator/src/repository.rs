use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MANIFEST_NAME: &str = "manifest.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Text,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub kind: FileKind,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub version: u32,
    pub files: Vec<Entry>,
}

#[derive(Debug)]
pub struct Catalog {
    repo_root: PathBuf,
    entries: Vec<Entry>,
    paths: HashSet<String>,
}

impl Catalog {
    #[cfg(test)]
    pub fn testing(paths: &[&str]) -> Self {
        let paths: HashSet<String> = paths.iter().map(|path| (*path).to_owned()).collect();
        Self {
            repo_root: PathBuf::new(),
            entries: Vec::new(),
            paths,
        }
    }

    pub fn discover(repo_root: &Path, include_worktree: bool) -> Result<Self, String> {
        let args: &[&str] = if include_worktree {
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ]
        } else {
            &["ls-files", "-z"]
        };
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .map_err(|e| format!("cannot run git ls-files: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let mut names = Vec::new();
        for raw in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|raw| !raw.is_empty())
        {
            let path = std::str::from_utf8(raw)
                .map_err(|_error| "repository contains a non-UTF-8 path".to_owned())?
                .replace('\\', "/");
            validate_path(&path)?;
            names.push(path);
        }
        names.sort();

        let mut entries = Vec::with_capacity(names.len());
        let mut paths = HashSet::with_capacity(names.len());
        for path in names {
            if !paths.insert(path.clone()) {
                return Err(format!("duplicate normalized repository path `{path}`"));
            }
            let source = repo_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let metadata = match std::fs::symlink_metadata(&source) {
                Ok(metadata) => metadata,
                Err(error) if include_worktree && error.kind() == std::io::ErrorKind::NotFound => {
                    paths.remove(&path);
                    continue;
                }
                Err(error) => return Err(format!("cannot inspect tracked file `{path}`: {error}")),
            };
            if !metadata.file_type().is_file() {
                return Err(format!("repository entry `{path}` is not a regular file"));
            }
            let bytes = std::fs::read(&source)
                .map_err(|e| format!("cannot read repository file `{path}`: {e}"))?;
            let known_binary =
                path.ends_with(".elf") || path.ends_with(".o") || bytes.starts_with(b"\x7fELF");
            let kind = if !known_binary && std::str::from_utf8(&bytes).is_ok() {
                FileKind::Text
            } else {
                FileKind::Binary
            };
            entries.push(Entry {
                path,
                bytes: metadata.len(),
                kind,
            });
        }

        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            entries,
            paths,
        })
    }

    pub fn contains(&self, path: &str) -> bool {
        self.paths.contains(path)
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn publish(&self, output: &Path) -> Result<usize, String> {
        std::fs::create_dir_all(output)
            .map_err(|e| format!("cannot create {}: {e}", output.display()))?;
        let entries = self
            .entries
            .iter()
            .filter(|entry| !is_rust_source(&entry.path))
            .cloned()
            .collect::<Vec<_>>();
        for entry in &entries {
            let source = self
                .repo_root
                .join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let target = output.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
            std::fs::copy(&source, &target)
                .map_err(|e| format!("cannot publish `{}`: {e}", entry.path))?;
        }
        let manifest = Manifest {
            version: 1,
            files: entries,
        };
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("cannot serialize repository manifest: {e}"))?;
        std::fs::write(output.join(MANIFEST_NAME), json)
            .map_err(|e| format!("cannot write repository manifest: {e}"))?;
        Ok(manifest.files.len())
    }
}

pub fn is_rust_source(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_stem, extension)| extension.eq_ignore_ascii_case("rs"))
}

pub fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || Path::new(path).is_absolute()
        || path.as_bytes().get(1) == Some(&b':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("invalid repository path `{path}`"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repository_paths_reject_absolute_and_traversing_values() {
        for path in [
            "",
            "/etc/passwd",
            "C:secret",
            "../secret",
            "a/../b",
            "a\\b",
            "a//b",
        ] {
            assert!(validate_path(path).is_err(), "{path}");
        }
        assert!(validate_path("crates/virtual-machine/src/bus.rs").is_ok());
        assert!(validate_path("directory/file with spaces.txt").is_ok());
    }

    #[test]
    fn catalog_contains_tracked_regular_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = Catalog::discover(&root, true).expect("catalog");
        assert!(catalog.contains("Cargo.toml"));
        assert!(catalog.entries().iter().all(|entry| entry.bytes > 0));
    }

    #[test]
    fn publishing_preserves_bytes_spaces_and_unowned_siblings() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("full-stack-guide-catalog-{stamp}"));
        let source = root.join("repo");
        let output = root.join("site/_source");
        std::fs::create_dir_all(source.join("directory")).expect("create source");
        std::fs::create_dir_all(root.join("site")).expect("create site");
        std::fs::write(source.join("directory/file with spaces.txt"), "hello").expect("write text");
        std::fs::write(source.join("image.bin"), [0, 0xFF, 3]).expect("write binary");
        std::fs::write(source.join("hidden.rs"), "fn hidden() {}\n").expect("write Rust source");
        std::fs::write(root.join("site/index.html"), "IDE").expect("write sibling");

        let entries = vec![
            Entry {
                path: "directory/file with spaces.txt".to_owned(),
                bytes: 5,
                kind: FileKind::Text,
            },
            Entry {
                path: "image.bin".to_owned(),
                bytes: 3,
                kind: FileKind::Binary,
            },
            Entry {
                path: "hidden.rs".to_owned(),
                bytes: 15,
                kind: FileKind::Text,
            },
        ];
        let catalog = Catalog {
            repo_root: source,
            paths: entries.iter().map(|entry| entry.path.clone()).collect(),
            entries,
        };
        assert_eq!(catalog.publish(&output).expect("publish"), 2);
        assert!(!output.join("hidden.rs").exists());
        assert_eq!(
            std::fs::read(output.join("image.bin")).expect("read binary"),
            [0, 0xFF, 3]
        );
        assert_eq!(
            std::fs::read_to_string(root.join("site/index.html")).expect("read sibling"),
            "IDE"
        );
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(output.join(MANIFEST_NAME)).expect("read manifest"),
        )
        .expect("manifest JSON");
        assert_eq!(manifest["version"], 1);
        assert_eq!(manifest["files"][1]["kind"], "binary");
        assert_eq!(
            manifest["files"].as_array().expect("manifest files").len(),
            2
        );
        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn publishing_fails_when_a_catalogued_file_disappears() {
        let catalog = Catalog {
            repo_root: PathBuf::from("does-not-exist"),
            entries: vec![Entry {
                path: "missing.md".to_owned(),
                bytes: 1,
                kind: FileKind::Text,
            }],
            paths: HashSet::from(["missing.md".to_owned()]),
        };
        let output = std::env::temp_dir().join(format!(
            "full-stack-guide-missing-output-{}",
            std::process::id()
        ));
        assert!(catalog.publish(&output).is_err());
        if output.exists() {
            std::fs::remove_dir_all(output).expect("remove fixture");
        }
    }
}
