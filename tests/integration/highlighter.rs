use full_stack::view::highlight_assembly;

fn style() -> egui::Style {
    egui::Style::default()
}

// The core regression: multi-byte UTF-8 chars inside .asciz string literals
// used to cause a byte-boundary panic in the fallthrough branch of the
// character-by-character loop in highlight_assembly.
#[test]
fn highlight_assembly_multibyte_in_asciz_does_not_panic() {
    let asm = "    .asciz \"single hart \u{2014} no SMP\"\n"; // em-dash U+2014
    let _ = highlight_assembly(&style(), asm);
}

#[test]
fn highlight_assembly_multibyte_in_ascii_does_not_panic() {
    let asm = "    .ascii \"caf\u{00E9}\"\n"; //  U+00E9 (2 bytes)
    let _ = highlight_assembly(&style(), asm);
}

#[test]
fn highlight_assembly_multibyte_cjk_does_not_panic() {
    let asm = "    .asciz \"\u{4E2D}\u{6587}\"\n"; // CJK (3-byte each)
    let _ = highlight_assembly(&style(), asm);
}

#[test]
fn highlight_assembly_multibyte_emoji_does_not_panic() {
    let asm = "    .asciz \"\u{1F600}\"\n"; // grinning face (4 bytes)
    let _ = highlight_assembly(&style(), asm);
}

// Sanity: plain ASCII still produces output with the right number of sections.
#[test]
fn highlight_assembly_ascii_produces_output() {
    let asm = "main:\n    addi a0, zero, 42\n    ret\n";
    let job = highlight_assembly(&style(), asm);
    assert!(!job.sections.is_empty());
}

// Multi-byte char at the very start of the code (not inside a string).
#[test]
fn highlight_assembly_multibyte_at_start_does_not_panic() {
    let asm = "\u{2014} comment-like junk\n    ret\n";
    let _ = highlight_assembly(&style(), asm);
}
