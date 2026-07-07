use crate::workspace::{EntryKind, VPath, WorkspaceFs};
use std::io;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    pub path: VPath,
    pub name: String,
    pub kind: EntryKind,
    pub len: u64,
    pub read_only: bool,
    pub children: Vec<TreeEntry>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceTree {
    pub root: Vec<TreeEntry>,
}

impl WorkspaceTree {
    pub fn refresh(fs: &dyn WorkspaceFs) -> io::Result<Self> {
        let mut root = read_children(fs, &VPath::root())?;
        sort_entries(&mut root);
        Ok(Self { root })
    }

    pub fn flatten(&self) -> Vec<&TreeEntry> {
        let mut out = Vec::new();
        for entry in &self.root {
            flatten_entry(entry, &mut out);
        }
        out
    }
}

fn read_children(fs: &dyn WorkspaceFs, path: &VPath) -> io::Result<Vec<TreeEntry>> {
    let mut out = Vec::new();
    for entry in fs.read_dir(path)? {
        let children = if entry.meta.kind == EntryKind::Dir {
            read_children(fs, &entry.path)?
        } else {
            Vec::new()
        };
        out.push(TreeEntry {
            path: entry.path,
            name: entry.name,
            kind: entry.meta.kind,
            len: entry.meta.len,
            read_only: entry.meta.read_only,
            children,
        });
    }
    sort_entries(&mut out);
    Ok(out)
}

fn sort_entries(entries: &mut [TreeEntry]) {
    entries.sort_by(|a, b| match (&a.kind, &b.kind) {
        (EntryKind::Dir, EntryKind::File) => std::cmp::Ordering::Less,
        (EntryKind::File, EntryKind::Dir) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    for entry in entries {
        sort_entries(&mut entry.children);
    }
}

fn flatten_entry<'a>(entry: &'a TreeEntry, out: &mut Vec<&'a TreeEntry>) {
    out.push(entry);
    for child in &entry.children {
        flatten_entry(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryFs;

    #[test]
    fn tree_refresh_sorts_dirs_before_files() {
        let mut fs = MemoryFs::new();
        fs.write_text(&VPath::parse("z.txt").expect("path"), "z")
            .expect("write");
        fs.write_text(&VPath::parse("a/main.hll").expect("path"), "a")
            .expect("write");

        let tree = WorkspaceTree::refresh(&fs).expect("tree");
        assert_eq!(tree.root[0].name, "a");
        assert_eq!(tree.root[1].name, "z.txt");
        assert_eq!(tree.flatten().len(), 3);
    }
}
