use crate::runconfig::RunConfigStore;
use crate::workspace::{
    Document, EntryKind, MemoryFs, VPath, WorkspaceFs, WorkspaceManifestDiagnostic, WorkspaceTree,
    lessons_workspace, load_build_manifest, sample_workspace,
};
use std::collections::BTreeMap;
use std::io;

pub struct IdeWorkspace {
    fs: Box<dyn WorkspaceFs>,
    tree: WorkspaceTree,
    documents: BTreeMap<VPath, Document>,
    /// Paths in the order they were opened; drives the editor tab strip.
    open_order: Vec<VPath>,
    active_document: Option<VPath>,
    run_configs: RunConfigStore,
}

#[derive(Debug)]
pub enum WorkspaceError {
    Io(io::Error),
    Utf8(std::string::FromUtf8Error),
    ReadOnly,
    NoActiveDocument,
    Manifest(Vec<WorkspaceManifestDiagnostic>),
}

impl IdeWorkspace {
    pub fn new(fs: Box<dyn WorkspaceFs>) -> io::Result<Self> {
        let tree = WorkspaceTree::refresh(fs.as_ref())?;
        let run_configs = RunConfigStore::load(fs.as_ref()).unwrap_or_default();
        Ok(Self {
            fs,
            tree,
            documents: BTreeMap::new(),
            open_order: Vec::new(),
            active_document: None,
            run_configs,
        })
    }

    pub fn sample() -> io::Result<Self> {
        Self::new(Box::new(sample_workspace()?))
    }

    pub fn lessons() -> io::Result<Self> {
        Self::new(Box::new(lessons_workspace()?))
    }

    pub fn fs(&self) -> &dyn WorkspaceFs {
        self.fs.as_ref()
    }

    pub fn fs_mut(&mut self) -> &mut dyn WorkspaceFs {
        self.fs.as_mut()
    }

    pub fn tree(&self) -> &WorkspaceTree {
        &self.tree
    }

    pub fn refresh_tree(&mut self) -> io::Result<()> {
        self.tree = WorkspaceTree::refresh(self.fs.as_ref())?;
        Ok(())
    }

    /// Create an empty file; fails when an entry already exists at `path`.
    pub fn create_file(&mut self, path: &VPath) -> Result<(), WorkspaceError> {
        self.ensure_vacant(path)?;
        self.fs.write_file(path, b"").map_err(WorkspaceError::Io)?;
        self.refresh_tree().map_err(WorkspaceError::Io)
    }

    /// Create a directory; fails when an entry already exists at `path`.
    pub fn create_dir(&mut self, path: &VPath) -> Result<(), WorkspaceError> {
        self.ensure_vacant(path)?;
        self.fs.create_dir(path).map_err(WorkspaceError::Io)?;
        self.refresh_tree().map_err(WorkspaceError::Io)
    }

    /// Rename a file or directory. Open documents at or under `from` follow the
    /// rename, keeping their text, dirty state, tab slot, and focus.
    pub fn rename_entry(&mut self, from: &VPath, to: &VPath) -> Result<(), WorkspaceError> {
        if from.is_root() || to.is_root() {
            return Err(WorkspaceError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the workspace root cannot be renamed",
            )));
        }
        if from == to {
            return Ok(());
        }
        self.ensure_vacant(to)?;
        self.fs.rename(from, to).map_err(WorkspaceError::Io)?;
        let moved: Vec<VPath> = self
            .documents
            .keys()
            .filter(|path| is_at_or_under(path, from))
            .cloned()
            .collect();
        for old in moved {
            let Some(new) = rebase(&old, from, to) else {
                continue;
            };
            if let Some(mut document) = self.documents.remove(&old) {
                document.set_path(new.clone());
                self.documents.insert(new.clone(), document);
            }
            for slot in &mut self.open_order {
                if *slot == old {
                    *slot = new.clone();
                }
            }
            if self.active_document.as_ref() == Some(&old) {
                self.active_document = Some(new);
            }
        }
        self.refresh_tree().map_err(WorkspaceError::Io)
    }

    /// Delete a file or directory (recursively); open documents at or under it
    /// close, discarding unsaved edits.
    pub fn remove_entry(&mut self, path: &VPath) -> Result<(), WorkspaceError> {
        if path.is_root() {
            return Err(WorkspaceError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the workspace root cannot be deleted",
            )));
        }
        self.fs.remove(path).map_err(WorkspaceError::Io)?;
        let closed: Vec<VPath> = self
            .documents
            .keys()
            .filter(|open| is_at_or_under(open, path))
            .cloned()
            .collect();
        for open in closed {
            self.close_document(&open);
        }
        self.refresh_tree().map_err(WorkspaceError::Io)
    }

    fn ensure_vacant(&self, path: &VPath) -> Result<(), WorkspaceError> {
        if self.fs.metadata(path).is_ok() {
            return Err(WorkspaceError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{path} already exists"),
            )));
        }
        Ok(())
    }

    pub fn open_document(&mut self, path: VPath) -> Result<(), WorkspaceError> {
        if !self.documents.contains_key(&path) {
            let meta = self.fs.metadata(&path).map_err(WorkspaceError::Io)?;
            if meta.kind != EntryKind::File {
                return Err(WorkspaceError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "document path is not a file",
                )));
            }
            let bytes = self.fs.read_file(&path).map_err(WorkspaceError::Io)?;
            // Known binary artifacts open as raw bytes; anything else falls back
            // to a binary document only when it is not valid UTF-8 text.
            let document = if is_binary_path(path.as_str()) {
                Document::new_binary(path.clone(), bytes)
            } else {
                match String::from_utf8(bytes) {
                    Ok(text) => Document::new(path.clone(), text, meta.read_only),
                    Err(err) => Document::new_binary(path.clone(), err.into_bytes()),
                }
            };
            self.documents.insert(path.clone(), document);
        }
        if !self.open_order.contains(&path) {
            self.open_order.push(path.clone());
        }
        self.active_document = Some(path);
        Ok(())
    }

    /// Close an open document, discarding unsaved edits. When it was active, the
    /// nearest remaining tab (same slot, else previous) becomes active.
    pub fn close_document(&mut self, path: &VPath) {
        self.documents.remove(path);
        let Some(idx) = self.open_order.iter().position(|open| open == path) else {
            return;
        };
        self.open_order.remove(idx);
        if self.active_document.as_ref() == Some(path) {
            self.active_document = self
                .open_order
                .get(idx)
                .or_else(|| self.open_order.last())
                .cloned();
        }
    }

    /// Make an already-open document active; ignored for paths that are not open.
    pub fn activate_document(&mut self, path: &VPath) {
        if self.documents.contains_key(path) {
            self.active_document = Some(path.clone());
        }
    }

    /// Open documents in the order they were opened (the tab order).
    pub fn open_paths(&self) -> &[VPath] {
        &self.open_order
    }

    pub fn document(&self, path: &VPath) -> Option<&Document> {
        self.documents.get(path)
    }

    pub fn active_document(&self) -> Option<&Document> {
        let path = self.active_document.as_ref()?;
        self.documents.get(path)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        let path = self.active_document.clone()?;
        self.documents.get_mut(&path)
    }

    pub fn documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    pub fn dirty_documents(&self) -> impl Iterator<Item = &Document> {
        self.documents
            .values()
            .filter(|document| document.is_dirty())
    }

    pub fn set_active_text(&mut self, text: impl Into<String>) -> Result<(), WorkspaceError> {
        let Some(document) = self.active_document_mut() else {
            return Err(WorkspaceError::NoActiveDocument);
        };
        document
            .set_text(text)
            .map_err(|_err| WorkspaceError::ReadOnly)
    }

    pub fn save_active_document(&mut self) -> Result<(), WorkspaceError> {
        let path = self
            .active_document
            .clone()
            .ok_or(WorkspaceError::NoActiveDocument)?;
        self.save_document(&path)
    }

    pub fn save_document(&mut self, path: &VPath) -> Result<(), WorkspaceError> {
        let Some(document) = self.documents.get_mut(path) else {
            return Ok(());
        };
        if document.is_read_only() {
            return Err(WorkspaceError::ReadOnly);
        }
        self.fs
            .write_file(path, document.text().as_bytes())
            .map_err(WorkspaceError::Io)?;
        document.mark_saved();
        self.refresh_tree().map_err(WorkspaceError::Io)?;
        Ok(())
    }

    pub fn save_all(&mut self) -> Result<(), WorkspaceError> {
        let dirty: Vec<VPath> = self
            .dirty_documents()
            .map(|document| document.path().clone())
            .collect();
        for path in dirty {
            self.save_document(&path)?;
        }
        Ok(())
    }

    pub fn run_configs(&self) -> &RunConfigStore {
        &self.run_configs
    }

    pub fn run_configs_mut(&mut self) -> &mut RunConfigStore {
        &mut self.run_configs
    }

    pub fn reload_run_configs(&mut self) -> io::Result<()> {
        self.run_configs = RunConfigStore::load(self.fs.as_ref()).unwrap_or_default();
        Ok(())
    }

    pub fn save_run_configs(&mut self) -> io::Result<()> {
        self.run_configs.save(self.fs.as_mut())?;
        self.refresh_tree()
    }

    pub fn load_manifest(
        &self,
        path: &VPath,
    ) -> Result<crate::build::BuildManifest, WorkspaceError> {
        load_build_manifest(self.fs.as_ref(), path).map_err(WorkspaceError::Manifest)
    }
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Utf8(err) => write!(f, "{err}"),
            Self::ReadOnly => write!(f, "the document is read-only"),
            Self::NoActiveDocument => write!(f, "no document is active"),
            Self::Manifest(diags) => match diags.first() {
                Some(diag) => write!(f, "{}", diag.message),
                None => write!(f, "invalid build manifest"),
            },
        }
    }
}

/// True when `path` equals `ancestor` or lives inside it.
fn is_at_or_under(path: &VPath, ancestor: &VPath) -> bool {
    path == ancestor
        || path
            .as_str()
            .strip_prefix(ancestor.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Rewrite `path` from under `from` to the same position under `to`.
fn rebase(path: &VPath, from: &VPath, to: &VPath) -> Option<VPath> {
    if path == from {
        return Some(to.clone());
    }
    let rest = path.as_str().strip_prefix(from.as_str())?;
    VPath::parse(&format!("{}{rest}", to.as_str())).ok()
}

/// Extensions the toolchain emits as binary artifacts.
fn is_binary_path(path: &str) -> bool {
    path.ends_with(".elf") || path.ends_with(".o")
}

impl Default for IdeWorkspace {
    fn default() -> Self {
        Self::sample().unwrap_or_else(|_| {
            Self::new(Box::new(MemoryFs::new())).expect("empty memory workspace")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_edits_and_saves_document() {
        let mut fs = MemoryFs::new();
        let path = VPath::parse("src/main.hll").expect("path");
        fs.write_text(&path, "old").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");

        workspace.open_document(path.clone()).expect("open");
        workspace.set_active_text("new").expect("edit");
        assert_eq!(workspace.dirty_documents().count(), 1);
        workspace.save_active_document().expect("save");

        assert_eq!(workspace.fs().read_file(&path).expect("read"), b"new");
        assert_eq!(workspace.dirty_documents().count(), 0);
    }

    #[test]
    fn close_keeps_tab_order_and_picks_a_neighbor() {
        let mut fs = MemoryFs::new();
        let paths: Vec<VPath> = ["a.hll", "b.hll", "c.hll"]
            .iter()
            .map(|name| VPath::parse(name).expect("path"))
            .collect();
        for path in &paths {
            fs.write_text(path, "x").expect("write");
        }
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
        for path in &paths {
            workspace.open_document(path.clone()).expect("open");
        }
        assert_eq!(workspace.open_paths(), &paths[..]);

        // Closing the active last tab falls back to the previous one.
        workspace.close_document(&paths[2]);
        assert_eq!(
            workspace.active_document().expect("active").path(),
            &paths[1]
        );
        // Closing an inactive tab leaves the active document alone.
        workspace.close_document(&paths[0]);
        assert_eq!(
            workspace.active_document().expect("active").path(),
            &paths[1]
        );
        assert_eq!(workspace.open_paths(), &paths[1..2]);

        workspace.close_document(&paths[1]);
        assert!(workspace.active_document().is_none());
        assert!(workspace.open_paths().is_empty());
    }

    #[test]
    fn elf_and_object_files_open_as_read_only_binary_documents() {
        let mut fs = MemoryFs::new();
        let elf = VPath::parse("out/demo.elf").expect("path");
        // Valid UTF-8 on purpose: the extension alone must force binary mode.
        fs.write_file(&elf, b"\x7fELF fake").expect("write");
        let noisy = VPath::parse("data.blob").expect("path");
        fs.write_file(&noisy, &[0xFF, 0xFE, 0x00, 0x9C])
            .expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");

        workspace.open_document(elf.clone()).expect("open elf");
        let doc = workspace.active_document().expect("active");
        assert!(doc.is_binary());
        assert!(doc.is_read_only());
        assert_eq!(doc.bytes(), Some(b"\x7fELF fake".as_slice()));
        assert!(workspace.set_active_text("edit").is_err());

        // Unknown extensions with invalid UTF-8 fall back to binary too.
        workspace.open_document(noisy).expect("open blob");
        assert!(workspace.active_document().expect("active").is_binary());
    }

    #[test]
    fn activate_switches_between_open_documents() {
        let mut fs = MemoryFs::new();
        let a = VPath::parse("a.hll").expect("path");
        let b = VPath::parse("b.hll").expect("path");
        fs.write_text(&a, "x").expect("write");
        fs.write_text(&b, "y").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
        workspace.open_document(a.clone()).expect("open");
        workspace.open_document(b.clone()).expect("open");

        workspace.activate_document(&a);
        assert_eq!(workspace.active_document().expect("active").path(), &a);
        // Unknown paths do not steal focus.
        workspace.activate_document(&VPath::parse("missing.hll").expect("path"));
        assert_eq!(workspace.active_document().expect("active").path(), &a);
    }

    #[test]
    fn persists_run_configs_into_workspace() {
        let mut workspace = IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace");
        workspace
            .run_configs_mut()
            .configs
            .push(crate::runconfig::RunConfig::hosted(
                "Demo",
                VPath::parse("programs/demo/demo.build").expect("path"),
            ));
        workspace.save_run_configs().expect("save configs");

        assert!(
            workspace
                .fs()
                .read_file(&VPath::parse(".fullstack/run.toml").expect("path"))
                .is_ok()
        );
    }

    #[test]
    fn create_file_and_dir_show_up_and_reject_duplicates() {
        let mut workspace = IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace");
        let dir = VPath::parse("src").expect("path");
        let file = VPath::parse("src/main.hll").expect("path");

        workspace.create_dir(&dir).expect("create dir");
        workspace.create_file(&file).expect("create file");
        assert!(
            workspace
                .tree()
                .flatten()
                .iter()
                .any(|entry| entry.path == file)
        );
        assert!(workspace.create_file(&file).is_err());
        assert!(workspace.create_dir(&dir).is_err());
    }

    #[test]
    fn rename_remaps_open_documents_and_keeps_edits() {
        let mut fs = MemoryFs::new();
        let old = VPath::parse("src/a.hll").expect("path");
        fs.write_text(&old, "original").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
        workspace.open_document(old.clone()).expect("open");
        workspace.set_active_text("edited").expect("edit");

        let new = VPath::parse("src/b.hll").expect("path");
        workspace.rename_entry(&old, &new).expect("rename");

        let active = workspace.active_document().expect("active");
        assert_eq!(active.path(), &new);
        assert_eq!(active.text(), "edited");
        assert!(active.is_dirty());
        assert_eq!(workspace.open_paths(), std::slice::from_ref(&new));
        assert!(workspace.fs().metadata(&old).is_err());
        // Saving after the rename writes to the new location.
        workspace.save_active_document().expect("save");
        assert_eq!(workspace.fs().read_file(&new).expect("read"), b"edited");
    }

    #[test]
    fn renaming_a_directory_carries_open_children() {
        let mut fs = MemoryFs::new();
        let inner = VPath::parse("old/deep/main.hll").expect("path");
        fs.write_text(&inner, "x").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
        workspace.open_document(inner).expect("open");

        workspace
            .rename_entry(
                &VPath::parse("old").expect("from"),
                &VPath::parse("new").expect("to"),
            )
            .expect("rename dir");

        assert_eq!(
            workspace.active_document().expect("active").path().as_str(),
            "new/deep/main.hll"
        );
        // A sibling prefix ("older/...") must not be treated as a child of "old".
        assert!(!is_at_or_under(
            &VPath::parse("older/x.hll").expect("path"),
            &VPath::parse("old").expect("path"),
        ));
    }

    #[test]
    fn rename_rejects_existing_targets_and_the_root() {
        let mut fs = MemoryFs::new();
        let a = VPath::parse("a.hll").expect("path");
        let b = VPath::parse("b.hll").expect("path");
        fs.write_text(&a, "a").expect("write");
        fs.write_text(&b, "b").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");

        assert!(workspace.rename_entry(&a, &b).is_err());
        assert!(workspace.rename_entry(&VPath::root(), &a).is_err());
        assert_eq!(workspace.fs().read_file(&b).expect("read"), b"b");
    }

    #[test]
    fn remove_closes_documents_under_the_deleted_entry() {
        let mut fs = MemoryFs::new();
        let doomed = VPath::parse("gone/main.hll").expect("path");
        let kept = VPath::parse("kept.hll").expect("path");
        fs.write_text(&doomed, "x").expect("write");
        fs.write_text(&kept, "y").expect("write");
        let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
        workspace.open_document(kept.clone()).expect("open kept");
        workspace.open_document(doomed).expect("open doomed");

        workspace
            .remove_entry(&VPath::parse("gone").expect("path"))
            .expect("remove");

        assert_eq!(workspace.open_paths(), std::slice::from_ref(&kept));
        assert_eq!(workspace.active_document().expect("active").path(), &kept);
        assert!(workspace.remove_entry(&VPath::root()).is_err());
    }

    #[test]
    fn sample_workspace_opens_a_real_example_file() {
        let mut workspace = IdeWorkspace::sample().expect("sample");
        workspace
            .open_document(
                VPath::parse("programs/example/core_basics/core_basics.hll").expect("path"),
            )
            .expect("open");
        assert!(
            workspace
                .active_document()
                .expect("document")
                .text()
                .contains("main")
        );
    }
}
