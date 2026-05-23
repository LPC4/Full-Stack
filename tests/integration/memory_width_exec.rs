/// Integration tests for load/store instruction width selection, verified by VM execution.
///
/// Each test exercises a specific memory width (i8/i16/i32/i64) through heap-allocated
/// pointers, confirming that the assembler emits the correct load/store opcode and that
/// sign/zero behaviour is correct at each width.
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

/// Store and load an i8: positive value 127 round-trips without corruption.
#[test]
fn mem_i8_store_load_positive() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i8*) -> void

main: () -> i32 {
    p: i8* = new(i8)
    @p = 127
    v: i8 = @p
    free(p)
    if v == 127 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "i8 store 127 → load should read 127, got {outcome:?}"
    );
}

/// Store and load an i8: negative value -1 (0xFF) round-trips as -1 (lb sign-extends).
#[test]
fn mem_i8_store_load_negative() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i8*) -> void

main: () -> i32 {
    p: i8* = new(i8)
    @p = -1
    v: i8 = @p
    free(p)
    if v == -1 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "i8 store -1 → load should read -1 (sign-extended), got {outcome:?}"
    );
}

/// Store and load an i16: value 1000 round-trips correctly.
#[test]
fn mem_i16_store_load() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i16*) -> void

main: () -> i32 {
    p: i16* = new(i16)
    @p = 1000
    v: i16 = @p
    free(p)
    if v == 1000 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "i16 store 1000 → load should read 1000, got {outcome:?}"
    );
}

/// Store and load an i32: value 1234567 round-trips correctly.
#[test]
fn mem_i32_store_load() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i32*) -> void

main: () -> i32 {
    p: i32* = new(i32)
    @p = 1234567
    v: i32 = @p
    free(p)
    if v == 1234567 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "i32 store 1234567 → load should read 1234567, got {outcome:?}"
    );
}

/// Store and load an i64: large value 0x8010_0000 round-trips without sign-corruption.
/// Expected value built from safe arithmetic as 1048576 * 2049.
#[test]
fn mem_i64_store_load_large() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i64*) -> void

main: () -> i32 {
    p: i64* = new(i64)
    @p = 2148532224
    v: i64 = @p
    free(p)
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
        "i64 store 0x8010_0000 → load should read same value, got {outcome:?}"
    );
}

/// Store then overwrite: last written value wins.
#[test]
fn mem_i32_overwrite() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i32*) -> void

main: () -> i32 {
    p: i32* = new(i32)
    @p = 10
    @p = 42
    v: i32 = @p
    free(p)
    if v == 42 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "second write should overwrite first; expected 42, got {outcome:?}"
    );
}
