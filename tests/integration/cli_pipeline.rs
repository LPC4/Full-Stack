//! Integration tests for the CLI pipeline: hll-to-ir, hll-to-asm, and run.
//!
//! These tests drive the same logic the `fsc` binary uses, calling the
//! underlying library functions directly so no binary invocation is needed.

use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::get_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

const STDLIB_PREFIX: &str = "_s_";
const USER_PREFIX: &str = "_u_";

fn make_pipeline(mode: TargetMode, prefix: &str) -> CompilationPipeline {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(mode);
    p.set_string_prefix(Some(prefix.to_owned()));
    p.set_run_semantic_analysis(false);
    p
}

/// Compile HLL + hosted stdlib → assembled output → run → (uart, exit_code).
fn run_hll(src: &str) -> (String, i64) {
    let stdlib_pipeline = make_pipeline(TargetMode::Hosted, STDLIB_PREFIX);
    let stdlib_result = stdlib_pipeline
        .compile(&get_stdlib_source())
        .expect("stdlib compile failed");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);

    let user_pipeline = make_pipeline(TargetMode::Hosted, USER_PREFIX);
    let user_result = user_pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) =
        user_pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);

    let assembled = user_pipeline
        .assemble_linked(&linked)
        .expect("assemble failed");

    let mut vm = VirtualMachine::new(&assembled);
    let result = vm.run(5_000_000);

    let code = match result.outcome {
        StepOutcome::Halted(c) => c,
        StepOutcome::Continue => panic!("program timed out"),
    };
    (result.uart_output, code)
}

/// Compile HLL to IR text (mirrors `fsc hll-to-ir`).
fn hll_to_ir_text(src: &str) -> String {
    let pipeline = make_pipeline(TargetMode::Hosted, USER_PREFIX);
    let result = pipeline.compile(src).expect("compile failed");
    result.ir_program.to_string()
}

/// Compile HLL to assembly text (mirrors `fsc hll-to-asm`).
fn hll_to_asm_text(src: &str) -> String {
    let pipeline = make_pipeline(TargetMode::Hosted, USER_PREFIX);
    let result = pipeline.compile(src).expect("compile failed");
    pipeline.compile_ir_to_assembly(&result.ir_program)
}

// ---------------------------------------------------------------------------
// hll-to-ir tests
// ---------------------------------------------------------------------------

#[test]
fn ir_output_contains_function_name() {
    let src = "add: (a: i32, b: i32) -> i32 { return a + b }";
    let ir = hll_to_ir_text(src);
    assert!(ir.contains("add"), "IR should contain the function name 'add'");
}

#[test]
fn ir_output_contains_math_op() {
    let src = "f: (x: i32) -> i32 { return x * 2 }";
    let ir = hll_to_ir_text(src);
    assert!(
        ir.contains("mul"),
        "IR should contain a multiply operation; got:\n{ir}"
    );
}

#[test]
fn ir_output_local_var_alloc() {
    let src = "f: () -> i32 { n: i32 = 99 return n }";
    let ir = hll_to_ir_text(src);
    assert!(
        ir.contains("stack_alloc"),
        "IR should contain stack_alloc for local variable; got:\n{ir}"
    );
}

#[test]
fn ir_output_multiple_functions() {
    let src = "
        square: (n: i32) -> i32 { return n * n }
        cube:   (n: i32) -> i32 { return n * n * n }
    ";
    let ir = hll_to_ir_text(src);
    assert!(ir.contains("square"), "IR should contain 'square'");
    assert!(ir.contains("cube"), "IR should contain 'cube'");
}

// ---------------------------------------------------------------------------
// hll-to-asm tests
// ---------------------------------------------------------------------------

#[test]
fn asm_output_contains_function_label() {
    let src = "my_func: (x: i32) -> i32 { return x }";
    let asm = hll_to_asm_text(src);
    assert!(
        asm.contains("my_func:"),
        "assembly should define 'my_func:' label; got:\n{asm}"
    );
}

#[test]
fn asm_output_has_prologue() {
    let src = "f: () -> i32 { return 0 }";
    let asm = hll_to_asm_text(src);
    assert!(
        asm.contains("addi") && asm.contains("sp"),
        "assembly should have stack-pointer adjustment in prologue; got:\n{asm}"
    );
}

#[test]
fn asm_output_has_ret() {
    let src = "f: () -> i32 { return 42 }";
    let asm = hll_to_asm_text(src);
    assert!(
        asm.contains("ret") || asm.contains("jalr"),
        "assembly should contain a return instruction; got:\n{asm}"
    );
}

#[test]
fn asm_output_text_section() {
    let src = "f: () -> i32 { return 0 }";
    let asm = hll_to_asm_text(src);
    assert!(
        asm.contains(".section .text") || asm.contains(".text"),
        "assembly should have a .text section; got:\n{asm}"
    );
}

// ---------------------------------------------------------------------------
// run tests
// ---------------------------------------------------------------------------

#[test]
fn run_simple_return_value() {
    let src = "main: () -> i32 { return 42 }";
    let (_, code) = run_hll(src);
    assert_eq!(code, 42, "program should return 42");
}

#[test]
fn run_arithmetic_exit_code() {
    // (10 + 20) * 2 / 5 - (10 % 20) = 12 - 10 = 2; negated = -2
    let src = "
main: () -> i32 {
    a: i32 = 10
    b: i32 = 20
    c: i32 = (a + b) * 2
    d: i32 = c / 5 - (a % b)
    return -d
}";
    let (_, code) = run_hll(src);
    assert_eq!(code, -2);
}

#[test]
fn run_conditional_branch() {
    let src = "
main: () -> i32 {
    x: i32 = 7
    if x > 5 {
        return 1
    }
    return 0
}";
    let (_, code) = run_hll(src);
    assert_eq!(code, 1, "branch should be taken when x=7 > 5");
}

#[test]
fn run_while_loop_accumulator() {
    let src = "
main: () -> i32 {
    sum: i32 = 0
    i: i32 = 1
    while i <= 10 {
        sum = sum + i
        i = i + 1
    }
    return sum
}";
    let (_, code) = run_hll(src);
    assert_eq!(code, 55, "sum 1..=10 should be 55");
}

#[test]
fn run_function_call() {
    let src = "
double: (n: i32) -> i32 { return n * 2 }
main: () -> i32 { return double(21) }";
    let (_, code) = run_hll(src);
    assert_eq!(code, 42);
}

#[test]
fn run_zero_exit_code() {
    let src = "main: () -> i32 { return 0 }";
    let (_, code) = run_hll(src);
    assert_eq!(code, 0);
}
