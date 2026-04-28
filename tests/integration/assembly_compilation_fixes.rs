use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::PathBuf;

fn suite_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/compiler_suite")
}

/// Recursively collect all .hll files from a directory tree
fn collect_hll_files(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_hll_files(&path, files);
            } else if path.extension().and_then(|e| e.to_str()) == Some("hll") {
                files.push(path);
            }
        }
    }
}

fn compile_fixture(subdir: &str, name: &str) -> String {
    let path = suite_root().join(subdir).join(format!("{}.hll", name));
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
    
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&source)
        .unwrap_or_else(|err| panic!("compilation error in {subdir}/{name}: {err}"));
    
    pipeline.compile_ir_to_assembly(&result.ir_program)
}

// ===========================================================================
// Fix 1: Type-width stores (sw/sb/sh vs sd)
// ===========================================================================

#[test]
fn fix1_uses_correct_store_widths() {
    let asm = compile_fixture("arithmetic", "01_basic_arithmetic");
    
    // i32 operations should use sw (store word), not just sd
    assert!(asm.contains("sw"), 
            "expected 'sw' for i32 stores in basic arithmetic");
}

#[test]
fn fix1_bool_uses_sb() {
    let source = r#"main: () -> i32 {
    a: i32 = 10
    b: i32 = 20
    c: bool = a < b
    if c { return 1 } else { return 0 }
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // Boolean (i1) comparison results should use sb (store byte)
    assert!(asm.contains("sb"), 
            "expected 'sb' for i1/bool storage");
}

// ===========================================================================
// Fix 2: Epilogue emitted before every ret
// ===========================================================================

#[test]
fn fix2_epilogue_before_return() {
    let asm = compile_fixture("functions", "11_constexpr_pure_functions");
    
    // Should have epilogue sequence (ld ra, ld s0, addi sp) before jalr
    assert!(asm.contains("ld     ra,"), 
            "expected epilogue with 'ld ra' instruction");
    assert!(asm.contains("ld     s0,"), 
            "expected epilogue with 'ld s0' instruction");
}

#[test]
fn fix2_no_duplicate_epilogue() {
    let asm = compile_fixture("arithmetic", "01_basic_arithmetic");
    
    // Count occurrences of epilogue pattern - should be exactly one per function
    let epilogue_count = asm.matches("ld     ra,").count();
    assert_eq!(epilogue_count, 1, 
               "expected exactly 1 epilogue, found {epilogue_count}");
}

// ===========================================================================
// Fix 3: Branch inversion (then/else labels correct)
// ===========================================================================

#[test]
fn fix3_conditional_branches_correct() {
    let asm = compile_fixture("control_flow", "02_conditional_and_loop");
    
    // Should have bne for conditional branches
    assert!(asm.contains("bne"), 
            "expected 'bne' instruction for conditional branching");
}

#[test]
fn fix3_loop_control_flow() {
    let asm = compile_fixture("control_flow", "05_constants_and_loops");
    
    // Loops require multiple labels and backward jumps
    let line_count = asm.lines().count();
    assert!(line_count > 10, 
            "loop control flow should generate substantial assembly");
}

// ===========================================================================
// Fix 4: Float arithmetic dispatch (flw/fsw/fadd.s)
// ===========================================================================

#[test]
fn fix4_float_load_store() {
    let source = r#"type Point = {
    x: f32,
    y: f32
}

main: () -> f32 {
    p: Point = { .x = 1.5, .y = 2.5 }
    return p.x
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // Should use flw (float load word) for loading f32
    assert!(asm.contains("flw"), 
            "expected 'flw' for f32 load operations");
    
    // Should use fsw (float store word) for storing f32
    assert!(asm.contains("fsw"), 
            "expected 'fsw' for f32 store operations");
}

#[test]
fn fix4_float_arithmetic() {
    let source = r#"main: () -> f32 {
    a: f32 = 1.5
    b: f32 = 2.0
    c: f32 = a + b
    d: f32 = c * 2.0
    return d
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // Should use float arithmetic instructions
    assert!(asm.contains("fadd.s") || asm.contains("fmul.s") || 
            asm.contains("fsub.s") || asm.contains("fdiv.s"),
            "expected float arithmetic instruction (fadd.s, fmul.s, etc.)");
}

#[test]
fn fix4_float_return_in_fa0() {
    let source = r#"main: () -> f32 {
    return 3.14
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // Float return value should be in fa0 (register f10)
    assert!(asm.contains("fa0") || asm.contains("f10"), 
            "expected float return in fa0/f10 register");
}

// ===========================================================================
// Fix 5: Temp counter reset (no register aliasing)
// ===========================================================================

#[test]
fn fix5_no_register_aliasing() {
    let source = r#"main: () -> i32 {
    a: i32 = 1
    b: i32 = 2
    c: i32 = 3
    d: i32 = 4
    e: i32 = 5
    f: i32 = 6
    g: i32 = 7
    h: i32 = 8
    i: i32 = 9
    j: i32 = 10
    result: i32 = a + b + c + d + e + f + g + h + i + j
    return result
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // With temp counter reset, each instruction should start fresh
    // Verify compilation succeeds and uses multiple temp registers
    let t_regs_used: Vec<&str> = ["t0", "t1", "t2", "t3", "t4", "t5", "t6"]
        .iter()
        .filter(|reg| asm.contains(*reg))
        .cloned()
        .collect();
    
    assert!(t_regs_used.len() >= 3, 
            "expected multiple temp registers, got {t_regs_used:?}");
}

// ===========================================================================
// Fix 6: Struct return ABI (fields in a0/a1 separately)
// ===========================================================================

#[test]
fn fix6_struct_return_two_fields() {
    let source = r#"divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    result: { quotient: i32, remainder: i32 } = { .quotient = a / b, .remainder = a % b }
    return result
}

main: () -> i32 {
    result: { quotient: i32, remainder: i32 } = divide(10, 3)
    return result.quotient
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compilation failed");
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);
    
    // Should load first field into a0 and second into a1 (x11)
    let has_a0_load = asm.contains("lw     a0,") || asm.contains("ld     a0,");
    let has_a1_load = asm.contains("lw     a1,") || asm.contains("ld     a1,") || 
                      asm.contains("lw     11,") || asm.contains("ld     11,");
    
    assert!(has_a0_load, "expected first struct field loaded into a0");
    assert!(has_a1_load, "expected second struct field loaded into a1/x11");
}

#[test]
fn fix6_tuple_destructuring() {
    let asm = compile_fixture("types", "06_tuple_destructuring");
    
    // Should have function calls and struct handling
    assert!(asm.contains("jal"), 
            "expected function call instruction");
}

// ===========================================================================
// Integration: Full compilation pipeline
// ===========================================================================

#[test]
fn execute_assembly_fix_validation_suite() {
    let root = suite_root();
    let mut hll_files = Vec::new();
    collect_hll_files(&root, &mut hll_files);

    // Sort for consistent test execution order
    hll_files.sort();

    let mut tests_run = 0;
    let pipeline = CompilationPipeline::new();

    for path in hll_files {
        if path.extension().and_then(|e| e.to_str()) == Some("hll") {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));

            // Compile HLL -> IR -> Assembly using shared pipeline
            let result = pipeline.compile(&source).unwrap_or_else(|e| {
                panic!(
                    "compilation error in {:?}: {}",
                    path.file_name().unwrap(),
                    e
                )
            });

            let actual_asm = pipeline.compile_ir_to_assembly(&result.ir_program);
            
            // Basic sanity checks for all compiled output
            assert!(actual_asm.contains(".text"), 
                    "{:?}: assembly should contain .text section", 
                    path.file_name().unwrap());
            assert!(actual_asm.contains("sp,"), 
                    "{:?}: assembly should use stack pointer", 
                    path.file_name().unwrap());
            
            tests_run += 1;
        }
    }

    assert!(tests_run > 0, "no tests found in assembly fix validation suite");
    println!(
        "\nsuccessfully ran {} assembly compilation tests across all categories",
        tests_run
    );
}
