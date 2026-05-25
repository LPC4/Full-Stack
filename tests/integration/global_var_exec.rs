/// Integration tests for global variable read/write, BSS layout, and large-value
/// stores (the pmm_init-style bug), verified by VM execution.
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn run_hll(src: &str) -> (VirtualMachine, StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    
    pipeline.set_write_artifacts(false);
    let stdlib_result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile failed");
    let (_, stdlib_tokens) =
        pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let stdlib_obj = pipeline.assemble(&stdlib_tokens).expect("stdlib assemble failed");
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");
    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

/// A global i32 starts at zero and can be written then read back correctly.
#[test]
fn global_i32_write_read() {
    let (_, outcome, _) = run_hll(r#"
gval: i32 = 0

main: () -> i32 {
    gval = 42
    v: i32 = gval
    if v == 42 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "global i32 write 42 then read should return 42, got {outcome:?}"
    );
}

/// A global i64 can hold a large positive value (> i32::MAX).
/// Expected value built from safe arithmetic to avoid circular dependency on emit_li.
#[test]
fn global_i64_write_large_value() {
    let (_, outcome, _) = run_hll(r#"
big_addr: i64 = 0

main: () -> i32 {
    big_addr = 2148532224
    v: i64 = big_addr
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
        "global i64 should hold 0x8010_0000 (2148532224), got {outcome:?}"
    );
}

/// Two separate global variables do not alias each other.
#[test]
fn global_two_vars_independent() {
    let (_, outcome, _) = run_hll(r#"
alpha: i32 = 0
beta: i32 = 0

main: () -> i32 {
    alpha = 10
    beta = 20
    a: i32 = alpha
    b: i32 = beta
    if a == 10 {
        if b == 20 {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "two globals must be independent; alpha=10 beta=20, got {outcome:?}"
    );
}

/// Global variable in BSS section is zero-initialized at program start.
#[test]
fn global_bss_zero_init() {
    let (_, outcome, _) = run_hll(r#"
uninit: i32 = 0

main: () -> i32 {
    v: i32 = uninit
    if v == 0 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "BSS global must start at 0, got {outcome:?}"
    );
}

/// Repeated writes accumulate correctly (global as a counter).
#[test]
fn global_i32_counter() {
    let (_, outcome, _) = run_hll(r#"
counter: i32 = 0

bump: () -> void {
    counter = counter + 1
}

main: () -> i32 {
    bump()
    bump()
    bump()
    v: i32 = counter
    if v == 3 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "counter bumped 3 times should equal 3, got {outcome:?}"
    );
}

