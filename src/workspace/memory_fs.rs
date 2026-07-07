use crate::workspace::{DirEntry, EntryKind, Meta, VPath, WorkspaceFs};
use std::collections::{BTreeMap, BTreeSet};
use std::io;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct MemoryFs {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    read_only: bool,
}

impl MemoryFs {
    pub fn new() -> Self {
        let mut fs = Self::default();
        fs.dirs.insert(String::new());
        fs
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn write_text(&mut self, path: &VPath, text: &str) -> io::Result<()> {
        self.write_file(path, text.as_bytes())
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

    fn ensure_parent_dir(&mut self, path: &VPath) -> io::Result<()> {
        let mut current = VPath::root();
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        for part in parent.as_str().split('/').filter(|part| !part.is_empty()) {
            current = current.join(part).map_err(invalid_path)?;
            self.dirs.insert(current.as_str().to_owned());
        }
        Ok(())
    }

    fn contains_dir(&self, path: &VPath) -> bool {
        self.dirs.contains(path.as_str())
    }
}

impl WorkspaceFs for MemoryFs {
    fn read_dir(&self, path: &VPath) -> io::Result<Vec<DirEntry>> {
        if !self.contains_dir(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "directory not found",
            ));
        }
        let prefix = if path.is_root() {
            String::new()
        } else {
            format!("{}/", path.as_str())
        };
        let mut names = BTreeSet::new();
        for dir in &self.dirs {
            if let Some(rest) = dir.strip_prefix(&prefix)
                && !rest.is_empty()
                && !rest.contains('/')
            {
                names.insert(rest.to_owned());
            }
        }
        for file in self.files.keys() {
            if let Some(rest) = file.strip_prefix(&prefix)
                && !rest.is_empty()
                && !rest.contains('/')
            {
                names.insert(rest.to_owned());
            }
        }

        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let child = path.join(&name).map_err(invalid_path)?;
            let meta = self.metadata(&child)?;
            entries.push(DirEntry {
                path: child,
                name,
                meta,
            });
        }
        Ok(entries)
    }

    fn read_file(&self, path: &VPath) -> io::Result<Vec<u8>> {
        self.files
            .get(path.as_str())
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))
    }

    fn write_file(&mut self, path: &VPath, data: &[u8]) -> io::Result<()> {
        self.ensure_writable()?;
        if path.is_root() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "root is not a file",
            ));
        }
        self.ensure_parent_dir(path)?;
        self.files.insert(path.as_str().to_owned(), data.to_vec());
        Ok(())
    }

    fn create_dir(&mut self, path: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        self.ensure_parent_dir(path)?;
        self.dirs.insert(path.as_str().to_owned());
        Ok(())
    }

    fn rename(&mut self, from: &VPath, to: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        self.ensure_parent_dir(to)?;
        if let Some(data) = self.files.remove(from.as_str()) {
            self.files.insert(to.as_str().to_owned(), data);
            return Ok(());
        }
        if !self.dirs.remove(from.as_str()) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"));
        }
        let from_prefix = format!("{}/", from.as_str());
        let to_prefix = if to.is_root() {
            String::new()
        } else {
            format!("{}/", to.as_str())
        };
        let dirs: Vec<String> = self
            .dirs
            .iter()
            .filter(|dir| dir.starts_with(&from_prefix))
            .cloned()
            .collect();
        for dir in dirs {
            self.dirs.remove(&dir);
            self.dirs
                .insert(format!("{to_prefix}{}", &dir[from_prefix.len()..]));
        }
        let files: Vec<String> = self
            .files
            .keys()
            .filter(|file| file.starts_with(&from_prefix))
            .cloned()
            .collect();
        for file in files {
            if let Some(data) = self.files.remove(&file) {
                self.files
                    .insert(format!("{to_prefix}{}", &file[from_prefix.len()..]), data);
            }
        }
        self.dirs.insert(to.as_str().to_owned());
        Ok(())
    }

    fn remove(&mut self, path: &VPath) -> io::Result<()> {
        self.ensure_writable()?;
        if self.files.remove(path.as_str()).is_some() {
            return Ok(());
        }
        if !self.dirs.remove(path.as_str()) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"));
        }
        let prefix = format!("{}/", path.as_str());
        self.dirs.retain(|dir| !dir.starts_with(&prefix));
        self.files.retain(|file, _| !file.starts_with(&prefix));
        Ok(())
    }

    fn metadata(&self, path: &VPath) -> io::Result<Meta> {
        if self.contains_dir(path) {
            return Ok(Meta {
                kind: EntryKind::Dir,
                len: 0,
                read_only: self.read_only,
            });
        }
        if let Some(data) = self.files.get(path.as_str()) {
            return Ok(Meta {
                kind: EntryKind::File,
                len: data.len() as u64,
                read_only: self.read_only,
            });
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
    }

    fn read_only(&self) -> bool {
        self.read_only
    }
}

fn invalid_path(err: crate::workspace::PathError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_reads_and_lists_files() {
        let mut fs = MemoryFs::new();
        let path = VPath::parse("programs/demo/main.hll").expect("path");
        fs.write_text(&path, "fn main() {}").expect("write");

        assert_eq!(fs.read_file(&path).expect("read"), b"fn main() {}");
        let root = fs.read_dir(&VPath::root()).expect("root");
        assert_eq!(root[0].name, "programs");
        assert_eq!(root[0].meta.kind, EntryKind::Dir);
    }

    #[test]
    fn renames_directories_with_children() {
        let mut fs = MemoryFs::new();
        fs.write_text(&VPath::parse("a/b.txt").expect("path"), "b")
            .expect("write");
        fs.rename(
            &VPath::parse("a").expect("from"),
            &VPath::parse("c").expect("to"),
        )
        .expect("rename");

        assert_eq!(
            fs.read_file(&VPath::parse("c/b.txt").expect("path"))
                .expect("read"),
            b"b"
        );
        assert!(
            fs.read_file(&VPath::parse("a/b.txt").expect("path"))
                .is_err()
        );
    }

    #[test]
    fn read_only_blocks_mutation() {
        let mut fs = MemoryFs::new().read_only(true);
        let err = fs
            .write_text(&VPath::parse("x").expect("path"), "x")
            .expect_err("read-only write should fail");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
