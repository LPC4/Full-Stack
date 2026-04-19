use crate::high_level_language::compiler::utility::diagnostics::Diagnostics;
use crate::high_level_language::compiler::utility::symbol_table::SymbolTable;
use crate::high_level_language::compiler::utility::type_context::TypeContext;
use crate::intermediate_language::values::{IrLabel, IrValue};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct LoweringContext {
    pub symbols: SymbolTable,
    pub types: TypeContext,
    pub diagnostics: Diagnostics,
    pub ssa_env: HashMap<String, IrValue>,
    /// Tracks SSA values at the end of each block for phi reconciliation
    pub block_exit_values: HashMap<String, HashMap<String, IrValue>>,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            types: TypeContext::new(),
            diagnostics: Diagnostics::new(),
            ssa_env: HashMap::new(),
            block_exit_values: HashMap::new(),
        }
    }

    pub fn reset_for_program(&mut self) {
        self.symbols.clear();
        self.types.clear();
        self.diagnostics.clear();
        self.ssa_env.clear();
        self.block_exit_values.clear();
    }

    pub fn begin_function(&mut self) {
        self.symbols.enter_scope();
    }

    pub fn end_function(&mut self) {
        self.symbols.exit_scope();
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
