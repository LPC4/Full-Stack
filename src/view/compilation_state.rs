// Shared compilation state

use crate::view::debug::DebugSession;
use crate::view::ide::vm_execution_view::VmExecutionResult;

#[derive(Default)]
pub struct CompilationState {
    pub tokens: String,
    pub ast: String,
    pub ir: String,
    pub asm: String,
    pub stdlib_ir: String,
    pub stdlib_asm: String,
    pub assembly_tokens: Vec<crate::assembly_language::rv_instruction::RvInstruction>,
    pub assembled: Option<crate::assembly_language::assembler::output::AssembledOutput>,
    pub error: Option<String>,
    pub error_summary: Option<String>,
    pub just_compiled: bool,
    pub execution_output: String,
    pub vm_result: Option<VmExecutionResult>,
    pub debug_session: Option<DebugSession>,
    pub entry_symbol: String,
    pub load_base: u64,
}

impl CompilationState {
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
