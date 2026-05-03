use crate::high_level_language::compiler::utility::diagnostics::Diagnostics;
use crate::high_level_language::compiler::utility::symbol_table::SymbolTable;
use crate::high_level_language::compiler::utility::type_context::TypeContext;
use crate::intermediate_language::values::{IrLabel, IrValue};
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
    /// Name of the function currently being lowered -- included in error messages.
    pub current_function: Option<String>,
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

    /// Emit a diagnostic error, prefixed with the current function name when available.
    pub fn error(&mut self, message: impl Into<String>) {
        let msg = message.into();
        let full = match &self.current_function {
            Some(fn_name) => format!("in function '{fn_name}': {msg}"),
            None => msg,
        };
        self.diagnostics.error(full);
    }

    /// Emit a diagnostic warning, prefixed with the current function name when available.
    pub fn warn(&mut self, message: impl Into<String>) {
        let msg = message.into();
        let full = match &self.current_function {
            Some(fn_name) => format!("in function '{fn_name}': {msg}"),
            None => msg,
        };
        self.diagnostics.warn(full);
    }

    pub fn save_block_exit_values(&mut self, label: IrLabel) {
        self.block_exit_values
            .insert(label.0.clone(), self.ssa_env.clone());
    }

    pub fn get_block_exit_values(&self, label: &IrLabel) -> Option<HashMap<String, IrValue>> {
        self.block_exit_values.get(&label.0).cloned()
    }

    pub fn snapshot_env(&self) -> HashMap<String, IrValue> {
        self.ssa_env.clone()
    }

    pub fn restore_env(&mut self, snapshot: HashMap<String, IrValue>) {
        self.ssa_env = snapshot;
    }
}
