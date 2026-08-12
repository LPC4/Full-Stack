use super::*;
use egui_kittest::{Harness, kittest::Queryable as _};

#[test]
fn enclosing_function_finds_the_header_above_the_caret() {
    let src = "PI: i32 = 3\n\
                   main: () -> i32 {\n    return helper(1)\n}\n\
                   helper<T>: (x: T) -> T {\n    return x\n}\n";
    assert_eq!(enclosing_function(src, 3).as_deref(), Some("main"));
    assert_eq!(enclosing_function(src, 6).as_deref(), Some("helper"));
    assert_eq!(enclosing_function(src, 1), None, "consts are not functions");
    assert_eq!(enclosing_function("", 5), None);
}

#[test]
fn locate_in_stage_finds_ir_defines_and_asm_labels() {
    let ir = "export define main() -> i32 {\nentry:\n}\n\ndefine helper($x: i64) -> i64 {\n";
    assert_eq!(locate_in_stage(ir, Layer::Ir, "main"), Some(0));
    let helper = locate_in_stage(ir, Layer::Ir, "helper").expect("helper found");
    assert!(ir[helper..].starts_with("define helper"));
    assert_eq!(locate_in_stage(ir, Layer::Ir, "absent"), None);

    let asm = ".globl main\nmain:\n    addi sp, sp, -16\nhelper:\n    ret\n";
    let main = locate_in_stage(asm, Layer::Asm, "main").expect("label found");
    assert!(asm[main..].starts_with("main:"));
    assert_eq!(locate_in_stage(asm, Layer::Asm, "elper"), None);
}

#[test]
fn framebuffer_key_mapping_matches_vm_contract() {
    assert_eq!(printable_scancode('A'), Some(0x41));
    assert_eq!(special_scancode(egui::Key::ArrowUp), Some(0x80));
    assert_eq!(special_scancode(egui::Key::ArrowDown), Some(0x81));
    assert_eq!(special_scancode(egui::Key::ArrowLeft), Some(0x82));
    assert_eq!(special_scancode(egui::Key::ArrowRight), Some(0x83));
    assert_eq!(key_scancode(egui::Key::W), Some(b'W' as u16));
    assert_eq!(key_scancode(egui::Key::A), Some(b'A' as u16));
    assert_eq!(key_scancode(egui::Key::S), Some(b'S' as u16));
    assert_eq!(key_scancode(egui::Key::D), Some(b'D' as u16));
    assert_eq!(key_scancode(egui::Key::Space), Some(b' ' as u16));
}

#[test]
fn framebuffer_fit_preserves_aspect_inside_any_window() {
    for available in [vec2(1000.0, 600.0), vec2(800.0, 1000.0), vec2(240.0, 180.0)] {
        let fitted = fit_framebuffer_size(available);
        assert!(fitted.x <= available.x);
        assert!(fitted.y <= available.y);
        assert!((fitted.x / fitted.y - 4.0 / 3.0).abs() < f32::EPSILON);
    }
    assert_eq!(fit_framebuffer_size(vec2(0.0, 180.0)), egui::Vec2::ZERO);
}

#[test]
fn ctrl_key_mapping_mirrors_uart_control_bytes() {
    assert_eq!(ctrl_scancode(egui::Key::A), Some(0x01));
    assert_eq!(ctrl_scancode(egui::Key::C), Some(0x03));
    assert_eq!(ctrl_scancode(egui::Key::Z), Some(0x1A));
    assert_eq!(ctrl_scancode(egui::Key::ArrowUp), None);
}

// Key alone, no Text event: that is what the web backend sends for Tab, and
// what a TextEdit without lock_focus filters away before it can indent.
#[test]
fn tab_indents_the_code_editor_rather_than_moving_focus() {
    let mut harness = code_tab_harness("ret\n");
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    harness.run();
    for _ in 0..4 {
        harness.key_press(egui::Key::ArrowLeft); // cursor starts at the end
    }
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();

    assert_eq!(
        harness.state(),
        "    ret\n",
        "Tab should indent with spaces, not move focus: {:?}",
        harness.state()
    );
    assert!(
        !harness.state().contains('\t'),
        "Tab must insert spaces, not tab characters: {:?}",
        harness.state()
    );
}

#[test]
fn tab_aligns_the_cursor_to_the_next_multiple_of_four() {
    let mut harness = code_tab_harness(" ret\n");
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    harness.run();
    for _ in 0..4 {
        harness.key_press(egui::Key::ArrowLeft); // cursor after the space
    }
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();

    assert_eq!(
        harness.state(),
        "    ret\n",
        "Tab should pad only up to the next multiple of four: {:?}",
        harness.state()
    );
}

#[test]
fn shift_tab_dedents_to_the_previous_multiple_of_four() {
    let mut harness = code_tab_harness("     ret\n"); // five leading spaces
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    harness.run();
    for _ in 0..9 {
        harness.key_press(egui::Key::ArrowLeft); // cursor at the line start
    }
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run();

    assert_eq!(
        harness.state(),
        "    ret\n",
        "Shift+Tab should drop five spaces back to four: {:?}",
        harness.state()
    );
}

#[test]
fn tab_and_shift_tab_apply_to_every_line_in_a_selection() {
    let mut harness = code_tab_harness("a\nb\nc\n");
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    harness.run();
    // Select the last two lines, then indent and dedent them as a block.
    for _ in 0..4 {
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    }
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(
        harness.state(),
        "a\n    b\n    c\n",
        "Tab should indent every selected line: {:?}",
        harness.state()
    );
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run();
    assert_eq!(
        harness.state(),
        "a\nb\nc\n",
        "Shift+Tab should dedent every selected line: {:?}",
        harness.state()
    );
}

#[test]
fn backspace_removes_one_space_not_a_tab_stop() {
    let mut harness = code_tab_harness("    ret\n");
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    harness.run();
    for _ in 0..4 {
        harness.key_press(egui::Key::ArrowLeft); // cursor after the spaces
    }
    harness.run();
    harness.key_press(egui::Key::Backspace);
    harness.run();

    assert_eq!(
        harness.state(),
        "   ret\n",
        "Backspace should delete a single space: {:?}",
        harness.state()
    );
}

fn code_tab_harness(text: &str) -> Harness<'static, String> {
    Harness::new_ui_state(
        |ui, state: &mut String| {
            let text = state.clone();
            let out = code_view(
                ui,
                CodeViewConfig {
                    id_salt: "tab_indent",
                    text: &text,
                    highlighter: None,
                    interactive: true,
                    show_current_line: false,
                    find: None,
                    marks: &[],
                    jump_to: None,
                    debug: &SourceDebug::default(),
                },
            );
            if let Some(edited) = out.edited {
                *state = edited;
            }
        },
        text.to_owned(),
    )
}
