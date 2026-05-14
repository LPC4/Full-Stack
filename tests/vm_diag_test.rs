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
//   4. assemble() (appends extern_stubs: putchar/printf/exit/_start)
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
