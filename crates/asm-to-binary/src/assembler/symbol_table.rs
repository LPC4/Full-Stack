use std::collections::HashMap;

/// Tracks label addresses accumulated across all sections during Pass 1.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// label -> absolute byte address (section-base + section-offset).
    symbols: HashMap<String, u64>,
    /// Labels marked `.globl` -- exported for the linker.
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

#[cfg(test)]
mod tests {
    use super::SymbolTable;

    #[test]
    fn define_and_resolve_label() {
        let mut st = SymbolTable::new();
        assert!(st.define("foo", 0x100));
        assert_eq!(st.resolve("foo"), Some(0x100));
    }

    #[test]
    fn duplicate_define_returns_false() {
        let mut st = SymbolTable::new();
        assert!(st.define("foo", 0x100));
        assert!(!st.define("foo", 0x200));
        assert_eq!(st.resolve("foo"), Some(0x100));
    }

    #[test]
    fn resolve_undefined_returns_none() {
        let st = SymbolTable::new();
        assert_eq!(st.resolve("missing"), None);
    }

    #[test]
    fn mark_global_and_query() {
        let mut st = SymbolTable::new();
        st.define("main", 0);
        st.mark_global("main");
        assert!(st.globals().contains(&"main".to_owned()));
    }

    #[test]
    fn globals_no_duplicates() {
        let mut st = SymbolTable::new();
        st.define("foo", 0);
        st.mark_global("foo");
        st.mark_global("foo");
        assert_eq!(st.globals().iter().filter(|g| *g == "foo").count(), 1);
    }

    #[test]
    fn all_returns_all_defined_labels() {
        let mut st = SymbolTable::new();
        st.define("a", 0);
        st.define("b", 4);
        st.define("c", 8);
        let all = st.all();
        assert_eq!(all.len(), 3);
        assert_eq!(all.get("a"), Some(&0));
        assert_eq!(all.get("b"), Some(&4));
        assert_eq!(all.get("c"), Some(&8));
    }

    #[test]
    fn unmarked_symbol_not_in_globals() {
        let mut st = SymbolTable::new();
        st.define("local_label", 42);
        assert!(!st.globals().contains(&"local_label".to_owned()));
    }

    #[test]
    fn define_zero_address() {
        let mut st = SymbolTable::new();
        assert!(st.define("entry", 0));
        assert_eq!(st.resolve("entry"), Some(0));
    }
}
