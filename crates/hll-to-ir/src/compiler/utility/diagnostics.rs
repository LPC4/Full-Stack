use crate::token::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    /// Source location, when available.
    pub span: Option<Span>,
    /// Additional context or suggestion shown below the primary message.
    pub note: Option<String>,
}

impl Diagnostic {
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            span: None,
            note: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Format with location and note if present.
    pub fn format_full(&self) -> String {
        let level = match self.level {
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Error => "error",
        };
        let mut s = if let Some(span) = &self.span {
            format!("{level} at {}: {}", span.location(), self.message)
        } else {
            format!("{level}: {}", self.message)
        };
        if let Some(note) = &self.note {
            s.push_str(&format!("\n  note: {note}"));
        }
        s
    }
}

#[derive(Debug, Default)]
pub struct Diagnostics {
    entries: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.entries
            .push(Diagnostic::new(DiagnosticLevel::Warning, message));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.entries
            .push(Diagnostic::new(DiagnosticLevel::Error, message));
    }

    /// Emit a warning pinned to a source location.
    pub fn warn_at(&mut self, span: Span, message: impl Into<String>) {
        self.entries
            .push(Diagnostic::new(DiagnosticLevel::Warning, message).with_span(span));
    }

    /// Emit an error pinned to a source location.
    pub fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.entries
            .push(Diagnostic::new(DiagnosticLevel::Error, message).with_span(span));
    }

    pub fn has_errors(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry.level, DiagnosticLevel::Error))
    }

    pub fn entries(&self) -> &[Diagnostic] {
        &self.entries
    }
}
