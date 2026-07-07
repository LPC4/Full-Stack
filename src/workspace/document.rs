use crate::workspace::VPath;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub range: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
pub struct Document {
    path: VPath,
    text: String,
    saved_text: String,
    undo: VecDeque<String>,
    redo: VecDeque<String>,
    diagnostics: Vec<Diagnostic>,
    read_only: bool,
    /// Raw content for binary documents (ELF images, objects); always read-only.
    bytes: Option<Vec<u8>>,
}

impl Document {
    pub fn new(path: VPath, text: impl Into<String>, read_only: bool) -> Self {
        let text = text.into();
        Self {
            path,
            saved_text: text.clone(),
            text,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            diagnostics: Vec::new(),
            read_only,
            bytes: None,
        }
    }

    pub fn new_binary(path: VPath, bytes: Vec<u8>) -> Self {
        Self {
            path,
            text: String::new(),
            saved_text: String::new(),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            diagnostics: Vec::new(),
            read_only: true,
            bytes: Some(bytes),
        }
    }

    pub fn is_binary(&self) -> bool {
        self.bytes.is_some()
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }

    pub fn path(&self) -> &VPath {
        &self.path
    }

    /// Follow a rename of this document's file; content and history are kept.
    pub(crate) fn set_path(&mut self, path: VPath) {
        self.path = path;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), DocumentError> {
        if self.read_only {
            return Err(DocumentError::ReadOnly);
        }
        let text = text.into();
        if self.text == text {
            return Ok(());
        }
        self.undo.push_back(std::mem::replace(&mut self.text, text));
        self.redo.clear();
        Ok(())
    }

    pub fn mark_saved(&mut self) {
        self.saved_text.clone_from(&self.text);
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push_back(std::mem::replace(&mut self.text, prev));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        self.undo.push_back(std::mem::replace(&mut self.text, next));
        true
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentError {
    ReadOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_state_tracks_saved_text() {
        let mut doc = Document::new(VPath::parse("main.hll").expect("path"), "a", false);
        assert!(!doc.is_dirty());
        doc.set_text("b").expect("edit");
        assert!(doc.is_dirty());
        doc.mark_saved();
        assert!(!doc.is_dirty());
    }

    #[test]
    fn undo_and_redo_restore_text() {
        let mut doc = Document::new(VPath::parse("main.hll").expect("path"), "a", false);
        doc.set_text("b").expect("edit");
        assert!(doc.undo());
        assert_eq!(doc.text(), "a");
        assert!(doc.redo());
        assert_eq!(doc.text(), "b");
    }

    #[test]
    fn read_only_documents_reject_edits() {
        let mut doc = Document::new(VPath::parse("stdlib/core.hll").expect("path"), "a", true);
        assert_eq!(doc.set_text("b"), Err(DocumentError::ReadOnly));
        assert_eq!(doc.text(), "a");
    }
}
