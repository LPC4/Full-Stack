use crate::compiler::utility::diagnostics::Diagnostics;
use crate::compiler::utility::symbol_table::SymbolTable;
use crate::compiler::utility::type_context::TypeContext;
use crate::ir::values::{IrLabel, IrValue};
use crate::token::Span;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct LoweringContext {
    pub symbols: SymbolTable,
    pub types: TypeContext,
    pub diagnostics: Diagnostics,
    pub ssa_env: HashMap<String, IrValue>,
    /// Tracks SSA values at the end of each block for phi nodes
    pub block_exit_values: HashMap<String, HashMap<String, IrValue>>,
    pub unsigned_vars: HashSet<String>,
    /// Name of the function currently being lowered; included in error messages.
    pub current_function: Option<String>,
    /// Span of the statement being lowered, attached to `error`/`warn` so
    /// lowering diagnostics point at a source line. Set by the compiler.
    pub pending_span: Option<Span>,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            types: TypeContext::new(),
            diagnostics: Diagnostics::new(),
            ssa_env: HashMap::new(),
            block_exit_values: HashMap::new(),
            unsigned_vars: HashSet::new(),
            current_function: None,
            pending_span: None,
        }
    }

    pub fn reset_for_program(&mut self) {
        self.symbols.clear();
        self.types.clear();
        self.diagnostics.clear();
        self.ssa_env.clear();
        self.block_exit_values.clear();
        self.unsigned_vars.clear();
        self.current_function = None;
        self.pending_span = None;
    }

    pub fn begin_function(&mut self, name: &str) {
        self.symbols.enter_scope();
        self.unsigned_vars.clear();
        self.current_function = Some(name.to_owned());
    }

    pub fn end_function(&mut self) {
        self.symbols.exit_scope();
        self.current_function = None;
    }

    /// Message prefixed with the current function name when one is set.
    fn scoped(&self, message: impl Into<String>) -> String {
        let msg = message.into();
        match &self.current_function {
            Some(fn_name) => format!("in function '{fn_name}': {msg}"),
            None => msg,
        }
    }

    /// Emit a diagnostic error at the pending statement span when one is set.
    pub fn error(&mut self, message: impl Into<String>) {
        let full = self.scoped(message);
        match self.pending_span.clone() {
            Some(span) => self.diagnostics.error_at(span, full),
            None => self.diagnostics.error(full),
        }
    }

    /// Emit an error with the caret narrowed to `token` within the pending span.
    pub fn error_at_token(&mut self, token: &str, message: impl Into<String>) {
        let full = self.scoped(message);
        match self.pending_span.as_ref().map(|s| s.narrowed_to(token)) {
            Some(span) => self.diagnostics.error_at(span, full),
            None => self.diagnostics.error(full),
        }
    }

    /// Emit a diagnostic warning at the pending statement span when one is set.
    pub fn warn(&mut self, message: impl Into<String>) {
        let full = self.scoped(message);
        match self.pending_span.clone() {
            Some(span) => self.diagnostics.warn_at(span, full),
            None => self.diagnostics.warn(full),
        }
    }

    pub fn save_block_exit_values(&mut self, label: IrLabel) {
        self.block_exit_values
            .insert(label.0.clone(), self.ssa_env.clone());
    }

    pub fn snapshot_env(&self) -> HashMap<String, IrValue> {
        self.ssa_env.clone()
    }

    pub fn restore_env(&mut self, snapshot: HashMap<String, IrValue>) {
        self.ssa_env = snapshot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_attaches_the_pending_span_and_function_scope() {
        let mut ctx = LoweringContext::new();
        ctx.current_function = Some("main".to_owned());
        ctx.pending_span = Some(Span {
            line: 4,
            col: 5,
            len: 3,
            source_line: "    bad".to_owned(),
        });
        ctx.error("boom");
        let diag = &ctx.diagnostics.entries()[0];
        assert_eq!(diag.span.as_ref().map(|s| s.line), Some(4));
        assert!(diag.message.contains("in function 'main': boom"));
    }

    #[test]
    fn error_without_a_pending_span_stays_unspanned() {
        let mut ctx = LoweringContext::new();
        ctx.error("no location");
        assert!(ctx.diagnostics.entries()[0].span.is_none());
    }
}
