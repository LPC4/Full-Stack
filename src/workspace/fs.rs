use crate::workspace::VPath;
use std::io;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Dir,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Meta {
    pub kind: EntryKind,
    pub len: u64,
    pub read_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry {
    pub path: VPath,
    pub name: String,
    pub meta: Meta,
}

pub trait WorkspaceFs {
    fn read_dir(&self, path: &VPath) -> io::Result<Vec<DirEntry>>;
    fn read_file(&self, path: &VPath) -> io::Result<Vec<u8>>;
    fn write_file(&mut self, path: &VPath, data: &[u8]) -> io::Result<()>;
    fn create_dir(&mut self, path: &VPath) -> io::Result<()>;
    fn rename(&mut self, from: &VPath, to: &VPath) -> io::Result<()>;
    fn remove(&mut self, path: &VPath) -> io::Result<()>;
    fn metadata(&self, path: &VPath) -> io::Result<Meta>;
    fn read_only(&self) -> bool;
}
