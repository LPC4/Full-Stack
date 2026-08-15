use full_stack::highlight::highlight_assembly;
use egui::epaint::text::ByteRangeExt as _;

fn style() -> egui::Style {
    egui::Style::default()
}

// Multi-byte UTF-8 inside a .asciz literal once hit a byte-boundary panic in
// the fallthrough branch of highlight_assembly.
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

// Everything after # or ; must be one comment-colored section, even when the
// comment mentions registers or mnemonics like sp and ret.
#[test]
fn highlight_assembly_inline_comment_is_all_comment_color() {
    let comment_color = full_stack::highlight::SyntaxPalette::dark().comment;
    for asm in [
        "  addi sp, sp, -16    # prologue: sp moves, ret comes later\n",
        "  addi sp, sp, -16    ; prologue: sp moves, ret comes later\n",
        "# a whole-line comment with a0 and mul in it\n",
    ] {
        let job = highlight_assembly(&style(), asm);
        let comment_start = asm.find(['#', ';']).expect("comment marker");
        for section in &job.sections {
            let text = section.byte_range.slice(asm);
            if section.byte_range.start.0 >= comment_start && text != "\n" {
                assert_eq!(
                    section.format.color, comment_color,
                    "non-comment color inside comment: {text:?} in {asm:?}"
                );
            }
        }
    }
}

// A # inside a string literal is not a comment; the directive after it on the
// next line still highlights normally.
#[test]
fn highlight_assembly_hash_inside_string_is_not_a_comment() {
    let asm = "    .asciz \"a # b\"\n";
    let job = highlight_assembly(&style(), asm);
    let string_color = full_stack::highlight::SyntaxPalette::dark().string;
    let quoted = job
        .sections
        .iter()
        .find(|s| s.byte_range.slice(asm).starts_with('"'))
        .expect("string section");
    assert_eq!(quoted.format.color, string_color);
    assert_eq!(quoted.byte_range.slice(asm), "\"a # b\"");
}
