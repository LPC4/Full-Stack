//! Shared `.build` manifest parser: line-oriented `key = value` with quoted strings,
//! `true`/`false` booleans, `["a", "b"]` lists, `#` comments. Dependency- and type-free.
#![expect(
    clippy::missing_errors_doc,
    reason = "parse failures are self-evident; house style keeps doc comments to 1-2 lines"
)]

use std::collections::HashMap;
use std::fmt;

/// A parsed manifest: keys mapped to their scalar form (strings already unquoted; lists and
/// booleans kept raw and decoded on demand through [`Manifest::list`] / [`Manifest::bool`]).
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    values: HashMap<String, String>,
}

/// A manifest parsing failure with a human-readable message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ManifestError {}

/// Parse manifest text into a [`Manifest`].
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    let mut values = HashMap::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(head, _)| head)
            .trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(ManifestError(format!(
                "line {}: expected `key = value`",
                index + 1
            )));
        };
        values.insert(key.trim().to_owned(), parse_scalar(value.trim())?);
    }
    Ok(Manifest { values })
}

impl Manifest {
    /// The stored scalar for `key` (unquoted for plain strings; raw for lists/booleans).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// An owned copy of a string-valued key.
    pub fn string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    /// Decode a boolean-valued key, if present.
    pub fn bool(&self, key: &str) -> Result<Option<bool>, ManifestError> {
        self.values
            .get(key)
            .map(|value| parse_bool(value))
            .transpose()
    }

    /// Decode a string-list-valued key, if present.
    pub fn list(&self, key: &str) -> Result<Option<Vec<String>>, ManifestError> {
        self.values
            .get(key)
            .map(|value| parse_string_list(value))
            .transpose()
    }
}

fn parse_scalar(value: &str) -> Result<String, ManifestError> {
    if value.starts_with('[') || matches!(value, "true" | "false") {
        return Ok(value.to_owned());
    }
    parse_quoted(value)
}

fn parse_quoted(value: &str) -> Result<String, ManifestError> {
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return Err(ManifestError(format!(
            "expected quoted string, got `{value}`"
        )));
    };
    Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn parse_string_list(value: &str) -> Result<Vec<String>, ManifestError> {
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Err(ManifestError(format!(
            "expected string list, got `{value}`"
        )));
    };
    let mut out = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        out.push(parse_quoted(item)?);
    }
    Ok(out)
}

fn parse_bool(value: &str) -> Result<bool, ManifestError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ManifestError(format!(
            "expected boolean `true` or `false`, got `{value}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_scalars_lists_and_bools() {
        let manifest = parse(
            r#"
name = "as"            # a tool
target = "hosted"
import_closure = false
modules = ["as_object.hll", "as_layout.hll"]
"#,
        )
        .expect("parse");
        assert_eq!(manifest.string("name").as_deref(), Some("as"));
        assert_eq!(manifest.get("target"), Some("hosted"));
        assert_eq!(manifest.bool("import_closure").expect("bool"), Some(false));
        assert_eq!(
            manifest.list("modules").expect("list"),
            Some(vec!["as_object.hll".to_owned(), "as_layout.hll".to_owned()])
        );
        assert_eq!(manifest.bool("missing").expect("absent"), None);
    }

    #[test]
    fn rejects_lines_without_equals() {
        assert!(parse("not a kv line").is_err());
    }

    #[test]
    fn unescapes_quotes_and_backslashes() {
        let manifest = parse(r#"desc = "a \"quoted\" path\\here""#).expect("parse");
        assert_eq!(
            manifest.string("desc").as_deref(),
            Some(r#"a "quoted" path\here"#)
        );
    }
}
