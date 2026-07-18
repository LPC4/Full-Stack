pub mod bundle;
pub mod document;
pub mod fs;
pub mod manifest;
pub mod memory_fs;
#[cfg(not(target_arch = "wasm32"))]
pub mod native_fs;
pub mod path;
pub mod state;
pub mod tree;

pub use bundle::{
    populate_demo_workspace, populate_teaching_workspace, sample_workspace, teaching_workspace,
};
pub use document::{Diagnostic, DiagnosticSeverity, Document};
pub use fs::{DirEntry, EntryKind, Meta, WorkspaceFs};
pub use manifest::{WorkspaceManifestDiagnostic, load_build_manifest};
pub use memory_fs::MemoryFs;
#[cfg(not(target_arch = "wasm32"))]
pub use native_fs::NativeFs;
pub use path::{PathError, VPath};
pub use state::{IdeWorkspace, WorkspaceError};
pub use tree::{TreeEntry, WorkspaceTree};
