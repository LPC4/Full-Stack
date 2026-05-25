/// Integration tests for struct field offsets, nested structs, and heap-based
/// multi-element access, verified by VM execution.
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

/// Two-field struct: fields land at the correct offsets.
#[test]
fn struct_two_field_access() {
    let (_, outcome, _) = run_hll(r#"
type Point = { x: i32, y: i32 }

make_point: (a: i32, b: i32) -> Point {
    return { .x = a, .y = b }
}

main: () -> i32 {
    p: Point = make_point(3, 7)
    if p.x == 3 {
        if p.y == 7 {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "struct fields x=3 y=7 must be readable at correct offsets, got {outcome:?}"
    );
}

/// Struct field arithmetic: compute a value from both fields.
#[test]
fn struct_field_arithmetic() {
    let (_, outcome, _) = run_hll(r#"
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}

main: () -> i32 {
    { quotient: i32, remainder: i32 } = divide(23, 7)
    sum: i32 = quotient + remainder
    if sum == 5 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "23 / 7 = 3 remainder 2; quotient + remainder = 5, got {outcome:?}"
    );
}

/// Struct returned from one call fed into another.
#[test]
fn struct_return_chained() {
    let (_, outcome, _) = run_hll(r#"
minmax: (a: i32, b: i32) -> { lo: i32, hi: i32 } {
    if a < b {
        return { .lo = a, .hi = b }
    }
    return { .lo = b, .hi = a }
}

span: (lo: i32, hi: i32) -> i32 {
    return hi - lo
}

main: () -> i32 {
    { lo: i32, hi: i32 } = minmax(10, 3)
    s: i32 = span(lo, hi)
    if s == 7 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "minmax(10,3) -> lo=3,hi=10 -> span=7, got {outcome:?}"
    );
}

/// Two independent heap i32 allocations do not overlap (stride = sizeof i32 = 4).
#[test]
fn heap_two_i32_slots_independent() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i32*) -> void

main: () -> i32 {
    p: i32* = new(i32)
    q: i32* = new(i32)
    @p = 100
    @q = 200
    vp: i32 = @p
    vq: i32 = @q
    free(p)
    free(q)
    if vp == 100 {
        if vq == 200 {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "two heap i32 slots must be independent (p=100, q=200), got {outcome:?}"
    );
}

/// Three-field struct: middle field is at the correct byte offset.
#[test]
fn struct_three_field_middle_offset() {
    let (_, outcome, _) = run_hll(r#"
type Triple = { a: i32, b: i32, c: i32 }

make: (x: i32, y: i32, z: i32) -> Triple {
    return { .a = x, .b = y, .c = z }
}

main: () -> i32 {
    t: Triple = make(1, 99, 3)
    if t.b == 99 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "middle field b=99 must be readable at offset 4, got {outcome:?}"
    );
}

/// Struct with mixed i32 and i64 fields: i64 must be aligned to 8 bytes.
#[test]
fn struct_mixed_i32_i64_fields() {
    let (_, outcome, _) = run_hll(r#"
type Mixed = { small: i32, big: i64 }

make_mixed: (s: i32, b: i64) -> Mixed {
    return { .small = s, .big = b }
}

main: () -> i32 {
    m: Mixed = make_mixed(7, 2148532224)
    a: i64 = 1048576
    expected: i64 = a * 2049
    if m.small == 7 {
        if m.big == expected {
            return 0
        }
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "mixed struct: small=7 big=0x8010_0000, got {outcome:?}"
    );
}

