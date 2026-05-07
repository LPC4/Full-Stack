use full_stack::assembly_language::assembler::Assembler;
use full_stack::assembly_language::real::RealInstruction;
use full_stack::assembly_language::riscv::rv64i::*;
use full_stack::assembly_language::riscv::rv64m::*;
use full_stack::assembly_language::riscv::rv64zicsr::Csrrs;
use full_stack::assembly_language::rv_instruction::RvInstruction;
use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::high_level_language::stdlib::prepend_stdlib;
use full_stack::virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ---------------------------------------------------------------------------
// Full HLL pipeline helpers
// ---------------------------------------------------------------------------

fn run_hll_with_limit(src: &str, max_steps: u64) -> (VirtualMachine, StepOutcome, String) {
    let src_with_stdlib = prepend_stdlib(src);
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src_with_stdlib).expect("compile failed");
    let (_, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(max_steps);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

fn run_hll_file(path: &str) -> (VirtualMachine, StepOutcome, String) {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let full_path = std::path::Path::new(manifest).join(path);
    let src = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    run_hll_with_limit(&src, 50_000_000)
}

fn run_hll(src: &str) -> (VirtualMachine, StepOutcome, String) {
    run_hll_with_limit(src, 5_000_000)
}

// ---------------------------------------------------------------------------
// HLL full-pipeline VM tests
// ---------------------------------------------------------------------------

#[test]
fn hll_arithmetic_return() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: i32 = 6
    b: i32 = 7
    return a * b
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn hll_return_zero() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    return 0
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "expected Halted(0), got {outcome:?}");
}

#[test]
fn hll_function_call_and_return() {
    let (_, outcome, _) = run_hll(r#"
add: (a: i32, b: i32) -> i32 {
    return a + b
}
main: () -> i32 {
    return add(10, 32)
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn hll_putchar_output() {
    let (_, outcome, uart) = run_hll(r#"
main: () -> i32 {
    putchar(65)
    putchar(66)
    putchar(67)
    return 0
}
"#);
    assert_eq!(uart, "ABC", "expected UART='ABC', got {uart:?}");
    assert!(matches!(outcome, StepOutcome::Halted(0)), "expected Halted(0), got {outcome:?}");
}

#[test]
fn hll_user_function_calls_putchar() {
    let (_, outcome, uart) = run_hll(r#"
emit: (c: i32) -> i32 {
    putchar(c)
    return 0
}
main: () -> i32 {
    emit(65)
    emit(66)
    return 0
}
"#);
    assert_eq!(uart, "AB", "expected UART='AB', got {uart:?}");
    assert!(matches!(outcome, StepOutcome::Halted(0)), "expected Halted(0), got {outcome:?}");
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn ri(i: RealInstruction) -> RvInstruction {
    RvInstruction::Real(i)
}

/// Assemble instructions, run in VM, return the VM (for peeking) and the outcome.
fn assemble_and_run(insns: Vec<RvInstruction>) -> (VirtualMachine, StepOutcome) {
    let assembled = Assembler::assemble(&insns).expect("assembly failed");
    let mut vm = VirtualMachine::new(&assembled);
    let result = vm.run(100_000);
    (vm, result.outcome)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_exit_zero() {
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(10, 0, 0))),   // a0 = 0
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93 (exit)
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "expected Halted(0), got {outcome:?}");
}

#[test]
fn test_exit_code_42() {
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(10, 0, 42))),  // a0 = 42
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93 (exit)
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn test_add_two_numbers() {
    // 10 + 20 = 30
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 10))),  // a1 = 10
        ri(RealInstruction::Addi(Addi::new(12, 0, 20))),  // a2 = 20
        ri(RealInstruction::Add(Add::new(10, 11, 12))),   // a0 = a1 + a2 = 30
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(30)), "expected Halted(30), got {outcome:?}");
}

#[test]
fn test_subtract() {
    // 100 - 37 = 63
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 100))), // a1 = 100
        ri(RealInstruction::Addi(Addi::new(12, 0, 37))),  // a2 = 37
        ri(RealInstruction::Sub(Sub::new(10, 11, 12))),   // a0 = 100 - 37 = 63
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(63)), "expected Halted(63), got {outcome:?}");
}

#[test]
fn test_memory_store_load() {
    // t0=x5, t1=x6, sp=x2, a0=x10, a7=x17
    // Store 99 to stack scratch space, load it back into a0, exit with it.
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(5, 0, 99))),   // t0 = 99
        ri(RealInstruction::Addi(Addi::new(6, 2, -8))),   // t1 = sp - 8 (scratch address)
        ri(RealInstruction::Sd(Sd::new(6, 5, 0))),        // sd t0, 0(t1)  — store 99
        ri(RealInstruction::Ld(Ld::new(10, 6, 0))),       // ld a0, 0(t1)  — load 99
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(99)), "expected Halted(99), got {outcome:?}");
}

#[test]
fn test_branch_loop() {
    // Count down from 5 to 0, then exit with 0.
    // t0=x5
    // addi t0, x0, 5
    // loop:
    //   addi t0, t0, -1
    //   bne  t0, x0, -4   (branch back to the addi, offset = -4 bytes)
    // add a0, t0, x0
    // addi a7, x0, 93
    // ecall
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(5, 0, 5))),    // t0 = 5
        ri(RealInstruction::Addi(Addi::new(5, 5, -1))),   // t0-- (loop body)
        ri(RealInstruction::Bne(Bne::new(5, 0, -4))),     // if t0 != 0, branch to loop body
        ri(RealInstruction::Add(Add::new(10, 5, 0))),     // a0 = t0 (= 0)
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "expected Halted(0), got {outcome:?}");
}

#[test]
fn test_multiply() {
    // 6 * 7 = 42
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 6))),   // a1 = 6
        ri(RealInstruction::Addi(Addi::new(12, 0, 7))),   // a2 = 7
        ri(RealInstruction::Mul(Mul::new(10, 11, 12))),   // a0 = 6 * 7 = 42
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn test_divide() {
    // 100 / 4 = 25
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 100))), // a1 = 100
        ri(RealInstruction::Addi(Addi::new(12, 0, 4))),   // a2 = 4
        ri(RealInstruction::Div(Div::new(10, 11, 12))),   // a0 = 100 / 4 = 25
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    assert!(matches!(outcome, StepOutcome::Halted(25)), "expected Halted(25), got {outcome:?}");
}

#[test]
fn test_ecall_write_uart() {
    // Write "hi\n" to stdout (fd=1) using ecall 64 (write), then exit.
    // t0=x5 (buffer pointer), t1=x6 (byte value), sp=x2
    // a0=x10, a1=x11, a2=x12, a7=x17
    let assembled = Assembler::assemble(&[
        ri(RealInstruction::Addi(Addi::new(5, 2, -4))),   // t0 = sp - 4 (buffer ptr)
        ri(RealInstruction::Addi(Addi::new(6, 0, 0x68))), // t1 = 'h' (104)
        ri(RealInstruction::Sb(Sb::new(5, 6, 0))),        // sb t1, 0(t0)
        ri(RealInstruction::Addi(Addi::new(6, 0, 0x69))), // t1 = 'i' (105)
        ri(RealInstruction::Sb(Sb::new(5, 6, 1))),        // sb t1, 1(t0)
        ri(RealInstruction::Addi(Addi::new(6, 0, 0x0A))), // t1 = '\n' (10)
        ri(RealInstruction::Sb(Sb::new(5, 6, 2))),        // sb t1, 2(t0)
        // ecall write: a0=1 (fd=stdout), a1=buf, a2=3 (len), a7=64
        ri(RealInstruction::Addi(Addi::new(10, 0, 1))),   // a0 = 1 (fd)
        ri(RealInstruction::Add(Add::new(11, 5, 0))),     // a1 = t0 (buf ptr)
        ri(RealInstruction::Addi(Addi::new(12, 0, 3))),   // a2 = 3 (len)
        ri(RealInstruction::Addi(Addi::new(17, 0, 64))),  // a7 = 64 (write)
        ri(RealInstruction::Ecall(Ecall::new())),          // syscall write
        // exit(0)
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Addi(Addi::new(10, 0, 0))),   // a0 = 0
        ri(RealInstruction::Ecall(Ecall::new())),
    ])
    .expect("assembly failed");

    let mut vm = VirtualMachine::new(&assembled);
    let result = vm.run(100_000);
    assert_eq!(result.uart_output, "hi\n", "UART output mismatch");
    assert!(
        matches!(result.outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {:?}",
        result.outcome
    );
}

#[test]
fn test_div_by_zero() {
    // divu a0, a1, x0 — divide by zero should yield u64::MAX in a0.
    // We overwrite a0 with 0 before exit so the exit code is 0,
    // then check a0 was u64::MAX via peek_reg before the overwrite.
    // Instead, use a simpler approach: read the divu result directly via peek_reg.
    let (vm, _outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 42))),  // a1 = 42
        ri(RealInstruction::Divu(Divu::new(10, 11, 0))),  // a0 = divu(42, 0) = u64::MAX
        // Save result to t0 before overwriting a0 for exit
        ri(RealInstruction::Add(Add::new(5, 10, 0))),     // t0 = a0 (preserved)
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Addi(Addi::new(10, 0, 0))),   // a0 = 0 (exit code)
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    // t0=x5 should hold u64::MAX from the divu
    assert_eq!(vm.peek_reg(5), u64::MAX, "divu by zero should produce u64::MAX");
}

#[test]
fn test_csr_instret() {
    // Run a few nops, then read instret via csrrs a0, 0xC02, x0, then exit.
    // The exit code (as i64) should be > 3 (we ran at least 3 nops + csrrs + addi + ecall).
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 1
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 2
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 3
        // csrrs a0, instret (0xC02), x0  — read instret, no write
        ri(RealInstruction::Csrrs(Csrrs::new(10, 0xC02, 0))),
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    // instret should be >= 3 (the 3 nops have retired; csrrs reads the count before incrementing)
    assert!(
        matches!(outcome, StepOutcome::Halted(n) if n >= 3),
        "expected instret >= 3, got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// End-to-end qemu program tests (full HLL pipeline through VM)
// ---------------------------------------------------------------------------

#[test]
fn qemu_01_arithmetic_and_types() {
    let (_, outcome, _) = run_hll_file("programs/test/qemu/01_arithmetic_and_types.hll");
    assert!(
        matches!(outcome, StepOutcome::Halted(42)),
        "expected Halted(42), got {outcome:?}"
    );
}

#[test]
fn qemu_02_control_flow() {
    let (_, outcome, _) = run_hll_file("programs/test/qemu/02_control_flow.hll");
    assert!(
        matches!(outcome, StepOutcome::Halted(100)),
        "expected Halted(100), got {outcome:?}"
    );
}

#[test]
fn qemu_03_structs_and_destructuring() {
    let (_, outcome, _) = run_hll_file("programs/test/qemu/03_structs_and_destructuring.hll");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {outcome:?}"
    );
}

#[test]
fn qemu_04_pointers_and_memory() {
    let (_, outcome, _) = run_hll_file("programs/test/qemu/04_pointers_and_memory.hll");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {outcome:?}"
    );
}

/// Single new/free cycle, no defer.
#[test]
fn hll_new_and_free_basic() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 42
    v: i32 = @p
    free(p)
    return v
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

/// Allocate, free, then reallocate - exercises the free-list reuse path.
#[test]
fn hll_new_free_reuse() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 1
    free(p)
    q: i32* = new(i32)
    @q = 42
    v: i32 = @q
    free(q)
    return v
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

/// defer free on a heap pointer.
#[test]
fn hll_defer_free() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    p: i32* = new(i32)
    defer free(p)
    @p = 42
    return @p
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn debug_malloc_ir() {
    let src = r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 42
    v: i32 = @p
    free(p)
    return v
}
"#;
    let src_with_stdlib = prepend_stdlib(src);
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src_with_stdlib).expect("compile failed");
    let ir_text = format!("{}", result.ir_program);
    let (asm, _) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    // Print just the heap_raw_alloc and malloc IR functions
    for line in ir_text.lines() {
        if line.contains("heap_raw_alloc") || line.contains("malloc") || line.contains("heap_list") || line.contains("heap_bump") {
            println!("{line}");
        }
    }
    println!("--- ASM (heap_raw_alloc section) ---");
    let mut in_fn = false;
    for line in asm.lines() {
        if line.contains("heap_raw_alloc:") { in_fn = true; }
        if in_fn {
            println!("{line}");
            if line.trim().starts_with("ret") || (in_fn && line.contains(':') && !line.contains("heap_raw_alloc") && line.trim().ends_with(':')) {
                in_fn = false;
            }
        }
    }
    panic!("diagnostic done");
}

#[test]
fn qemu_05_functions_and_io() {
    let (_, outcome, uart) = run_hll_file("programs/test/qemu/05_functions_and_io.hll");
    assert_eq!(uart, "PASS\n", "expected UART='PASS\\n', got {uart:?}");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {outcome:?}"
    );
}
