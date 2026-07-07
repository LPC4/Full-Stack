//! Editor-facing view of build diagnostics: spans mapped onto the compiled
//! source as line markers and byte ranges the code view can underline.

use crate::ui::theme;
use eframe::egui;
use full_stack::compilation_pipeline::{Diagnostic, DiagnosticLevel};

/// One spanned diagnostic located in the compiled source.
pub(crate) struct DiagMark {
    /// 1-based source line.
    pub line: usize,
    /// Byte range to underline; None when the span has no width on its line.
    pub range: Option<(usize, usize)>,
    pub error: bool,
    /// Message plus the note, ready for hover display.
    pub message: String,
}

impl DiagMark {
    pub fn color(&self) -> egui::Color32 {
        if self.error {
            theme::DANGER
        } else {
            theme::WARNING
        }
    }
}

/// Map spanned diagnostics onto `source`, the exact text that was compiled.
/// Unspanned diagnostics are skipped; they only exist in the Output log.
pub(crate) fn marks_for_source(diagnostics: &[Diagnostic], source: &str) -> Vec<DiagMark> {
    let line_starts = line_start_offsets(source);
    diagnostics
        .iter()
        .filter_map(|diag| {
            let span = diag.span.as_ref()?;
            let line = span.line as usize;
            let line_start = *line_starts.get(line.checked_sub(1)?)?;
            let line_end = source[line_start..]
                .find('\n')
                .map_or(source.len(), |i| line_start + i);
            let start = (line_start + (span.col as usize).saturating_sub(1)).min(line_end);
            let mut message = format!("{}: {}", level_tag(diag.level), diag.message);
            if let Some(note) = &diag.note {
                message.push_str("\nnote: ");
                message.push_str(note);
            }
            Some(DiagMark {
                line,
                range: span_range(source, start, line_end, span.len as usize),
                error: diag.level == DiagnosticLevel::Error,
                message,
            })
        })
        .collect()
}

/// (errors, warnings) tallies for the status bar chip.
pub(crate) fn count_levels(diagnostics: &[Diagnostic]) -> (usize, usize) {
    let errors = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    (errors, diagnostics.len() - errors)
}

/// Byte offset of the start of 1-based `line`, clamped to the text end.
pub(crate) fn byte_offset_of_line(text: &str, line: usize) -> usize {
    let starts = line_start_offsets(text);
    starts
        .get(line.saturating_sub(1))
        .copied()
        .unwrap_or(text.len())
}

fn level_tag(level: DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Error => "error",
        DiagnosticLevel::Warning => "warning",
    }
}

fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// The underline range: `len` bytes from `start` when the span has a width,
/// else the identifier-like token at `start` as a fallback.
fn span_range(source: &str, start: usize, line_end: usize, len: usize) -> Option<(usize, usize)> {
    if start >= line_end || !source.is_char_boundary(start) {
        return None;
    }
    if len > 0 {
        let mut end = (start + len).min(line_end);
        while end < source.len() && !source.is_char_boundary(end) {
            end += 1;
        }
        return Some((start, end));
    }
    let bytes = source.as_bytes();
    let mut end = start;
    while end < line_end && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if end == start {
        end = source[start..line_end]
            .chars()
            .next()
            .map(|c| start + c.len_utf8())?;
    }
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use full_stack::compilation_pipeline::Diagnostic;
    use hll_to_ir::Span;

    fn spanned(level: DiagnosticLevel, message: &str, line: u32, col: u32) -> Diagnostic {
        spanned_len(level, message, line, col, 0)
    }

    fn spanned_len(
        level: DiagnosticLevel,
        message: &str,
        line: u32,
        col: u32,
        len: u32,
    ) -> Diagnostic {
        let mut diag = Diagnostic::new(level, message);
        diag.span = Some(Span {
            line,
            col,
            len,
            source_line: String::new(),
        });
        diag
    }

    #[test]
    fn width_span_underlines_exactly_its_bytes() {
        // An operator span (len 2) underlines `==`, not an identifier run.
        let source = "if a == b {\n}\n";
        let diags = vec![spanned_len(
            DiagnosticLevel::Error,
            "type mismatch",
            1,
            6,
            2,
        )];
        let marks = marks_for_source(&diags, source);
        let (start, end) = marks[0].range.expect("range");
        assert_eq!(&source[start..end], "==");
    }

    #[test]
    fn marks_map_spans_to_identifier_ranges() {
        let source = "main: () -> i32 {\n    return bogus\n}\n";
        let diags = vec![spanned(
            DiagnosticLevel::Error,
            "unknown identifier `bogus`",
            2,
            12,
        )];
        let marks = marks_for_source(&diags, source);
        assert_eq!(marks.len(), 1);
        let mark = &marks[0];
        assert_eq!(mark.line, 2);
        assert!(mark.error);
        let (start, end) = mark.range.expect("range");
        assert_eq!(&source[start..end], "bogus");
    }

    #[test]
    fn unspanned_and_out_of_range_diagnostics_yield_no_marks() {
        let source = "short\n";
        let unspanned = Diagnostic::new(DiagnosticLevel::Error, "no span");
        let past_the_end = spanned(DiagnosticLevel::Warning, "line 99", 99, 1);
        assert!(marks_for_source(&[unspanned], source).is_empty());
        assert!(marks_for_source(&[past_the_end], source).is_empty());
        // A column past the line end keeps the line marker, drops the underline.
        let wide_col = spanned(DiagnosticLevel::Warning, "wide", 1, 40);
        let marks = marks_for_source(&[wide_col], source);
        assert_eq!(marks.len(), 1);
        assert!(marks[0].range.is_none());
        assert!(!marks[0].error);
    }

    #[test]
    fn notes_join_the_hover_message() {
        let source = "x\n";
        let diag = spanned(DiagnosticLevel::Error, "bad", 1, 1).with_note("try this");
        let marks = marks_for_source(&[diag], source);
        assert_eq!(marks[0].message, "error: bad\nnote: try this");
    }

    #[test]
    fn line_offsets_and_level_counts() {
        let text = "one\ntwo\nthree";
        assert_eq!(byte_offset_of_line(text, 1), 0);
        assert_eq!(byte_offset_of_line(text, 2), 4);
        assert_eq!(byte_offset_of_line(text, 3), 8);
        assert_eq!(byte_offset_of_line(text, 9), text.len());
        let diags = vec![
            Diagnostic::new(DiagnosticLevel::Error, "e"),
            Diagnostic::new(DiagnosticLevel::Warning, "w"),
            Diagnostic::new(DiagnosticLevel::Warning, "w2"),
        ];
        assert_eq!(count_levels(&diags), (1, 2));
    }
}
