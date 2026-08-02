use full_stack::workspace::VPath;
#[cfg(target_arch = "wasm32")]
use serde::Deserialize;

#[derive(Debug)]
pub(crate) struct ReferenceDocument {
    pub path: VPath,
    pub bytes: Vec<u8>,
    pub binary: bool,
    pub line: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct StartupReference {
    pub document: Option<ReferenceDocument>,
    pub status: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct RepositoryManifest {
    version: u32,
    files: Vec<RepositoryEntry>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct RepositoryEntry {
    path: String,
    bytes: u64,
    kind: FileKind,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum FileKind {
    Text,
    Binary,
}

#[cfg(any(target_arch = "wasm32", test))]
fn exact_path(input: &str) -> Result<VPath, String> {
    let path = VPath::parse(input).map_err(|e| format!("invalid repository path: {e}"))?;
    if path.is_root() || path.as_str() != input {
        return Err("repository path must be an exact relative path".to_owned());
    }
    Ok(path)
}

pub(crate) fn is_rust_source(path: &VPath) -> bool {
    path.file_name()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(_stem, extension)| extension.eq_ignore_ascii_case("rs"))
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_line(input: Option<&str>) -> Result<Option<usize>, String> {
    let Some(input) = input else {
        return Ok(None);
    };
    let line = input
        .parse::<usize>()
        .map_err(|_error| format!("invalid line `{input}`; expected a positive integer"))?;
    if line == 0 {
        return Err("invalid line `0`; line numbers are one-based".to_owned());
    }
    Ok(Some(line))
}

#[cfg(any(target_arch = "wasm32", test))]
fn line_count(text: &str) -> usize {
    if text.is_empty() {
        1
    } else {
        text.lines().count().max(1)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn clamp_line(line: usize, text: &str) -> (usize, bool) {
    let last = line_count(text);
    (line.min(last), line > last)
}

fn percent_encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub(crate) fn raw_file_url(path: &VPath) -> String {
    let encoded = path
        .as_str()
        .split('/')
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("_source/{encoded}")
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn load() -> StartupReference {
    use gloo_net::http::Request;

    let Some(window) = web_sys::window() else {
        return StartupReference::default();
    };
    let search = window.location().search().unwrap_or_default();
    let params = match web_sys::UrlSearchParams::new_with_str(&search) {
        Ok(params) => params,
        Err(_) => {
            return StartupReference {
                status: Some("could not parse the browser query string".to_owned()),
                ..StartupReference::default()
            };
        }
    };
    let Some(requested) = params.get("file") else {
        return StartupReference::default();
    };
    let path = match exact_path(&requested) {
        Ok(path) => path,
        Err(status) => {
            return StartupReference {
                status: Some(status),
                ..StartupReference::default()
            };
        }
    };
    if is_rust_source(&path) {
        return StartupReference {
            status: Some(format!("Rust source `{path}` is available on GitHub")),
            ..StartupReference::default()
        };
    }
    let line_result = parse_line(params.get("line").as_deref());

    let manifest_response = match Request::get("_source/manifest.json").send().await {
        Ok(response) if response.ok() => response,
        Ok(response) => {
            return StartupReference {
                status: Some(format!(
                    "repository manifest request failed with status {}",
                    response.status()
                )),
                ..StartupReference::default()
            };
        }
        Err(error) => {
            return StartupReference {
                status: Some(format!("repository manifest request failed: {error}")),
                ..StartupReference::default()
            };
        }
    };
    let manifest_text = match manifest_response.text().await {
        Ok(text) => text,
        Err(error) => {
            return StartupReference {
                status: Some(format!("could not read repository manifest: {error}")),
                ..StartupReference::default()
            };
        }
    };
    let manifest: RepositoryManifest = match serde_json::from_str(&manifest_text) {
        Ok(manifest) => manifest,
        Err(error) => {
            return StartupReference {
                status: Some(format!("invalid repository manifest: {error}")),
                ..StartupReference::default()
            };
        }
    };
    if manifest.version != 1 {
        return StartupReference {
            status: Some(format!(
                "unsupported repository manifest version {}",
                manifest.version
            )),
            ..StartupReference::default()
        };
    }
    let Some(entry) = manifest
        .files
        .iter()
        .find(|entry| entry.path == path.as_str())
    else {
        return StartupReference {
            status: Some(format!("repository file `{path}` is not published")),
            ..StartupReference::default()
        };
    };

    let response = match Request::get(&raw_file_url(&path)).send().await {
        Ok(response) if response.ok() => response,
        Ok(response) => {
            return StartupReference {
                status: Some(format!(
                    "repository file `{path}` request failed with status {}",
                    response.status()
                )),
                ..StartupReference::default()
            };
        }
        Err(error) => {
            return StartupReference {
                status: Some(format!("repository file `{path}` request failed: {error}")),
                ..StartupReference::default()
            };
        }
    };
    let bytes = match response.binary().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return StartupReference {
                status: Some(format!("could not read repository file `{path}`: {error}")),
                ..StartupReference::default()
            };
        }
    };
    if bytes.len() as u64 != entry.bytes {
        return StartupReference {
            status: Some(format!(
                "repository file `{path}` has the wrong byte length"
            )),
            ..StartupReference::default()
        };
    }
    let mut status = line_result.as_ref().err().cloned();
    let mut line = line_result.ok().flatten();
    if entry.kind == FileKind::Text {
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                return StartupReference {
                    status: Some(format!("repository file `{path}` is not valid UTF-8")),
                    ..StartupReference::default()
                };
            }
        };
        if let Some(requested_line) = line {
            let (opened_line, clamped) = clamp_line(requested_line, text);
            line = Some(opened_line);
            if clamped {
                status = Some(format!(
                    "line {requested_line} is beyond EOF; opened line {opened_line}"
                ));
            }
        }
    } else {
        line = None;
    }

    StartupReference {
        document: Some(ReferenceDocument {
            path,
            bytes,
            binary: entry.kind == FileKind::Binary,
            line,
        }),
        status,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn replace_address_bar(file: Option<&VPath>, line: Option<usize>) {
    use eframe::wasm_bindgen::JsValue;

    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let search = location.search().unwrap_or_default();
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return;
    };
    match file {
        Some(path) => params.set("file", path.as_str()),
        None => params.delete("file"),
    }
    match line {
        Some(line) if file.is_some() => params.set("line", &line.to_string()),
        _ => params.delete("line"),
    }
    let query: String = params.to_string().into();
    let pathname = location.pathname().unwrap_or_default();
    let hash = location.hash().unwrap_or_default();
    let url = if query.is_empty() {
        format!("{pathname}{hash}")
    } else {
        format!("{pathname}?{query}{hash}")
    };
    if let Ok(history) = window.history() {
        let _result = history.replace_state_with_url(&JsValue::NULL, "", Some(&url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_paths_reject_normalization() {
        assert_eq!(
            exact_path("crates/virtual-machine/src/bus.rs")
                .expect("path")
                .as_str(),
            "crates/virtual-machine/src/bus.rs"
        );
        for path in [
            "/src/main.rs",
            "src/../main.rs",
            "src\\main.rs",
            " src/main.rs",
        ] {
            assert!(exact_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn lines_are_positive_and_one_based() {
        assert_eq!(parse_line(None).expect("line"), None);
        assert_eq!(parse_line(Some("42")).expect("line"), Some(42));
        assert!(parse_line(Some("0")).is_err());
        assert!(parse_line(Some("4.2")).is_err());
    }

    #[test]
    fn raw_urls_encode_segments_without_encoding_separators() {
        let path = VPath::parse("directory/file with spaces.rs").expect("path");
        assert_eq!(
            raw_file_url(&path),
            "_source/directory/file%20with%20spaces.rs"
        );
    }

    #[test]
    fn rust_sources_are_identified_case_insensitively() {
        assert!(is_rust_source(&VPath::parse("src/main.rs").expect("path")));
        assert!(is_rust_source(&VPath::parse("src/main.RS").expect("path")));
        assert!(!is_rust_source(
            &VPath::parse("crates/asm-to-binary/_RISCV_SPECIFICATIONS.md").expect("path")
        ));
    }

    #[test]
    fn line_count_handles_empty_and_unterminated_text() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("one\ntwo"), 2);
        assert_eq!(line_count("one\ntwo\n"), 2);
        assert_eq!(clamp_line(42, "one\ntwo\n"), (2, true));
        assert_eq!(clamp_line(1, "one\ntwo\n"), (1, false));
    }
}
