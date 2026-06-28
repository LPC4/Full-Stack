//! VM round-trip tests for physical register allocation.
//!
//! Every test compiles the same program with register allocation off and on,
//! runs both in the VM, and asserts identical exit outcome and UART output.
//! The allocator must be a pure performance change.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use hll_to_ir::TargetMode;
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Stdlib compiled once per flag setting (the flag changes its codegen too).
// Each module is its own object (no source concatenation).
fn cached_stdlib_objs(regalloc: bool) -> &'static [(String, AssembledOutput)] {
    static STDLIB_OFF: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    static STDLIB_ON: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    let cell = if regalloc { &STDLIB_ON } else { &STDLIB_OFF };
    cell.get_or_init(|| {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_write_artifacts(false);
        pipeline.set_register_allocation(regalloc);
        pipeline.set_type_prelude(get_stdlib_type_prelude());
        get_stdlib_modules_for_mode(TargetMode::Hosted)
            .iter()
            .map(|(name, src)| {
                let r = pipeline.compile(src).expect("stdlib compile failed");
                let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&r.ir_program);
                let obj = pipeline
                    .assemble_named(name, &tokens)
                    .expect("stdlib assemble failed");
                ((*name).to_owned(), obj)
            })
            .collect()
    })
}

/// Compile and run with register allocation on or off; returns the VM outcome,
/// UART output, emitted user-code instruction count, and executed cycles.
fn run_with_regalloc(src: &str, regalloc: bool) -> (StepOutcome, String, usize, u64) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_register_allocation(regalloc);

    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let code_lines = user_tokens.iter().filter(|t| t.is_code()).count();
    let user_obj = pipeline
        .assemble(&user_tokens)
        .expect("user assemble failed");

    let mut modules: Vec<(&str, &AssembledOutput)> = cached_stdlib_objs(regalloc)
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(20_000_000);
    let cycles = vm.pipeline_stats().cycles;
    (run.outcome, run.uart_output.clone(), code_lines, cycles)
}

/// Assert the program behaves identically with allocation off and on, and
/// return the (off, on) instruction counts and cycle counts for extra checks.
fn assert_equivalent(src: &str) -> ((usize, u64), (usize, u64)) {
    let (base_outcome, base_uart, base_lines, base_cycles) = run_with_regalloc(src, false);
    let (alloc_outcome, alloc_uart, alloc_lines, alloc_cycles) = run_with_regalloc(src, true);
    assert!(
        matches!(base_outcome, StepOutcome::Halted(_)),
        "baseline did not finish: {base_outcome:?}"
    );
    assert_eq!(
        format!("{base_outcome:?}"),
        format!("{alloc_outcome:?}"),
        "register allocation changed the exit outcome"
    );
    assert_eq!(
        base_uart, alloc_uart,
        "register allocation changed UART output"
    );
    ((base_lines, base_cycles), (alloc_lines, alloc_cycles))
}

#[test]
fn regalloc_arithmetic_branches_and_loops() {
    let ((base_lines, base_cycles), (alloc_lines, alloc_cycles)) = assert_equivalent(
        r#"
compute: (n: i64) -> i64 {
    a: i64 = n + 1
    b: i64 = a * 2
    c: i64 = 0
    if b > 10 {
        c = b - a
    } else {
        c = a + b
    }
    d: i64 = c + a
    return d
}

main: () -> i32 {
    total: i64 = 0
    i: i64 = 0
    while i < 50 {
        total = total + compute(i)
        i = i + 1
    }
    return (total % 256) as i32
}
"#,
    );
    assert!(
        alloc_lines < base_lines,
        "register allocation should remove slot traffic: {base_lines} -> {alloc_lines}"
    );
    assert!(
        alloc_cycles < base_cycles,
        "register allocation should reduce executed cycles: {base_cycles} -> {alloc_cycles}"
    );
}

#[test]
fn regalloc_hot_loop_is_substantially_faster() {
    // A tight arithmetic loop is the allocator's target workload; require a
    // real win, not statistical noise.
    let ((_, base_cycles), (_, alloc_cycles)) = assert_equivalent(
        r#"
main: () -> i32 {
    acc: i64 = 0
    i: i64 = 0
    while i < 20000 {
        acc = acc + i * 3 - (i / 2)
        i = i + 1
    }
    return (acc % 100) as i32
}
"#,
    );
    println!("hot loop cycles: {base_cycles} -> {alloc_cycles}");
    let saved = base_cycles.saturating_sub(alloc_cycles);
    assert!(
        saved * 4 >= base_cycles,
        "hot loop should run at least 25% faster: {base_cycles} -> {alloc_cycles}"
    );
}

#[test]
fn regalloc_recursion_preserves_callee_saved_values() {
    // Live values across recursive calls sit in callee-saved registers; the
    // prologue/epilogue must preserve them through arbitrary call depth.
    assert_equivalent(
        r#"
fib: (n: i64) -> i64 {
    if n < 2 {
        return n
    }
    a: i64 = fib(n - 1)
    b: i64 = fib(n - 2)
    return a + b
}

main: () -> i32 {
    return fib(15) as i32
}
"#,
    );
}

#[test]
fn regalloc_nine_variable_args_pass_on_stack() {
    // Ninth-and-beyond arguments travel on the stack; passing live variables
    // (not constants) exercises the slot loads around the sp adjustment, which
    // previously read from stale offsets after sp had already moved.
    let src = r#"
sum_nine: (a: i64, b: i64, c: i64, d: i64, e: i64, f: i64, g: i64, h: i64, ninth: i64) -> i64 {
    return a + b + c + d + e + f + g + h + ninth
}

main: () -> i32 {
    x: i64 = 10
    y: i64 = 20
    z: i64 = 12
    r: i64 = sum_nine(x, y, z, x, y, z, x, y, z)
    return (r % 256) as i32
}
"#;
    let (outcome, _, _, _) = run_with_regalloc(src, false);
    assert!(
        matches!(outcome, StepOutcome::Halted(126)),
        "expected Halted(126) = 3*(10+20+12), got {outcome:?}"
    );
    assert_equivalent(src);
}

#[test]
fn regalloc_pointers_structs_and_arrays() {
    assert_equivalent(
        r#"
external free: (p: i64*) -> void

struct Point {
    x: i64,
    y: i64,
}

sum_buffer: (buf: i64*, len: i64) -> i64 {
    total: i64 = 0
    i: i64 = 0
    while i < len {
        total = total + buf[i]
        i = i + 1
    }
    return total
}

main: () -> i32 {
    p: Point = { .x = 17 as i64, .y = 25 as i64 }
    arr: i64[4] = [1 as i64, 2 as i64, 3 as i64, 4 as i64]
    local: i64 = arr[0] + arr[1] + arr[2] + arr[3]
    buf: i64* = new(i64, 4)
    i: i64 = 0
    while i < 4 {
        buf[i] = i * 10
        i = i + 1
    }
    s: i64 = sum_buffer(buf, 4)
    free(buf)
    return (p.x + p.y + local + s) as i32
}
"#,
    );
}

#[test]
fn regalloc_mixed_float_and_int() {
    // Floats stay slot-based while their integer neighbors are allocated; the
    // GP/FP boundary (casts, compares) must agree between modes.
    assert_equivalent(
        r#"
main: () -> i32 {
    acc: f64 = 0.0
    half: f64 = 0.5
    lo: f64 = 2474.0
    hi: f64 = 2476.0
    i: i64 = 0
    while i < 100 {
        acc = acc + (i as f64) * half
        i = i + 1
    }
    if acc > lo {
        if acc < hi {
            return 42
        }
    }
    return 1
}
"#,
    );
}

#[test]
fn regalloc_narrow_width_wrapping() {
    // Narrow integer results are width-normalized in registers exactly like a
    // slot store/reload would truncate and sign-extend.
    assert_equivalent(
        r#"
main: () -> i32 {
    a: i8 = 100
    b: i8 = 100
    c: i8 = a + b
    w: i16 = 30000
    x: i16 = w + w
    y: i32 = 70000
    z: i32 = y * y
    return ((c as i64) + (x as i64) + (z % 1000) as i64) as i32
}
"#,
    );
}

#[test]
fn regalloc_heap_alloc_and_uart_output() {
    // malloc results cross a call boundary into allocated registers; printing
    // exercises stdlib calls with allocated arguments.
    assert_equivalent(
        r#"
console := import("console")
external free: (p: i64*) -> void

main: () -> i32 {
    p: i64* = new(i64, 8)
    i: i64 = 0
    while i < 8 {
        p[i] = i * i
        i = i + 1
    }
    total: i64 = 0
    i = 0
    while i < 8 {
        total = total + p[i]
        i = i + 1
    }
    console.putchar(65 + (total % 26) as i32)
    console.putchar(10)
    free(p)
    return (total % 256) as i32
}
"#,
    );
}

#[test]
fn regalloc_composes_with_peephole_and_ir_opt() {
    // All three optimization flags together must still be behavior-preserving.
    let src = r#"
compute: (n: i64) -> i64 {
    a: i64 = n + 1
    b: i64 = a * 2
    c: i64 = 0
    if b > 10 {
        c = b - a
    } else {
        c = a + b
    }
    return c + a
}

main: () -> i32 {
    total: i64 = 0
    i: i64 = 0
    while i < 25 {
        total = total + compute(i)
        i = i + 1
    }
    return (total % 256) as i32
}
"#;
    let (base_outcome, base_uart, _, _) = run_with_regalloc(src, false);

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_register_allocation(true);
    pipeline.set_peephole(true);
    pipeline.set_optimize(hll_to_ir::OptOptions::all());
    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline
        .assemble(&user_tokens)
        .expect("user assemble failed");
    let mut modules: Vec<(&str, &AssembledOutput)> = cached_stdlib_objs(true)
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(20_000_000);

    assert_eq!(
        format!("{base_outcome:?}"),
        format!("{:?}", run.outcome),
        "combined optimization flags changed the exit outcome"
    );
    assert_eq!(
        base_uart, run.uart_output,
        "combined flags changed UART output"
    );
}
