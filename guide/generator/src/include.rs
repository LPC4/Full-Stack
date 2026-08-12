use crate::repository::validate_path;
use std::path::Path;

const PREFIX: &str = "{{#include ";

pub fn expand(markdown: &str, repo_root: &Path) -> Result<String, String> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve repository root: {e}"))?;
    let mut expanded = String::with_capacity(markdown.len());

    for line in markdown.split_inclusive('\n') {
        let directive = line.trim_end_matches(['\r', '\n']).trim();
        if let Some(rest) = directive.strip_prefix(PREFIX) {
            let path = rest
                .strip_suffix("}}")
                .ok_or_else(|| format!("malformed include directive `{directive}`"))?;
            validate_path(path)?;
            let source = repo_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let resolved = source
                .canonicalize()
                .map_err(|e| format!("cannot resolve included file `{path}`: {e}"))?;
            if !resolved.starts_with(&repo_root) || !resolved.is_file() {
                return Err(format!("included path `{path}` is not a repository file"));
            }
            let contents = std::fs::read_to_string(&resolved)
                .map_err(|e| format!("cannot read included file `{path}`: {e}"))?;
            expanded.push_str(&contents);
            if line.ends_with('\n') && !contents.ends_with('\n') {
                expanded.push('\n');
            }
        } else if directive.starts_with("{{#include") {
            return Err(format!("malformed include directive `{directive}`"));
        } else {
            expanded.push_str(line);
        }
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("full-stack-guide-include-{stamp}"));
        std::fs::create_dir_all(root.join("programs")).expect("create fixture");
        std::fs::write(root.join("programs/example.s"), "addi a0, zero, 1\n")
            .expect("write fixture");
        root
    }

    #[test]
    fn includes_repository_file_bytes_inside_a_fence() {
        let root = fixture();
        let markdown = "```asm\n{{#include programs/example.s}}\n```\n";
        let expanded = expand(markdown, &root).expect("expand");
        assert_eq!(expanded, "```asm\naddi a0, zero, 1\n```\n");
        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn rejects_traversal_absolute_and_malformed_includes() {
        let root = fixture();
        for directive in [
            "{{#include ../secret}}\n",
            "{{#include /etc/passwd}}\n",
            "{{#include programs/example.s}\n",
        ] {
            assert!(expand(directive, &root).is_err(), "{directive}");
        }
        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}
