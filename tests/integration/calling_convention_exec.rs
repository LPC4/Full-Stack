/// Integration tests for the RISC-V calling convention, verified by VM execution.
///
/// All tests compile HLL programs that return 0 (pass) or 1 (fail).
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

/// All eight argument registers (a0-a7) are passed and summed correctly.
#[test]
fn call_eight_args_all_used() {
    let (_, outcome, _) = run_hll(r#"
sum_eight: (a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32) -> i32 {
    return a + b + c + d + e + f + g + h
}
main: () -> i32 {
    result: i32 = sum_eight(1, 2, 3, 4, 5, 6, 7, 8)
    if result == 36 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "sum of 1..8 = 36, got {outcome:?}"
    );
}

/// The ninth argument must be passed via the caller's stack frame.
#[test]
fn call_ninth_arg() {
    let (_, outcome, _) = run_hll(r#"
sum_nine: (a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32, ninth: i32) -> i32 {
    return a + b + c + d + e + f + g + h + ninth
}
main: () -> i32 {
    result: i32 = sum_nine(1, 2, 3, 4, 5, 6, 7, 8, 9)
    if result == 45 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "sum of 1..9 = 45 (9th arg via stack), got {outcome:?}"
    );
}

/// Values held across a function call must be restored (callee-saved registers or stack spills).
#[test]
fn call_callee_saves_preserved() {
    let (_, outcome, _) = run_hll(r#"
compute: (x: i32, y: i32) -> i32 {
    return x + y
}
main: () -> i32 {
    preserved: i32 = 100
    inner_result: i32 = compute(3, 4)
    if preserved == 100 {
        if inner_result == 7 {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "value alive across call must be preserved; inner result must be 7, got {outcome:?}"
    );
}

/// Two-field struct returned inline (a0/a1 small-struct ABI).
#[test]
fn call_struct_return_two_fields() {
    let (_, outcome, _) = run_hll(r#"
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}
main: () -> i32 {
    { quotient: i32, remainder: i32 } = divide(17, 5)
    if quotient == 3 {
        if remainder == 2 {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "17 / 5 = 3 remainder 2 (struct return), got {outcome:?}"
    );
}

/// Return value must be usable in a subsequent expression.
#[test]
fn call_return_value_used_in_expr() {
    let (_, outcome, _) = run_hll(r#"
double: (n: i32) -> i32 {
    return n * 2
}
main: () -> i32 {
    result: i32 = double(21)
    check: i32 = result - 42
    if check == 0 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "double(21) = 42, used in expression, got {outcome:?}"
    );
}
