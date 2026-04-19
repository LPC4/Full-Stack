#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
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
        self.entries.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            message: message.into(),
        });
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.entries.push(Diagnostic {
            level: DiagnosticLevel::Error,
            message: message.into(),
        });
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
