use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct VPath(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathError {
    EmptyComponent,
    EscapesRoot,
    AbsoluteHostPath,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyComponent => write!(f, "path contains an empty component"),
            Self::EscapesRoot => write!(f, "path escapes the workspace root"),
            Self::AbsoluteHostPath => write!(f, "host-absolute paths are not workspace paths"),
        }
    }
}

impl std::error::Error for PathError {}

impl VPath {
    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn parse(input: &str) -> Result<Self, PathError> {
        normalize(input)
    }

    pub fn join(&self, child: &str) -> Result<Self, PathError> {
        if child.starts_with('/') || child.starts_with('\\') {
            return Self::parse(child);
        }
        let joined = if self.0.is_empty() {
            child.to_owned()
        } else {
            format!("{}/{}", self.0, child)
        };
        Self::parse(&joined)
    }

    pub fn parent(&self) -> Option<Self> {
        if self.0.is_empty() {
            return None;
        }
        match self.0.rsplit_once('/') {
            Some((parent, _)) => Some(Self(parent.to_owned())),
            None => Some(Self::root()),
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.0.is_empty() {
            None
        } else {
            self.0.rsplit('/').next()
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for VPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "/")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

fn normalize(input: &str) -> Result<VPath, PathError> {
    let raw = input.trim();
    if raw.is_empty() || raw == "/" || raw == "\\" {
        return Ok(VPath::root());
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' {
        return Err(PathError::AbsoluteHostPath);
    }

    let normalized = raw.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" => {
                if parts.is_empty() {
                    continue;
                }
                return Err(PathError::EmptyComponent);
            }
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(PathError::EscapesRoot);
                }
            }
            name => parts.push(name),
        }
    }
    Ok(VPath(parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_slashes_and_dot_segments() {
        let path = VPath::parse("/programs/./user/shell.build").expect("valid path");
        assert_eq!(path.as_str(), "programs/user/shell.build");
        assert_eq!(path.file_name(), Some("shell.build"));
        assert_eq!(path.parent().expect("parent").as_str(), "programs/user");
    }

    #[test]
    fn rejects_root_escape_and_host_absolute_paths() {
        assert_eq!(VPath::parse("../x"), Err(PathError::EscapesRoot));
        assert_eq!(VPath::parse("C:/tmp/x"), Err(PathError::AbsoluteHostPath));
    }

    #[test]
    fn joins_relative_children() {
        let base = VPath::parse("src").expect("valid base");
        assert_eq!(
            base.join("main.hll").expect("join").as_str(),
            "src/main.hll"
        );
    }
}
