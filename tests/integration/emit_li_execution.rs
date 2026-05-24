/// Integration tests for `emit_li` correctness verified by VM execution.
///
/// Each test compiles an HLL program whose `main` returns 0 (pass) or 1 (fail)
/// based on whether the loaded constant matches the expected value.  Using
/// arithmetic to build the expected value from safe (<= 2047) operands ensures
/// the comparison target is not affected by the same bug under test.
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn run_hll(src: &str) -> (VirtualMachine, StepOutcome, String) {
    let pipeline = CompilationPipeline::new();
    let stdlib_result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile failed");
    let (_, stdlib_tokens) =
        pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);
    let assembled = pipeline.assemble(&linked).expect("assemble failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

/// 42 - exercises the ADDI-only path (value in [-2048, 2047]).
#[test]
fn li_small_positive() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i32 = 42
    if v == 42 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 42 should load 42, got {outcome:?}"
    );
}

/// 2047 - last value that fits in a single ADDI.
#[test]
fn li_boundary_2047() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i32 = 2047
    if v == 2047 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 2047 should load 2047, got {outcome:?}"
    );
}

/// 2048 - first value that requires the LUI path.
#[test]
fn li_boundary_2048() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i32 = 2048
    if v == 2048 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 2048 should load 2048 (LUI+ADDI path), got {outcome:?}"
    );
}

/// 0x7FFF_FFFF - last value before the sign-extension danger zone.
/// LUI + ADDI, bit 31 clear -> no zero-extension needed.
#[test]
fn li_max_signed_32bit() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i32 = 2147483647
    a: i32 = 1073741823
    expected: i32 = a + a + 1
    if v == expected {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 0x7FFF_FFFF should load 2147483647, got {outcome:?}"
    );
}

/// 0x8000_0000 - first value where LUI sign-extends; zero-extension (slli/srli) is required.
/// Expected value built as 1048576 * 2048 = 2147483648.  Both factors are in the safe range
/// (bit 31 clear) so they are loaded correctly regardless of the zero-extension bug.
#[test]
fn li_sign_extend_boundary() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i64 = 2147483648
    a: i64 = 1048576
    b: i64 = 2048
    expected: i64 = a * b
    if v == expected {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 0x8000_0000 should load 2147483648 (zero-extended), got {outcome:?}"
    );
}

/// 0x8010_0000 - the exact pmm_init address that exposed the original kernel bug.
/// Expected value built as 1048576 * 2049 = 2148532224.
#[test]
fn li_original_bug_value() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i64 = 2148532224
    a: i64 = 1048576
    expected: i64 = a * 2049
    if v == expected {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 0x8010_0000 should load 2148532224, got {outcome:?}"
    );
}

/// 0xFFFF_FFFF - hi_adj overflows i32; the slli/srli sequence must still produce the right value.
/// Expected value built as 65535 * 65537 = 65536^2 - 1 = 4294967295.
#[test]
fn li_max_unsigned_32bit() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i64 = 4294967295
    a: i64 = 65535
    b: i64 = 65537
    expected: i64 = a * b
    if v == expected {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 0xFFFF_FFFF should load 4294967295, got {outcome:?}"
    );
}

/// 0x1_0000_0000 - first true 64-bit value; previously produced 0 when the else branch was missing.
/// Expected value built as 65536 * 65536 = 4294967296.
#[test]
fn li_true_64bit_small() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    v: i64 = 4294967296
    a: i64 = 65536
    expected: i64 = a * a
    if v == expected {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "li 0x1_0000_0000 should load 4294967296 (true 64-bit path), got {outcome:?}"
    );
}
