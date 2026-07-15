//! String interpolation runtime and `string.format_into` end-to-end tests.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::TargetMode;
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn cached_stdlib_objs() -> &'static [(String, AssembledOutput)] {
    static STDLIB: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    STDLIB.get_or_init(|| {
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
            .expect("stdlib compile failed")
    })
}

// Silences the unused-import warning some sibling modules trigger; the prelude
// getters stay exercised so their symbols cannot rot silently.
#[test]
fn stdlib_preludes_resolve() {
    assert!(!get_stdlib_modules_for_mode(TargetMode::Hosted).is_empty());
    assert!(!get_stdlib_type_prelude().is_empty());
}

fn run_hll(src: &str, max_steps: u64) -> (StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline
        .assemble(&user_tokens)
        .expect("user assemble failed");

    let mut modules: Vec<(&str, &AssembledOutput)> = cached_stdlib_objs()
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(max_steps);
    (run.outcome, run.uart_output)
}

fn assert_exit_zero_with_uart(src: &str, expected_uart: &str) {
    let (outcome, uart) = run_hll(src, 50_000_000);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "program did not exit 0 ({outcome:?}); uart:\n{uart}"
    );
    assert_eq!(uart, expected_uart, "unexpected uart output");
}

#[test]
fn format_runtime_helpers_render_primitives() {
    let src = r#"
console := import("console")
string := import("string")

emit: (buf: u8[], n: u64) {
    console.print(string.fmt_subslice(buf, 0, n))
    console.println("")
}

main: () -> i32 {
    buf: u8[64] = []
    b: u8[] = buf[..]
    emit(b, string.fmt_write_i64(b, 0, 0))
    emit(b, string.fmt_write_i64(b, 0, -42))
    umax: u64 = 0 - 1
    emit(b, string.fmt_write_u64(b, 0, umax))
    emit(b, string.fmt_write_bool(b, 0, true))
    emit(b, string.fmt_write_bool(b, 0, false))
    emit(b, string.fmt_write_bytes(b, 0, "hello"))
    emit(b, string.fmt_write_f64(b, 0, 0.1))
    emit(b, string.fmt_write_f64(b, 0, 1.5))
    emit(b, string.fmt_write_f64(b, 0, 42.0))
    emit(b, string.fmt_write_f64(b, 0, -2.5))
    emit(b, string.fmt_write_f64(b, 0, 0.0))
    one: f64 = 1.0
    zero: f64 = 0.0
    emit(b, string.fmt_write_f64(b, 0, one / zero))
    emit(b, string.fmt_write_f64(b, 0, zero / zero))
    return 0
}
"#;
    let expected = "0\n-42\n18446744073709551615\ntrue\nfalse\nhello\n\
                    0.10000000000000001\n1.5\n42\n-2.5\n0\ninf\nnan\n";
    assert_exit_zero_with_uart(src, expected);
}

// Regression: a slice-returning extern call passed directly as a call argument
// lost its aggregate return ABI; IR now carries non-scalar extern signatures.
#[test]
fn slice_returning_extern_call_as_argument() {
    let src = r#"
console := import("console")
string := import("string")

main: () -> i32 {
    console.print(string.fmt_subslice("hello", 0, 4))
    console.println("")
    return 0
}
"#;
    assert_exit_zero_with_uart(src, "hell\n");
}

// Regression: fld/fsd (and lw/lh/lb/flw/fsw) skipped memory-offset
// legalization, so an f64 parameter spilled beyond +2047 bytes read garbage.
#[test]
fn f64_param_survives_large_frame() {
    let src = r#"
console := import("console")

big_frame: (value: f64) -> u64 {
    pad: u64[500] = []
    pad[0] = 1
    threshold: f64 = 1.0
    if value > threshold {
        return pad[0] + 41
    }
    return 0
}

main: () -> i32 {
    console.print_int(big_frame(1.5) as i64)
    console.println("")
    return 0
}
"#;
    assert_exit_zero_with_uart(src, "42\n");
}

#[test]
fn format_into_renders_primitives_and_braces() {
    let src = r#"
console := import("console")
string := import("string")

main: () -> i32 {
    buf: u8[128] = []
    count: i64 = -3
    name: u8[] = "cube"
    ratio: f64 = 1.5
    flag: bool = true
    written := string.format_into(buf[..], "{{{name}}} n={count} r={ratio} ok={flag}")
    match written {
        Ok(n) -> {
            console.print(string.fmt_subslice(buf[..], 0, n))
            console.println("")
            return 0
        }
        Err(_) -> {
            return 1
        }
    }
    return 2
}
"#;
    assert_exit_zero_with_uart(src, "{cube} n=-3 r=1.5 ok=true\n");
}

#[test]
fn format_into_undersized_buffer_is_untouched() {
    let src = r#"
console := import("console")
string := import("string")

main: () -> i32 {
    buf: u8[4] = []
    buf[0] = 'a' as u8
    buf[1] = 'b' as u8
    buf[2] = 'c' as u8
    buf[3] = 'd' as u8
    x: i64 = 12345
    written := string.format_into(buf[..], "value={x}")
    match written {
        Ok(_) -> {
            return 1
        }
        Err(e) -> {
            match e {
                FormatBufferTooSmall(required) -> {
                    ; required must be the complete byte count: "value=12345"
                    if required != 11 { return 2 }
                }
                FormatSizeOverflow -> {
                    return 3
                }
            }
        }
    }
    ; the destination must be byte-for-byte unchanged
    console.print(buf[0..4])
    console.println("")
    return 0
}
"#;
    assert_exit_zero_with_uart(src, "abcd\n");
}

#[test]
fn format_into_evaluates_left_to_right_once() {
    let src = r#"
console := import("console")
string := import("string")

counter: i64 = 0

next: () -> i64 {
    counter = counter + 1
    return counter
}

init_counter: () {
    counter = 0
}

main: () -> i32 {
    init_counter()
    buf: u8[64] = []
    written := string.format_into(buf[..], "{next()} {next()} {next()}")
    match written {
        Ok(n) -> {
            console.print(string.fmt_subslice(buf[..], 0, n))
            console.println("")
            return 0
        }
        Err(_) -> {
            return 1
        }
    }
    return 2
}
"#;
    assert_exit_zero_with_uart(src, "1 2 3\n");
}

#[test]
fn format_into_calls_custom_format_impl() {
    let src = r#"
console := import("console")
string := import("string")

struct Point { x: i64, y: i64 }

impl Point {
    formatted_len: (self: Point*) -> Result<u64, FormatError> {
        ; "(x, y)" = x digits + y digits + 4 punctuation bytes
        return Ok(string.fmt_len_i64(self.x) + string.fmt_len_i64(self.y) + 4)
    }
    format_exact_into: (self: Point*, destination: u8[]) -> u64 {
        at: u64 = 0
        destination[at] = '(' as u8
        at = string.fmt_write_i64(destination, at + 1, self.x)
        destination[at] = ',' as u8
        destination[at + 1] = ' ' as u8
        at = string.fmt_write_i64(destination, at + 2, self.y)
        destination[at] = ')' as u8
        return at + 1
    }
}

main: () -> i32 {
    buf: u8[64] = []
    p: Point = Point { .x = 3, .y = -14 }
    written := string.format_into(buf[..], "p={p}")
    match written {
        Ok(n) -> {
            console.print(string.fmt_subslice(buf[..], 0, n))
            console.println("")
            return 0
        }
        Err(_) -> {
            return 1
        }
    }
    return 2
}
"#;
    assert_exit_zero_with_uart(src, "p=(3, -14)\n");
}

#[test]
fn format_into_rejects_bad_templates() {
    for (template, needle) in [
        ("\"unmatched }\"", "unmatched `}`"),
        ("\"open {\"", "unterminated `{`"),
        ("\"{}\"", "empty format string interpolation"),
        ("\"{ {x} }\"", "nested `{`"),
    ] {
        let src = format!(
            r#"
string := import("string")

main: () -> i32 {{
    buf: u8[8] = []
    x: i64 = 1
    written := string.format_into(buf[..], {template})
    return 0
}}
"#
        );
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_write_artifacts(false);
        let err = pipeline
            .compile(&src)
            .expect_err("template must be rejected");
        let text = format!("{err:?}");
        assert!(
            text.contains(needle),
            "expected `{needle}` for {template}, got: {text}"
        );
    }
}

#[test]
fn format_runtime_lengths_agree_with_writes() {
    let src = r#"
string := import("string")

main: () -> i32 {
    buf: u8[64] = []
    b: u8[] = buf[..]
    if string.fmt_write_i64(b, 0, -12345) != string.fmt_len_i64(-12345) { return 1 }
    umax: u64 = 0 - 1
    if string.fmt_write_u64(b, 0, umax) != string.fmt_len_u64(umax) { return 2 }
    if string.fmt_write_bool(b, 0, true) != string.fmt_len_bool(true) { return 3 }
    if string.fmt_write_bool(b, 0, false) != string.fmt_len_bool(false) { return 4 }
    if string.fmt_write_f64(b, 0, 0.1) != string.fmt_len_f64(0.1) { return 5 }
    if string.fmt_write_f64(b, 0, -2.5) != string.fmt_len_f64(-2.5) { return 6 }
    if string.fmt_write_bytes(b, 0, "abc") != string.fmt_len_str("abc") { return 7 }
    ; saturating accumulator: overflow is sticky and detectable
    big: u64 = 0 - 2
    sat: u64 = string.fmt_len_add(big, 5)
    if string.fmt_is_len_overflow(sat) == false { return 8 }
    if string.fmt_is_len_overflow(string.fmt_len_add(3, 4)) { return 9 }
    return 0
}
"#;
    let (outcome, uart) = run_hll(src, 50_000_000);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "helper mismatch ({outcome:?}); uart:\n{uart}"
    );
}
