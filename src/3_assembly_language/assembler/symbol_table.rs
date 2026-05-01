use std::collections::HashMap;

/// Tracks label addresses accumulated across all sections during Pass 1.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// label → absolute byte address (section-base + section-offset).
    symbols: HashMap<String, u64>,
    /// Labels marked `.globl` — exported for the linker.
    globals: Vec<String>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Define a label at the given address. Returns `false` if it was already defined.
    pub fn define(&mut self, name: impl Into<String>, address: u64) -> bool {
        let name = name.into();
        if self.symbols.contains_key(&name) {
            return false;
        }
        self.symbols.insert(name, address);
        true
    }

    pub fn resolve(&self, name: &str) -> Option<u64> {
        self.symbols.get(name).copied()
    }

    pub fn mark_global(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.globals.contains(&name) {
            self.globals.push(name);
        }
    }

    pub fn globals(&self) -> &[String] {
        &self.globals
    }

    pub fn all(&self) -> &HashMap<String, u64> {
        &self.symbols
    }
}
