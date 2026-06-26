use crate::compilation_pipeline::PipelineResult;
use crate::view::debug::DebugSession;
use crate::view::ide::vm_execution_view::VmExecutionResult;
use asm_to_binary::AssembledOutput;
use hll_to_ir::Diagnostic;

#[derive(Default)]
pub struct CompilationState {
    pub pipeline: Option<PipelineResult>,
    pub linked_asm_text: String,
    pub error: Option<String>,
    pub error_summary: Option<String>,
    pub just_compiled: bool,
    pub execution_output: String,
    pub vm_result: Option<VmExecutionResult>,
    pub debug_session: Option<DebugSession>,
    pub disasm_follow_pc: bool,
    pub entry_symbol: String,
    pub load_base: u64,
    pub last_hosted_binary: Option<AssembledOutput>,
    /// Set when a diagnostic is clicked: the 1-based source line the editor
    /// should move its cursor to and scroll into view. Consumed by the source view.
    pub goto_line: Option<u32>,
}

impl CompilationState {
    pub fn tokens(&self) -> &str {
        self.pipeline
            .as_ref()
            .and_then(|p| p.lex.as_ref())
            .map(|l| l.display.as_str())
            .unwrap_or("")
    }

    pub fn ast(&self) -> &str {
        self.pipeline
            .as_ref()
            .and_then(|p| p.parse.as_ref())
            .map(|p| p.display.as_str())
            .unwrap_or("")
    }

    pub fn ir(&self) -> &str {
        self.pipeline
            .as_ref()
            .and_then(|p| p.ir.as_ref())
            .map(|i| i.display.as_str())
            .unwrap_or("")
    }

    pub fn asm(&self) -> &str {
        self.pipeline
            .as_ref()
            .and_then(|p| p.asm.as_ref())
            .map(|a| a.display.as_str())
            .unwrap_or("")
    }

    pub fn linked_asm(&self) -> &str {
        if !self.linked_asm_text.is_empty() {
            &self.linked_asm_text
        } else {
            self.asm()
        }
    }

    /// Structured diagnostics from the last compile, if any. These carry level,
    /// message, source span, and note, and survive even when compilation failed.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.pipeline
            .as_ref()
            .map(|p| p.diagnostics.as_slice())
            .unwrap_or(&[])
    }

    pub fn assembled(&self) -> Option<&AssembledOutput> {
        self.pipeline
            .as_ref()
            .and_then(|p| p.binary.as_ref())
            .map(|b| &b.assembled)
    }

    pub fn set_error(&mut self, full: String) {
        let first_meaningful_line = full
            .lines()
            .find(|l| l.trim_start().starts_with("- ") || l.trim_start().starts_with('-'))
            .or_else(|| full.lines().find(|l| !l.trim().is_empty()))
            .unwrap_or("Compilation error");

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
