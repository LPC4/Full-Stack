#![expect(
    clippy::collapsible_if,
    clippy::explicit_iter_loop,
    clippy::match_wildcard_for_single_variants,
    clippy::print_stderr,
    clippy::unwrap_used,
    reason = "manual VM diagnostic test emits traces and unwraps controlled fixtures"
)]

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::TargetMode;
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use virtual_machine::bus::ELF_LOAD_BASE;
use virtual_machine::virtual_machine::VirtualMachine;

// --- Kernel diagnostic: build the kernel via the import closure, step-trace the VM ---
// Run the test with nocapture to see the trace.
#[test]
fn kernel_asm_diag() {
    use virtual_machine::virtual_machine::StepOutcome;

    let stdlib_objs =
        CompilationPipeline::compile_stdlib_objects(TargetMode::Kernel).expect("stdlib compile");
    let mut kernel_objs =
        CompilationPipeline::compile_kernel_module_objects().expect("kernel module compile");
    // The kernel build marks `kmain` as an ABI export (kernel.build); the flat closure
    // leaves it local, so mark it here or `entry`'s external reference stays unresolved.
    for (_, obj) in &mut kernel_objs {
        if obj.has_symbol("kmain") {
            obj.mark_entry_global("kmain");
        }
    }

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_target_mode(TargetMode::Kernel);
    pipeline.set_write_artifacts(false);
    pipeline.set_entry_point(Some("_kernel_start".to_owned()));

    let mut modules: Vec<(&str, &AssembledOutput)> =
        stdlib_objs.iter().map(|(n, o)| (n.as_str(), o)).collect();
    for (name, obj) in &kernel_objs {
        modules.push((name.as_str(), obj));
    }
    let assembled = pipeline.link_assembled_objects(&modules).expect("link");
    let mut vm = VirtualMachine::new_kernel(&assembled);

    eprintln!("\n=== STEP TRACE (first 500000 steps) ===");
    let mut uart_so_far = String::new();
    for step in 0..500_000 {
        match vm.step() {
            Ok(StepOutcome::Halted(code)) => {
                eprintln!("  [step {step}] HALTED with code {code}");
                break;
            }
            Ok(StepOutcome::Continue) => {}
            Err(e) => {
                eprintln!("  [step {step}] ERROR: {e:?}");
                break;
            }
        }
        // drain UART each step so we can see where output stops
        let new_output = String::from_utf8_lossy(&vm.drain_uart_output()).into_owned();
        if !new_output.is_empty() {
            uart_so_far.push_str(&new_output);
            eprint!("{new_output}");
        }
    }

    eprintln!("\n=== UART OUTPUT SO FAR ===\n{uart_so_far}");
}

// --- Linking helper ---
// This is the canonical "link with stdlib" path used by the GUI and tests:
// 1. Compile stdlib once -> Vec<RvInstruction> token stream
// 2. Compile user source independently -> Vec<RvInstruction> token stream
// 3. Assemble stdlib + user into relocatable objects
// 4. Link the objects and inject the kernel layout symbols
// 5. Load the resulting ELF into VM and run
//
// To link with a different runtime (custom allocator, bare-metal glue, or a
// future C stdlib path) substitute step 1 with any Vec<RvInstruction> that
// defines malloc/free and whatever other symbols user code calls as external.
fn link_stdlib_and_run(user_src: &str) -> (String, Option<i64>) {
    use virtual_machine::virtual_machine::StepOutcome;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let stdlib_objs =
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted).expect("stdlib compile");
    let user_result = pipeline.compile(user_src).expect("user compile");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble");
    let mut modules: Vec<(&str, &AssembledOutput)> =
        stdlib_objs.iter().map(|(n, o)| (n.as_str(), o)).collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline.link_assembled_objects(&modules).expect("link");
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
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    // Compile each stdlib module independently; malloc lives in the allocator module.
    pipeline.set_type_prelude(get_stdlib_type_prelude());
    let mut tokens = Vec::new();
    for (_, src) in get_stdlib_modules_for_mode(TargetMode::Hosted).iter() {
        let result = pipeline.compile(src).expect("stdlib compile");
        let (_, t) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
        tokens.extend(t);
    }
    assert!(!tokens.is_empty(), "stdlib token stream must not be empty");
    let has_malloc = tokens.iter().any(|t| {
        use asm_to_binary::rv_instruction::RvInstruction;
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
    assert_eq!(
        uart, "-- compile-time math --\n7205512787ok\n",
        "compile-time example output changed"
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
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    // Compile each stdlib module independently and gather their token streams.
    pipeline.set_type_prelude(get_stdlib_type_prelude());
    let mut tokens = Vec::new();
    for (_, src) in get_stdlib_modules_for_mode(TargetMode::Hosted).iter() {
        let result = pipeline.compile(src).expect("stdlib compile");
        let (_, t) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
        tokens.extend(t);
    }
    use asm_to_binary::rv_instruction::RvInstruction;
    let has = |name: &str| {
        tokens
            .iter()
            .any(|t| matches!(t, RvInstruction::Label(n) if n == name))
    };
    assert!(has("putchar"), "stdlib must define putchar");
    assert!(has("puts"), "stdlib must define puts");
    assert!(has("print_int"), "stdlib must define print_int");
    assert!(has("printf"), "stdlib must define printf");
    assert!(has("exit"), "stdlib must define exit");
    assert!(has("_start"), "stdlib must define _start");
}

// Verify puts writes a null-terminated string plus newline.
#[test]
fn puts_basic() {
    let src = r#"
external puts: (str: u8*) -> i32

main: () -> i32 {
    puts("Hi".ptr)
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
