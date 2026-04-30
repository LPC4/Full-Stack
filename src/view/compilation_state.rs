#[derive(Default)]
pub struct CompilationState {
    pub tokens: String,
    pub ast: String,
    pub ir: String,
    pub asm: String,
    /// Full formatted error message (multi-line, displayed in error panel).
    pub error: Option<String>,
    /// Short one-liner for the top status bar (e.g. "Parse error at line 5").
    pub error_summary: Option<String>,
    pub just_compiled: bool,
    pub execution_output: String,
}

impl CompilationState {
    /// Set both the full error and derive a compact summary from it.
    pub fn set_error(&mut self, full: String) {
        // Try to find the first bulleted error line; fallback to the first non-empty line.
        let first_meaningful_line = full
            .lines()
            .find(|l| l.trim_start().starts_with("- ") || l.trim_start().starts_with("-"))
            .or_else(|| full.lines().find(|l| !l.trim().is_empty()))
            .unwrap_or("Compilation error");

        // Clean up the line (remove leading whitespace and the bullet dash) and cap at 80 chars.
        let summary = first_meaningful_line
            .trim_start_matches(|c: char| c.is_whitespace() || c == '-')
            .trim()
            .chars()
            .take(80)
            .collect::<String>();

        self.error_summary = Some(summary);
        self.error = Some(full);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
        self.error_summary = None;
    }
}
