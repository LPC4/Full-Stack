use crate::high_level_language::compiler::diagnostics::Diagnostics;
use crate::high_level_language::compiler::symbol_table::SymbolTable;
use crate::high_level_language::compiler::type_context::TypeContext;

#[derive(Debug, Default)]
pub struct LoweringContext {
    pub symbols: SymbolTable,
    pub types: TypeContext,
    pub diagnostics: Diagnostics,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            types: TypeContext::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn reset_for_program(&mut self) {
        self.symbols.clear();
        self.types.clear();
        self.diagnostics.clear();
    }

    pub fn begin_function(&mut self) {
        self.symbols.enter_scope();
    }

    pub fn end_function(&mut self) {
        self.symbols.exit_scope();
    }
}
