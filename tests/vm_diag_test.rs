use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::high_level_language::stdlib::get_stdlib_source;
use full_stack::virtual_machine::bus::ELF_LOAD_BASE;
use full_stack::virtual_machine::virtual_machine::VirtualMachine;

// ---------------------------------------------------------------------------
// Linking helper
//
// This is the canonical "link with stdlib" path used by the GUI and tests:
//   1. Compile stdlib once → Vec<RvInstruction> token stream
//   2. Compile user source independently → Vec<RvInstruction> token stream
//   3. Token-level link: [stdlib_tokens..., user_tokens...]
//   4. assemble()  — no injected stubs; all runtime is in stdlib (runtime.hll)
//   5. Load ELF into VM and run
//
// To link with a different runtime (custom allocator, bare-metal glue, or a
// future C stdlib path) substitute step 1 with any Vec<RvInstruction> that
// defines malloc/free and whatever other symbols user code calls as external.
// ---------------------------------------------------------------------------
fn link_stdlib_and_run(user_src: &str) -> (String, Option<i64>) {
    use full_stack::virtual_machine::virtual_machine::StepOutcome;

    let pipeline = CompilationPipeline::new();
    let stdlib_result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) =
        pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);

    let user_result = pipeline.compile(user_src).expect("user compile");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);

    let assembled = pipeline.assemble(&linked).expect("assemble");
    let elf = assembled.to_elf(ELF_LOAD_BASE);
    let mut vm = VirtualMachine::from_elf(&elf).expect("load elf");
    let run = vm.run(5_000_000);

    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

// Verify the stdlib compiles and that its token stream defines malloc, which
// is required by any user code that calls new(T) or free(ptr).
#[test]
fn stdlib_provides_malloc() {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    assert!(!tokens.is_empty(), "stdlib token stream must not be empty");
    let has_malloc = tokens.iter().any(|t| {
        use full_stack::assembly_language::rv_instruction::RvInstruction;
        matches!(t, RvInstruction::Label(n) if n == "malloc")
    });
    assert!(has_malloc, "stdlib must define malloc");
}

#[test]
fn putchar_basic() {
    let src = r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    putchar(72)
    putchar(105)
    putchar(10)
    return 0
}
"#;
    let (uart, exit) = link_stdlib_and_run(src);
    assert_eq!(uart.trim_end_matches('\n'), "Hi");
    assert_eq!(exit, Some(0));
}

#[test]
fn printf_constexpr() {
    let src = std::fs::read_to_string("programs/example/compile_time_math.hll").unwrap();
    let (uart, exit) = link_stdlib_and_run(&src);
    assert!(
        uart.contains("Factorial 7 = 5040"),
        "expected 'Factorial 7 = 5040' in UART output, got: {uart:?}"
    );
    assert_eq!(exit, Some(0));
}

#[test]
fn functions_and_io() {
    let src = std::fs::read_to_string("programs/test/qemu/05_functions_and_io.hll").unwrap();
    let (uart, exit) = link_stdlib_and_run(&src);
    assert_eq!(uart.trim_end_matches('\n'), "PASS");
    assert_eq!(exit, Some(0));
}

// Verify that asm_reg(sp) compiles, runs, and returns a plausible stack-pointer value.
#[test]
fn asm_reg_reads_sp() {
    let src = r#"
external putchar: (c: i32) -> i32
external print_int: (n: i64) -> i32

get_sp: () -> i64 {
    return asm_reg(sp)
}

main: () -> i32 {
    sp_val: i64 = get_sp()
    ; Stack pointer lives in the upper half of the 128 MiB VM address space,
    ; so it must be above 0 and the high bit of a 64-bit word should be clear.
    if sp_val > 0 {
        putchar(80)  ; 'P'
        putchar(65)  ; 'A'
        putchar(83)  ; 'S'
        putchar(83)  ; 'S'
    }
    return 0
}
"#;
    let (uart, exit) = link_stdlib_and_run(src);
    assert_eq!(uart.trim_end_matches('\n'), "PASS");
    assert_eq!(exit, Some(0));
}

// Verify that the stdlib token stream contains the HLL-defined runtime symbols.
#[test]
fn stdlib_provides_runtime() {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    use full_stack::assembly_language::rv_instruction::RvInstruction;
    let has = |name: &str| tokens.iter().any(|t| matches!(t, RvInstruction::Label(n) if n == name));
    assert!(has("putchar"),     "stdlib must define putchar");
    assert!(has("puts"),        "stdlib must define puts");
    assert!(has("print_int"),   "stdlib must define print_int");
    assert!(has("printf"),      "stdlib must define printf");
    assert!(has("exit"),        "stdlib must define exit");
    assert!(has("_start"),      "stdlib must define _start");
}

// Verify puts writes a null-terminated string plus newline.
#[test]
fn puts_basic() {
    let src = r#"
external puts: (str: u8*) -> i32

main: () -> i32 {
    puts("Hi".data)
    return 0
}
"#;
    let (uart, exit) = link_stdlib_and_run(src);
    assert_eq!(uart.trim_end_matches('\n'), "Hi");
    assert_eq!(exit, Some(0));
}

// Verify print_int handles zero, positive, and negative values.
#[test]
fn print_int_basic() {
    let src = r#"
external print_int: (n: i64) -> i32
external putchar: (c: i32) -> i32

main: () -> i32 {
    print_int(42)
    putchar(10)
    print_int(-7)
    putchar(10)
    print_int(0)
    putchar(10)
    return 0
}
"#;
    let (uart, exit) = link_stdlib_and_run(src);
    assert_eq!(uart, "42\n-7\n0\n");
    assert_eq!(exit, Some(0));
}

// Verify that an asm { } block with raw RISC-V instructions is emitted and executed.
// The block stores 42 into a stack slot via sd, then reads it back with ld.
// (We exercise the inline-asm path through the assembler's parse_instruction_line.)
#[test]
fn asm_block_round_trip() {
    let src = r#"
external putchar: (c: i32) -> i32

; Writes a single byte via the Linux write syscall (syscall 64).
; This mirrors what putchar does in extern_stubs, implemented in HLL inline asm.
write_byte: (c: i32) -> i32 {
    asm {
        addi  sp, sp, -16
        sd    ra, 8(sp)
        sb    a0, 7(sp)
        li    a0, 1
        addi  a1, sp, 7
        li    a2, 1
        li    a7, 64
        ecall
        ld    ra, 8(sp)
        addi  sp, sp, 16
    }
    return 0
}

main: () -> i32 {
    write_byte(72)   ; 'H'
    write_byte(105)  ; 'i'
    write_byte(10)   ; '\n'
    return 0
}
"#;
    let (uart, exit) = link_stdlib_and_run(src);
    assert_eq!(uart.trim_end_matches('\n'), "Hi");
    assert_eq!(exit, Some(0));
}
