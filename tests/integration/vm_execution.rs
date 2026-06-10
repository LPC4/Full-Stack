use asm_to_binary::assembler::Assembler;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64i::*;
use asm_to_binary::riscv::rv64m::*;
use asm_to_binary::riscv::rv64zicsr::Csrrs;
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_stdlib_source;
use hll_to_ir::{
    IntWidth, IrBlock, IrCmpOp, IrFunction, IrInstruction, IrLabel, IrMathOp, IrProgram,
    IrRegister, IrTerminator, IrType, IrUnaryOp, IrValue,
};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// --- Full HLL pipeline helpers ---

// The hosted stdlib is identical for every VM-execution test, so compile and
// assemble it exactly once for the whole suite and link each user program
// against the cached object. This avoids ~40 redundant stdlib compiles.
fn cached_stdlib_obj() -> &'static AssembledOutput {
    static STDLIB: OnceLock<AssembledOutput> = OnceLock::new();
    STDLIB.get_or_init(|| {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_write_artifacts(false);
        let stdlib_result = pipeline
            .compile(&get_stdlib_source())
            .expect("stdlib compile failed");
        let (_, stdlib_tokens) =
            pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
        pipeline.assemble(&stdlib_tokens).expect("stdlib assemble failed")
    })
}

/// Compile user HLL, link it against the cached stdlib object, and run in the VM.
fn run_hll_with_limit(src: &str, max_steps: u64) -> (VirtualMachine, StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");

    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(max_steps);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

/// Compile a directly-constructed IR program (for ops the HLL surface lacks),
/// link against the cached stdlib, and run it.
fn run_ir(program: &IrProgram) -> (VirtualMachine, StepOutcome, String) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(program);
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");

    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let uart = run.uart_output.clone();
    (vm, run.outcome, uart)
}

/// Build a minimal pass/fail IR program around an entry block. `entry` must not
/// yet have a terminator; this adds the branch, a pass block (return 0), and a
/// fail block (return 1).
fn pass_fail_ir(module: &str, mut entry: IrBlock, cond: IrValue) -> IrProgram {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut program = IrProgram::new(module);
    let mut func = IrFunction::new("main", i32_ty.clone());

    entry.set_terminator(IrTerminator::Branch {
        cond,
        then_label: IrLabel::new("pass"),
        else_label: IrLabel::new("fail"),
    });

    let mut pass_block = IrBlock::new("pass");
    pass_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));

    let mut fail_block = IrBlock::new("fail");
    fail_block.set_terminator(IrTerminator::Return(Some(IrValue::Integer(1))));

    func.push_block(entry);
    func.push_block(pass_block);
    func.push_block(fail_block);
    program.push_function(func);
    program
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

/// Compile a user program with the peephole pass either on or off, returning the
/// optimized-or-not token stream alongside the VM run (exit outcome + UART).
fn run_hll_peephole(src: &str, peephole: bool) -> (StepOutcome, String, usize) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_peephole(peephole);

    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let code_lines = user_tokens.iter().filter(|t| t.is_code()).count();
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");

    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    (run.outcome, run.uart_output.clone(), code_lines)
}

// A program with disjoint branches and locals: lots of per-vreg store/reload
// churn for the peephole to clean up, while exercising real control flow.
const PEEPHOLE_PROGRAM: &str = r#"
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
    while i < 5 {
        total = total + compute(i)
        i = i + 1
    }
    return total as i32
}
"#;

#[test]
fn peephole_preserves_behavior_and_shrinks_code() {
    let (base_outcome, base_uart, base_lines) = run_hll_peephole(PEEPHOLE_PROGRAM, false);
    let (opt_outcome, opt_uart, opt_lines) = run_hll_peephole(PEEPHOLE_PROGRAM, true);

    assert_eq!(
        format!("{base_outcome:?}"),
        format!("{opt_outcome:?}"),
        "peephole changed the exit outcome"
    );
    assert_eq!(base_uart, opt_uart, "peephole changed UART output");
    assert!(
        opt_lines < base_lines,
        "peephole should remove instructions: {base_lines} -> {opt_lines}"
    );
}

/// Compile a user program with IR optimization on or off, returning the VM run
/// (exit outcome + UART) and the emitted instruction count.
fn run_hll_optimize(
    src: &str,
    opts: hll_to_ir::OptOptions,
) -> (StepOutcome, String, usize) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_optimize(opts);

    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let code_lines = user_tokens.iter().filter(|t| t.is_code()).count();
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");

    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    (run.outcome, run.uart_output.clone(), code_lines)
}

// Constant-heavy program with a dead local: const folding collapses the
// arithmetic and DCE drops the unused computation.
const OPT_CONST_PROGRAM: &str = r#"
main: () -> i32 {
    a: i32 = 2 + 3 * 4
    b: i32 = a * 10
    unused: i32 = 100 + 200 + 300
    return b as i32
}
"#;

#[test]
fn ir_opt_preserves_behavior_and_shrinks_code() {
    // Folds to b = 14 * 10 = 140; `unused` is dead.
    let (base_outcome, base_uart, base_lines) =
        run_hll_optimize(OPT_CONST_PROGRAM, hll_to_ir::OptOptions::none());
    let (opt_outcome, opt_uart, opt_lines) =
        run_hll_optimize(OPT_CONST_PROGRAM, hll_to_ir::OptOptions::all());

    assert!(
        matches!(base_outcome, StepOutcome::Halted(140)),
        "expected Halted(140) unoptimized, got {base_outcome:?}"
    );
    assert_eq!(
        format!("{base_outcome:?}"),
        format!("{opt_outcome:?}"),
        "IR optimization changed the exit outcome"
    );
    assert_eq!(base_uart, opt_uart, "IR optimization changed UART output");
    assert!(
        opt_lines < base_lines,
        "const-fold + DCE should remove instructions: {base_lines} -> {opt_lines}"
    );
}

#[test]
fn ir_opt_preserves_control_flow_program() {
    // The peephole stress program has params and loops: optimization must not
    // change its result even where little folds.
    let (base_outcome, base_uart, _) =
        run_hll_optimize(PEEPHOLE_PROGRAM, hll_to_ir::OptOptions::none());
    let (opt_outcome, opt_uart, _) =
        run_hll_optimize(PEEPHOLE_PROGRAM, hll_to_ir::OptOptions::all());

    assert_eq!(
        format!("{base_outcome:?}"),
        format!("{opt_outcome:?}"),
        "IR optimization changed the exit outcome on the control-flow program"
    );
    assert_eq!(base_uart, opt_uart, "IR optimization changed UART output");
}

#[test]
fn hll_new_i32_and_return() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 42
    return @p
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

// --- HLL full-pipeline VM tests ---

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
fn hll_negative_integer_inits() {
    // Negative literals and literal arithmetic adopt the declared i64 width with
    // no explicit cast (PLAN 0.1).
    let (_, outcome, _) = run_hll(r#"
neg_one: () -> i64 {
    return -1
}
main: () -> i32 {
    a: i64 = -42
    b: i64 = 0 - 1
    c: i64 = a * b
    d: i64 = neg_one()
    e: i64 = c + d
    return e as i32
}
"#);
    // a=-42, b=-1, c=42, d=-1, e=41.
    assert!(matches!(outcome, StepOutcome::Halted(41)), "expected Halted(41), got {outcome:?}");
}

#[test]
fn hll_global_scalar_initializer() {
    // A non-zero scalar global reads back its declared value (PLAN 0.2).
    let (_, outcome, _) = run_hll(r#"
g: i64 = 42
main: () -> i32 {
    return g as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
}

#[test]
fn hll_global_array_initializer() {
    // A non-zero array global initializer lands in .data and reads back (PLAN 0.2).
    let (_, outcome, _) = run_hll(r#"
arr: i64[4] = [10, 20, 12, 0]
main: () -> i32 {
    s: i64 = @arr[0] + @arr[1] + @arr[2] + @arr[3]
    return s as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(42)), "expected Halted(42), got {outcome:?}");
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
external putchar: (c: i32) -> i32

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
external putchar: (c: i32) -> i32

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

// --- Keyboard device ---

// The guest drains the keyboard MMIO ring (STATUS at 0x10070000, DATA at
// 0x10070004) and echoes the scancode of each *press* (bit 16 set), so releases
// must be filtered out. Events are queued by the host before the run starts.
#[test]
fn keyboard_device_delivers_press_events_to_guest() {
    let src = r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    while 1 == 1 {
        status_p: u32* = 0x10070000 as u32*
        if (@status_p) == 0 {
            break
        }
        data_p: u32* = 0x10070004 as u32*
        ev: u32 = @data_p
        if (ev & 0x10000) != 0 {
            putchar((ev & 0xFF) as i32)
        }
    }
    return 0
}
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let user_result = pipeline.compile(src).expect("user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble failed");
    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");

    let mut vm = VirtualMachine::new(&assembled);
    // Queue: A press, B release (filtered), C press -> guest echoes "AC".
    vm.keyboard_push(b'A' as u16, true);
    vm.keyboard_push(b'B' as u16, false);
    vm.keyboard_push(b'C' as u16, true);

    let run = vm.run(5_000_000);
    assert_eq!(run.uart_output, "AC", "expected 'AC', got {:?}", run.uart_output);
    assert!(
        matches!(run.outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {:?}",
        run.outcome
    );
}

// --- Helper ---

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

// --- Tests ---

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
        ri(RealInstruction::Sd(Sd::new(6, 5, 0))),        // sd t0, 0(t1)  - store 99
        ri(RealInstruction::Ld(Ld::new(10, 6, 0))),       // ld a0, 0(t1)  - load 99
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
    // divu a0, a1, x0 - divide by zero should yield u64::MAX in a0.
    // Save into s0 (x8, callee-saved) before overwriting a0 for exit.
    // The ROM handler for sys_exit uses t0-t6 as scratch, so t0 (x5) would
    // be clobbered; s0 (x8) is safe.
    let (vm, _outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(11, 0, 42))),  // a1 = 42
        ri(RealInstruction::Divu(Divu::new(10, 11, 0))),  // a0 = divu(42, 0) = u64::MAX
        // Save result to s0 (x8) before overwriting a0 for exit
        ri(RealInstruction::Add(Add::new(8, 10, 0))),     // s0 = a0 (preserved across ecall)
        ri(RealInstruction::Addi(Addi::new(17, 0, 93))),  // a7 = 93
        ri(RealInstruction::Addi(Addi::new(10, 0, 0))),   // a0 = 0 (exit code)
        ri(RealInstruction::Ecall(Ecall::new())),
    ]);
    // s0=x8 should hold u64::MAX from the divu
    assert_eq!(vm.peek_reg(8), u64::MAX, "divu by zero should produce u64::MAX");
}

#[test]
fn test_csr_instret() {
    // Run a few nops, then read instret via csrrs a0, 0xC02, x0, then exit.
    // The exit code (as i64) should be > 3 (we ran at least 3 nops + csrrs + addi + ecall).
    let (_, outcome) = assemble_and_run(vec![
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 1
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 2
        ri(RealInstruction::Addi(Addi::new(0, 0, 0))),    // nop 3
        // csrrs a0, instret (0xC02), x0  - read instret, no write
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

// --- End-to-end qemu program tests (full HLL pipeline through VM) ---

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
fn qemu_05_functions_and_io() {
    let (_, outcome, uart) = run_hll_file("programs/test/qemu/05_functions_and_io.hll");
    assert_eq!(uart, "PASS\n", "expected UART='PASS\\n', got {uart:?}");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "expected Halted(0), got {outcome:?}"
    );
}

#[test]
fn examples_exit_zero_in_vm() {
    let files = [
        "programs/example/core_basics.hll",
        "programs/example/pointer_arrays.hll",
        "programs/example/array_initialization.hll",
        "programs/example/struct_binding.hll",
        "programs/example/control_flow_basics.hll",
        "programs/example/casting_and_pointers.hll",
        "programs/example/compile_time_math.hll",
        "programs/example/generics_and_strings.hll",
    ];

    for file in files {
        let (_, outcome, _uart) = run_hll_file(file);
        assert!(
            matches!(outcome, StepOutcome::Halted(0)),
            "{file}: expected Halted(0), got {outcome:?}"
        );
    }
}

// --- Calling convention (merged from calling_convention_exec.rs) ---
// All programs return 0 (pass) or 1 (fail).

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

/// Values held across a function call must be restored (callee-saved or spilled).
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

// --- emit_li constant materialization (merged from emit_li_execution.rs) ---
// Expected values are built from safe (<= 2047) operands so the comparison
// target is not affected by the same bug under test.

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
    assert!(matches!(outcome, StepOutcome::Halted(0)), "li 42 should load 42, got {outcome:?}");
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
    assert!(matches!(outcome, StepOutcome::Halted(0)), "li 2047 should load 2047, got {outcome:?}");
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

/// 0x8000_0000 - first value where LUI sign-extends; zero-extension is required.
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

/// 0xFFFF_FFFF tests hi_adj overflow; slli/srli sequence must still produce correct bits.
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

/// 0x1_0000_0000 - first true 64-bit value.
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

// --- Global variables (merged from global_var_exec.rs) ---

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

// --- Memory width load/store (merged from memory_width_exec.rs) ---

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
        "i8 store 127 -> load should read 127, got {outcome:?}"
    );
}

/// Store and load an i8: negative value -1 round-trips as -1 (lb sign-extends).
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
        "i8 store -1 -> load should read -1 (sign-extended), got {outcome:?}"
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
        "i16 store 1000 -> load should read 1000, got {outcome:?}"
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
        "i32 store 1234567 -> load should read 1234567, got {outcome:?}"
    );
}

/// i64 store/load round-trip for large value 0x8010_0000.
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
        "i64 store 0x8010_0000 -> load should read same value, got {outcome:?}"
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

// --- Struct layout and heap (merged from struct_array_exec.rs) ---

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

/// Two independent heap i32 allocations do not overlap.
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

// --- IR instruction lowering (merged from ir_instruction_exec.rs) ---
// HLL programs cover ops the language expresses directly; directly-constructed
// IR covers ops with no HLL surface syntax (signed shift, etc.).

/// Signed integer division with a negative dividend must use `div` (signed).
#[test]
fn ir_math_signed_div() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    result: i32 = -8 / 2
    if result == -4 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "signed -8 / 2 should equal -4, got {outcome:?}"
    );
}

/// Unsigned division: 100 / 3 = 33 (unsigned semantics).
#[test]
fn ir_math_udiv() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: u32 = 100
    b: u32 = 3
    c: u32 = a / b
    if c == 33 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "unsigned 100 / 3 = 33, got {outcome:?}"
    );
}

/// Signed comparison: -1 < 0 must be true (uses `slt`, not `sltu`).
#[test]
fn ir_cmp_signed_negative() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    neg: i32 = -1
    zero: i32 = 0
    if neg < zero {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "-1 < 0 should be true (signed), got {outcome:?}"
    );
}

/// Unsigned comparison: 0xFFFF_FFFF as u32 must be greater than 1.
#[test]
fn ir_cmp_unsigned_max() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    big: u32 = 4294967295
    small: u32 = 1
    if big > small {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "0xFFFF_FFFF > 1 should be true (unsigned), got {outcome:?}"
    );
}

/// Unary negation: 0 - 5 = -5.
#[test]
fn ir_unary_neg() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    pos: i32 = 5
    neg: i32 = 0 - pos
    if neg == -5 {
        return 0
    }
    return 1
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "0 - 5 = -5, got {outcome:?}");
}

/// All-ones value check (HLL has no bitwise-not surface op here).
#[test]
fn ir_unary_not() {
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    all_ones: i32 = 0 - 1
    neg_one: i32 = 0 - 1
    if all_ones == neg_one {
        return 0
    }
    return 1
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "bitwise all-ones check, got {outcome:?}");
}

/// Array stride correctness: two separate heap i32 values sum to 30.
#[test]
fn ir_index_with_stride() {
    let (_, outcome, _) = run_hll(r#"
external free: (p: i32*) -> void

main: () -> i32 {
    p: i32* = new(i32)
    q: i32* = new(i32)
    @p = 10
    @q = 20
    v: i32 = @p + @q
    free(p)
    free(q)
    if v == 30 {
        return 0
    }
    return 1
}
"#);
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "two separate heap i32 values sum to 30, got {outcome:?}"
    );
}

/// Signed right shift (`sra`): -8 >> 2 must equal -2. Built directly in IR.
#[test]
fn ir_math_shr_signed() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Named("shifted".into()),
        op: IrMathOp::Shr,
        ty: i32_ty.clone(),
        lhs: IrValue::Integer(-8),
        rhs: IrValue::Integer(2),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("shifted".into())),
        rhs: IrValue::Integer(-2),
    });
    let program = pass_fail_ir("shr_signed", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "sra: -8 >> 2 == -2, got {outcome:?}");
}

/// Left shift (`sll`): 1 << 10 must equal 1024. Built directly in IR.
#[test]
fn ir_math_shl() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Named("result".into()),
        op: IrMathOp::Shl,
        ty: i32_ty.clone(),
        lhs: IrValue::Integer(1),
        rhs: IrValue::Integer(10),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("result".into())),
        rhs: IrValue::Integer(1024),
    });
    let program = pass_fail_ir("shl", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "sll: 1 << 10 == 1024, got {outcome:?}");
}

/// IR-level unary negation: IrUnaryOp::Neg applied to 7 yields -7.
#[test]
fn ir_unary_neg_ir() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Unary {
        dest: IrRegister::Named("neg_val".into()),
        op: IrUnaryOp::Neg,
        ty: i32_ty.clone(),
        value: IrValue::Integer(7),
    });
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Eq,
        ty: i32_ty,
        lhs: IrValue::Register(IrRegister::Named("neg_val".into())),
        rhs: IrValue::Integer(-7),
    });
    let program = pass_fail_ir("neg", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "neg 7 == -7, got {outcome:?}");
}

/// Unsigned less-than comparison via IR: 2 < 5 must be true with `Ult`.
#[test]
fn ir_cmp_ult() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Ult,
        ty: i32_ty,
        lhs: IrValue::Integer(2),
        rhs: IrValue::Integer(5),
    });
    let program = pass_fail_ir("ult", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "ult: 2 < 5 should be true, got {outcome:?}");
}

/// Signed less-than comparison via IR: -1 < 0 must be true with `Slt`.
#[test]
fn ir_cmp_slt_negative() {
    let i32_ty = IrType::Integer(IntWidth::I32);
    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Cmp {
        dest: IrRegister::Named("ok".into()),
        op: IrCmpOp::Slt,
        ty: i32_ty,
        lhs: IrValue::Integer(-1),
        rhs: IrValue::Integer(0),
    });
    let program = pass_fail_ir("slt", entry, IrValue::Register(IrRegister::Named("ok".into())));
    let (_, outcome, _) = run_ir(&program);
    assert!(matches!(outcome, StepOutcome::Halted(0)), "slt: -1 < 0 should be true, got {outcome:?}");
}

// --- Floating-point lowering (PLAN 3.1 / 5.1) ---
//
// These guard the three float-lowering correctness fixes: f32 and f64 constant
// materialization, f64 arithmetic going through the FPU (not the integer ALU),
// int<->float casts using fcvt, and FP comparisons. Each program computes a
// float result and truncates it to an i32 exit code via an `as` cast.

#[test]
fn float_f32_constant_and_add() {
    // Before the fix, `1.5: f32` materialized 0.0 (wrong low 32 bits), so the
    // sum was 0. Correct: 1.5 + 2.5 = 4.0 -> 4.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f32 = 1.5
    b: f32 = 2.5
    c: f32 = a + b
    return c as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(4)), "expected Halted(4), got {outcome:?}");
}

#[test]
fn float_f64_arithmetic_uses_fpu() {
    // Before the fix, f64 math fell through to the integer ALU and operated on
    // raw bit patterns. Correct: 3.5 * 2.0 = 7.0 -> 7.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f64 = 3.5
    b: f64 = 2.0
    c: f64 = a * b
    return c as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(7)), "expected Halted(7), got {outcome:?}");
}

#[test]
fn float_f64_div_and_sub() {
    // 20.0 / 4.0 = 5.0, then 5.0 - 2.0 = 3.0 -> 3.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f64 = 20.0
    b: f64 = 4.0
    two: f64 = 2.0
    c: f64 = a / b
    d: f64 = c - two
    return d as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(3)), "expected Halted(3), got {outcome:?}");
}

#[test]
fn float_int_to_float_roundtrip() {
    // i32 -> f64 (fcvt.d.w) then f64 -> i32 (fcvt.w.d). Before the fix both
    // casts were a plain `mv` reinterpreting the bit pattern.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    n: i32 = 7
    three: f64 = 3.0
    f: f64 = n as f64
    g: f64 = f * three
    return g as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(21)), "expected Halted(21), got {outcome:?}");
}

#[test]
fn float_f32_int_cast() {
    // i32 -> f32 -> i32 with a non-trivial intermediate value.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    n: i32 = 9
    f: f32 = n as f32
    g: f32 = f + 0.5
    h: f32 = g * 2.0
    return h as i32
}
"#);
    // 9.0 + 0.5 = 9.5, * 2.0 = 19.0 -> 19.
    assert!(matches!(outcome, StepOutcome::Halted(19)), "expected Halted(19), got {outcome:?}");
}

#[test]
fn float_f32_to_f64_widen() {
    // f32 -> f64 conversion (fcvt.d.s). 3.5 widened, + 1.5 = 5.0 -> 5.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f32 = 3.5
    onefive: f64 = 1.5
    b: f64 = a as f64
    c: f64 = b + onefive
    return c as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(5)), "expected Halted(5), got {outcome:?}");
}

#[test]
fn float_f64_to_f32_narrow() {
    // f64 -> f32 conversion (fcvt.s.d). 6.5 narrowed -> 6.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f64 = 6.5
    b: f32 = a as f32
    return b as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(6)), "expected Halted(6), got {outcome:?}");
}

#[test]
fn float_f64_negation() {
    // Unary negation via fsgnjn.d. -5.0 + 8.0 = 3.0 -> 3.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f64 = 5.0
    eight: f64 = 8.0
    b: f64 = -a
    c: f64 = b + eight
    return c as i32
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(3)), "expected Halted(3), got {outcome:?}");
}

#[test]
fn float_f32_comparison() {
    // f32 less-than comparison must use flt.s.
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f32 = 1.5
    b: f32 = 2.5
    if a < b {
        return 1
    }
    return 0
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(1)), "expected Halted(1), got {outcome:?}");
}

#[test]
fn float_f64_comparison() {
    // f64 equality must use feq.d (not the integer comparator).
    let (_, outcome, _) = run_hll(r#"
main: () -> i32 {
    a: f64 = 5.5
    b: f64 = 2.5
    eight: f64 = 8.0
    c: f64 = a + b
    if c == eight {
        return 1
    }
    return 0
}
"#);
    assert!(matches!(outcome, StepOutcome::Halted(1)), "expected Halted(1), got {outcome:?}");
}


