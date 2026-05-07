use full_stack::assembly_language::assembler::symbol_table::SymbolTable;

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
    // Original value is preserved
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
    st.mark_global("foo"); // second call should not duplicate
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
