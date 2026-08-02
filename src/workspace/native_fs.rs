use crate::workspace::{DirEntry, EntryKind, Meta, VPath, WorkspaceFs};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct NativeFs {
    root: PathBuf,
    read_only: bool,
}

impl NativeFs {
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into().canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace root is not a directory",
            ));
        }
        Ok(Self {
            root,
            read_only: false,
        })
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn host_path(&self, path: &VPath) -> PathBuf {
        let mut out = self.root.clone();
        for part in path.as_str().split('/').filter(|part| !part.is_empty()) {
            out.push(part);
        }
        out
    }

    fn existing_host_path(&self, path: &VPath) -> io::Result<PathBuf> {
        let candidate = self.host_path(path);
        let canonical = candidate.canonicalize()?;
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path escapes workspace root",
            ))
        }
    }

    fn writable_host_path(&self, path: &VPath) -> io::Result<PathBuf> {
        self.ensure_writable()?;
        let candidate = self.host_path(path);
        let parent = candidate.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
        })?;
        let parent = parent.canonicalize()?;
        if parent.starts_with(&self.root) {
            Ok(candidate)
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path escapes workspace root",
            ))
        }
    }

    fn creatable_host_path(&self, path: &VPath) -> io::Result<PathBuf> {
        self.ensure_writable()?;
        let candidate = self.host_path(path);
        let mut ancestor = candidate.as_path();
        while !ancestor.exists() {
            ancestor = ancestor.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "path has no existing ancestor")
            })?;
        }
        let ancestor = ancestor.canonicalize()?;
        if ancestor.starts_with(&self.root) {
            Ok(candidate)
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path escapes workspace root",
            ))
        }
    }

    fn ensure_writable(&self) -> io::Result<()> {
        if self.read_only {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "workspace is read-only",
            ))
        } else {
            Ok(())
        }
    }
}

impl WorkspaceFs for NativeFs {
    fn read_dir(&self, path: &VPath) -> io::Result<Vec<DirEntry>> {
        let host = self.existing_host_path(path)?;
        let mut out = Vec::new();
        for entry in fs::read_dir(host)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = path
                .join(&name)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            out.push(DirEntry {
                meta: self.metadata(&child)?,
                path: child,
                name,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn read_file(&self, path: &VPath) -> io::Result<Vec<u8>> {
        fs::read(self.existing_host_path(path)?)
    }

    fn write_file(&mut self, path: &VPath, data: &[u8]) -> io::Result<()> {
        fs::write(self.writable_host_path(path)?, data)
    }

    fn create_dir(&mut self, path: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        fs::create_dir_all(self.creatable_host_path(path)?)
    }

    fn rename(&mut self, from: &VPath, to: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        fs::rename(self.existing_host_path(from)?, self.writable_host_path(to)?)
    }

    fn remove(&mut self, path: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        let host = self.existing_host_path(path)?;
        if host.is_dir() {
            fs::remove_dir_all(host)
        } else {
            fs::remove_file(host)
        }
    }

    fn metadata(&self, path: &VPath) -> io::Result<Meta> {
        let meta = fs::metadata(self.existing_host_path(path)?)?;
        let kind = if meta.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        Ok(Meta {
            kind,
            len: meta.len(),
            read_only: self.read_only || meta.permissions().readonly(),
        })
    }

    fn read_only(&self) -> bool {
        self.read_only
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn native_fs_reads_writes_and_blocks_escape() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create temp root");
        let mut native = NativeFs::open(&root).expect("open native fs");

        let path = VPath::parse("src/main.hll").expect("path");
        native
            .create_dir(&VPath::parse("src").expect("dir"))
            .expect("dir");
        native.write_file(&path, b"fn main() {}").expect("write");
        assert_eq!(native.read_file(&path).expect("read"), b"fn main() {}");
        assert_eq!(
            native
                .metadata(&VPath::parse("src").expect("dir"))
                .expect("meta")
                .kind,
            EntryKind::Dir
        );
        assert!(VPath::parse("../outside").is_err());

        fs::remove_dir_all(root).expect("cleanup");
    }

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("full_stack_native_fs_test_{nanos}"))
    }
}
