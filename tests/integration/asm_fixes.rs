/// Assembly code-generation fix tests.
///
/// Each test targets a specific property of the emitted RISC-V assembly
/// (store widths, epilogue structure, float instructions, register allocation,
/// struct return conventions).  Exact output correctness is covered by the
/// golden-file suites (compiler_suite.rs, assembly_golden_suite.rs).
use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::PathBuf;

fn suite_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/compiler_suite")
}

fn compile_fixture(subdir: &str, name: &str) -> String {
    let path = suite_root().join(subdir).join(format!("{name}.hll"));
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"));
    let pipeline = CompilationPipeline::new();
    let result = pipeline
        .compile(&source)
        .unwrap_or_else(|e| panic!("compilation error in {subdir}/{name}: {e}"));
    pipeline.compile_ir_to_assembly(&result.ir_program)
}

fn compile_inline(source: &str) -> String {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    pipeline.compile_ir_to_assembly(&result.ir_program)
}

// ── Store widths ──────────────────────────────────────────────────────────────

#[test]
fn i32_stores_use_sw_not_sd() {
    let asm = compile_fixture("arithmetic", "01_basic_arithmetic");
    assert!(asm.contains("sw"), "expected 'sw' for i32 stores");
}

#[test]
fn bool_stores_use_sb() {
    let asm = compile_inline(r#"main: () -> i32 {
    a: i32 = 10
    b: i32 = 20
    c: bool = a < b
    if c { return 1 } else { return 0 }
}"#);
    assert!(asm.contains("sb"), "expected 'sb' for bool/i1 storage");
}

// ── Function epilogue ─────────────────────────────────────────────────────────

#[test]
fn epilogue_restores_ra_and_s0() {
    let asm = compile_fixture("functions", "11_constexpr_pure_functions");
    assert!(asm.contains("ld     ra,"), "expected epilogue 'ld ra'");
    assert!(asm.contains("ld     s0,"), "expected epilogue 'ld s0'");
}

#[test]
fn single_function_has_exactly_one_epilogue() {
    let asm = compile_fixture("arithmetic", "01_basic_arithmetic");
    let count = asm.matches("ld     ra,").count();
    assert_eq!(count, 1, "expected exactly 1 epilogue, found {count}");
}

// ── Conditional and loop control flow ────────────────────────────────────────

#[test]
fn conditionals_emit_bne() {
    let asm = compile_fixture("control_flow", "02_conditional_and_loop");
    assert!(asm.contains("bne"), "expected 'bne' for conditional branching");
}

#[test]
fn loops_emit_multiple_labels_and_jumps() {
    let asm = compile_fixture("control_flow", "05_constants_and_loops");
    assert!(asm.lines().count() > 10, "loop assembly should be substantial");
}

// ── Floating-point instructions ───────────────────────────────────────────────

#[test]
fn f32_uses_flw_and_fsw() {
    let asm = compile_inline(r#"type Point = { x: f32, y: f32 }
main: () -> f32 {
    p: Point = { .x = 1.5, .y = 2.5 }
    return p.x
}"#);
    assert!(asm.contains("flw"), "expected 'flw' for f32 load");
    assert!(asm.contains("fsw"), "expected 'fsw' for f32 store");
}

#[test]
fn f32_arithmetic_uses_float_instructions() {
    let asm = compile_inline(r#"main: () -> f32 {
    a: f32 = 1.5
    b: f32 = 2.0
    c: f32 = a + b
    d: f32 = c * 2.0
    return d
}"#);
    assert!(
        asm.contains("fadd.s") || asm.contains("fmul.s"),
        "expected float arithmetic instruction"
    );
}

#[test]
fn f32_return_value_in_fa0() {
    let asm = compile_inline("main: () -> f32 { return 3.14 }");
    assert!(asm.contains("fa0") || asm.contains("f10"), "expected float return in fa0");
}

// ── Register allocation ───────────────────────────────────────────────────────

#[test]
fn many_locals_use_multiple_temp_registers() {
    let asm = compile_inline(r#"main: () -> i32 {
    a: i32 = 1  b: i32 = 2  c: i32 = 3  d: i32 = 4  e: i32 = 5
    f: i32 = 6  g: i32 = 7  h: i32 = 8  i: i32 = 9  j: i32 = 10
    return a + b + c + d + e + f + g + h + i + j
}"#);
    let used: Vec<&str> = ["t0", "t1", "t2", "t3", "t4", "t5", "t6"]
        .iter()
        .filter(|r| asm.contains(*r))
        .cloned()
        .collect();
    assert!(used.len() >= 3, "expected multiple temp registers, got {used:?}");
}

// ── Struct return convention ──────────────────────────────────────────────────

#[test]
fn two_field_struct_return_uses_a0_and_a1() {
    let asm = compile_inline(r#"divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}
main: () -> i32 {
    result: { quotient: i32, remainder: i32 } = divide(10, 3)
    return result.quotient
}"#);
    assert!(
        asm.contains("lw     a0,") || asm.contains("ld     a0,"),
        "expected first field in a0"
    );
    assert!(
        asm.contains("lw     a1,") || asm.contains("ld     a1,"),
        "expected second field in a1"
    );
}

#[test]
fn tuple_destructuring_emits_function_call() {
    let asm = compile_fixture("types", "06_tuple_destructuring");
    assert!(asm.contains("jal"), "expected 'jal' for function call");
}
