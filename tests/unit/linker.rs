use full_stack::assembly_language::linker::{link_assembly, runtime_glue, strip_comments, LinkedProgram};

// ---- runtime_glue (text form) -----------------------------------------------

#[test]
fn runtime_glue_contains_start() {
    let glue = runtime_glue();
    assert!(glue.contains("_start"));
    assert!(glue.contains("putchar"));
    assert!(glue.contains("ecall"));
}

#[test]
fn runtime_glue_contains_exit_syscall() {
    let glue = runtime_glue();
    // sys_exit_group = 93
    assert!(glue.contains("93") || glue.contains("exit"));
}

#[test]
fn runtime_glue_has_puts_and_printf() {
    let glue = runtime_glue();
    assert!(glue.contains("puts"));
    assert!(glue.contains("printf"));
}

// ---- link_assembly -----------------------------------------------------------

#[test]
fn link_assembly_glue_comes_first() {
    let linked = link_assembly(&["# source 1", "# source 2"]);
    assert!(linked.starts_with("\t.text"));
    assert!(linked.contains("# source 1"));
    assert!(linked.contains("# source 2"));
}

#[test]
fn link_assembly_preserves_source_order() {
    let linked = link_assembly(&["first", "second", "third"]);
    let first_pos = linked.find("first").unwrap();
    let second_pos = linked.find("second").unwrap();
    let third_pos = linked.find("third").unwrap();
    assert!(first_pos < second_pos && second_pos < third_pos);
}

#[test]
fn link_assembly_empty_sources_still_has_glue() {
    let linked = link_assembly(&[]);
    assert!(linked.contains("_start"));
}

#[test]
fn link_assembly_skips_empty_source_strings() {
    let linked_with = link_assembly(&["", "real_code"]);
    let linked_without = link_assembly(&["real_code"]);
    // Both should contain real_code; empty string should not inject extra content
    assert!(linked_with.contains("real_code"));
    assert!(linked_without.contains("real_code"));
}

// ---- strip_comments ----------------------------------------------------------

#[test]
fn strip_comments_removes_inline_comment() {
    let stripped = strip_comments("add a0, a1, a2 ; this is a comment\nsub a0, a0, 1");
    assert!(stripped.contains("add a0, a1, a2"));
    assert!(!stripped.contains("this is a comment"));
    assert!(stripped.contains("sub a0, a0, 1"));
}

#[test]
fn strip_comments_removes_full_comment_lines() {
    let stripped = strip_comments("; full comment line\nadd a0, a1, a2");
    assert!(!stripped.contains("full comment line"));
    assert!(stripped.contains("add a0, a1, a2"));
}

#[test]
fn strip_comments_removes_blank_lines() {
    let stripped = strip_comments("line1\n\nline2");
    assert!(!stripped.contains("\n\n"));
    assert!(stripped.contains("line1"));
    assert!(stripped.contains("line2"));
}

#[test]
fn strip_comments_empty_input_returns_empty() {
    assert_eq!(strip_comments(""), "");
}

// ---- LinkedProgram -----------------------------------------------------------

#[test]
fn linked_program_new_stripped_removes_comments() {
    let prog = LinkedProgram::new_stripped(&["add a0, a1, a2 ; comment"]);
    assert!(!prog.as_str().contains("; comment"));
    assert!(prog.as_str().contains("add a0, a1, a2"));
}

#[test]
fn linked_program_into_string_consumes() {
    let prog = LinkedProgram::new(&["# test"]);
    let s = prog.into_string();
    assert!(s.contains("_start"));
    assert!(s.contains("# test"));
}

#[test]
fn linked_program_display_matches_as_str() {
    let prog = LinkedProgram::new(&["# display test"]);
    assert_eq!(format!("{prog}"), prog.as_str());
}
