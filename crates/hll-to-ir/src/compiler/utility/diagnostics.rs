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

    /// The rich, multi-line block with no file name in the location line.
    pub fn format_full(&self) -> String {
        self.render_block(None)
    }

    /// The rich block naming `file` in the location line.
    pub fn render(&self, file: &str) -> String {
        self.render_block(Some(file))
    }

    fn level_word(&self) -> &'static str {
        match self.level {
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Error => "error",
        }
    }

    /// Render the canonical diagnostic block (message, location, source line,
    /// caret, note). ASCII only, so every consumer shows identical text.
    fn render_block(&self, file: Option<&str>) -> String {
        let mut out = format!("{}: {}", self.level_word(), self.message);
        let gutter = if let Some(span) = &self.span {
            let col = span.col.max(1);
            let where_ = match file {
                Some(name) => format!("{name}:{}:{col}", span.line),
                None => format!("{}:{col}", span.line),
            };
            out.push_str(&format!("\n  --> {where_}"));

            let num = span.line.to_string();
            let pad = " ".repeat(num.len());
            if !span.source_line.is_empty() || span.len > 0 {
                let prefix = span
                    .source_line
                    .get(..(col as usize).saturating_sub(1))
                    .unwrap_or("");
                let indent = " ".repeat(prefix.chars().count());
                let carets = "^".repeat(span.caret_width());
                out.push_str(&format!("\n{pad} |"));
                out.push_str(&format!("\n{num} | {}", span.source_line));
                out.push_str(&format!("\n{pad} | {indent}{carets}"));
            }
            pad
        } else {
            String::new()
        };
        if let Some(note) = &self.note {
            out.push_str(&format!("\n{gutter} = note: {note}"));
        }
        out
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

    /// Push a warning located at `span`.
    pub fn warn_at(&mut self, span: Span, message: impl Into<String>) {
        self.push(Diagnostic::new(DiagnosticLevel::Warning, message).with_span(span));
    }

    /// Push an error located at `span`.
    pub fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.push(Diagnostic::new(DiagnosticLevel::Error, message).with_span(span));
    }

    /// Push an already-built diagnostic (use `Diagnostic::with_span`/`with_note`
    /// to attach a location or note before calling).
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.entries.push(diagnostic);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn span(line: u32, col: u32, len: u32, source_line: &str) -> Span {
        Span {
            line,
            col,
            len,
            source_line: source_line.to_owned(),
        }
    }

    #[test]
    fn renders_block_with_caret_under_the_token() {
        let diag = Diagnostic::new(DiagnosticLevel::Error, "unknown identifier `bogus`")
            .with_span(span(2, 12, 5, "    return bogus"))
            .with_note("declare it first");
        let expected = "\
error: unknown identifier `bogus`
  --> demo.hll:2:12
  |
2 |     return bogus
  |            ^^^^^
  = note: declare it first";
        assert_eq!(diag.render("demo.hll"), expected);
    }

    #[test]
    fn format_full_omits_the_file_name() {
        let diag = Diagnostic::new(DiagnosticLevel::Warning, "unused value")
            .with_span(span(1, 1, 3, "let x"));
        let expected = "\
warning: unused value
  --> 1:1
  |
1 | let x
  | ^^^";
        assert_eq!(diag.format_full(), expected);
    }

    #[test]
    fn unspanned_diagnostic_is_a_single_line() {
        let diag = Diagnostic::new(DiagnosticLevel::Error, "no main function");
        assert_eq!(diag.render("demo.hll"), "error: no main function");
    }

    #[test]
    fn zero_length_span_draws_one_caret() {
        let diag = Diagnostic::new(DiagnosticLevel::Error, "unexpected end of input")
            .with_span(span(3, 1, 0, "}"));
        assert!(diag.render("f.hll").ends_with("\n  | ^"));
    }
}
