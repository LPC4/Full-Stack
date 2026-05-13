use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::high_level_language::stdlib::{extract_registries, get_stdlib_source};
use full_stack::virtual_machine::bus::ELF_LOAD_BASE;
use full_stack::virtual_machine::virtual_machine::VirtualMachine;

const ALLOCATOR_TYPES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/types.hll"
));
const ALLOCATOR_MEMORY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/memory_allocator.hll"
));

fn prepend_allocator_runtime(source: &str) -> String {
    let mut out = String::with_capacity(ALLOCATOR_TYPES.len() + ALLOCATOR_MEMORY.len() + source.len() + 128);
    out.push_str(ALLOCATOR_TYPES);
    out.push('\n');
    out.push_str(ALLOCATOR_MEMORY);
    out.push('\n');
    out.push_str(source);
    out
}

#[test]
fn vm_diag_simple() {
    let src = r#"
main: () -> i32 {
    a: i32 = 6
    b: i32 = 7
    return a * b
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(src).expect("compile");
    let (_, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(1_000_000);

    eprintln!("UART: {:?}", run.uart_output);
    eprintln!("Outcome: {:?}", run.outcome);
    eprintln!("Steps: {}", run.steps);
}

#[test]
fn vm_diag_printf() {
    let src = std::fs::read_to_string("programs/test/qemu/05_functions_and_io.hll").unwrap();
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src).expect("compile");
    let (_, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);

    eprintln!("UART:\n{}", run.uart_output);
    eprintln!("Outcome: {:?}", run.outcome);
    eprintln!("Steps: {}", run.steps);
}

#[test]
fn vm_diag_printf_symbols() {
    let src = std::fs::read_to_string("programs/test/qemu/05_functions_and_io.hll").unwrap();
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src).expect("compile");
    let (asm_text, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    eprintln!(
        "=== ASSEMBLY ===\n{}",
        &asm_text[..asm_text.len().min(3000)]
    );
    eprintln!("=== SYMBOL TABLE ===");
    let mut syms: Vec<_> = assembled.symbol_table.iter().collect();
    syms.sort_by_key(|(_, v)| *v);
    for (k, v) in &syms {
        eprintln!("  {:#010x}  {}", v, k);
    }
    eprintln!("=== SECTIONS ===");
    for s in &assembled.sections {
        eprintln!("  {:?}: {} bytes", s.kind, s.bytes.len());
    }
}

#[test]
fn vm_diag_generics_strings() {
    let raw = std::fs::read_to_string("programs/example/generics_and_strings.hll").unwrap();
    let pipeline = CompilationPipeline::new();
    let result = pipeline
        .compile(&prepend_allocator_runtime(&raw))
        .expect("compile");

    let (asm_text, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    eprintln!("=== ASSEMBLY ===\n{}", asm_text);
    eprintln!("=== SYMBOL TABLE ===");
    let mut syms: Vec<_> = assembled.symbol_table.iter().collect();
    syms.sort_by_key(|(_, v)| *v);
    for (k, v) in &syms {
        eprintln!("  {:#010x}  {}", v, k);
    }

    // Check rodata section
    use full_stack::assembly_language::assembler::section::SectionKind;
    let rodata = assembled.section_bytes(&SectionKind::RoData);
    eprintln!("\n=== RODATA SECTION ({} bytes) ===", rodata.len());
    for (i, chunk) in rodata.chunks(16).enumerate() {
        let hex: String = chunk.iter().map(|b| format!("{:02x} ", b)).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if b >= 32 && b < 127 { b as char } else { '.' })
            .collect();
        eprintln!("  {:04x}: {:<48} {}", i * 16, hex, ascii);
    }

    // Run it
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(1_000_000);
    eprintln!("\n=== EXECUTION RESULTS ===");
    eprintln!("UART:\n{}", run.uart_output);
    eprintln!("Outcome: {:?}", run.outcome);
    eprintln!("Steps: {}", run.steps);
}

#[test]
fn vm_diag_call_encoding() {
    // Simple: one function that calls another
    let src = r#"
external putchar: (c: i32) -> i32

emit: (c: i32) -> i32 {
    putchar(c)
    return 0
}
main: () -> i32 {
    emit(65)
    return 0
}
"#;
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src).expect("compile");
    let (asm_text, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    eprintln!("=== ASSEMBLY ===\n{}", asm_text);
    eprintln!("=== SYMBOL TABLE ===");
    let mut syms: Vec<_> = assembled.symbol_table.iter().collect();
    syms.sort_by_key(|(_, v)| *v);
    for (k, v) in &syms {
        eprintln!("  {:#010x}  {}", v, k);
    }

    // Run it
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(100_000);
    eprintln!("UART: {:?}", run.uart_output);
    eprintln!("Outcome: {:?}", run.outcome);
    eprintln!("Steps: {}", run.steps);
}

#[test]
fn vm_diag_malloc_loop() {
    let src = prepend_allocator_runtime(
        r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 42
    v: i32 = @p
    free(p)
    return v
}
"#,
    );
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&src).expect("compile");
    let (asm_text, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    let assembled = pipeline.assemble(&toks).expect("assemble");

    eprintln!("=== SYMBOL TABLE ===");
    let mut syms: Vec<_> = assembled.symbol_table.iter().collect();
    syms.sort_by_key(|(_, v)| *v);
    for (k, v) in &syms {
        eprintln!("  {:#010x}  {}", v, k);
    }

    let _ = asm_text;
    let mut vm = VirtualMachine::new(&assembled);

    // Run step-by-step and print PC transitions
    let mut prev_pc = 0u64;
    let mut same_count = 0u64;
    for i in 0..200_000u64 {
        let pc = vm.peek_pc();
        if pc == prev_pc {
            same_count += 1;
            if same_count > 100 {
                eprintln!(
                    "STUCK at PC {:#x} for {} steps (step {})",
                    pc, same_count, i
                );
                break;
            }
        } else {
            same_count = 0;
        }
        prev_pc = pc;
        match vm.step() {
            Ok(full_stack::virtual_machine::virtual_machine::StepOutcome::Halted(code)) => {
                eprintln!("Halted({}) at step {}, PC={:#x}", code, i, pc);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error at step {}: {:?}", i, e);
                break;
            }
        }
    }
    eprintln!(
        "UART: {:?}",
        String::from_utf8_lossy(&vm.drain_uart_output())
    );
}

// ---------------------------------------------------------------------------
// GUI-path tests: replicate the token-level linking approach used in app.rs
// ---------------------------------------------------------------------------

/// Helper: compile stdlib once, then compile user source with extern seeding,
/// concatenate the two token streams, assemble, run in VM. Returns UART output.
fn run_gui_path(user_src: &str) -> (String, Option<i64>) {
    use full_stack::virtual_machine::virtual_machine::StepOutcome;

    let pipeline = CompilationPipeline::new();
    let stdlib_src = get_stdlib_source();
    let stdlib_result = pipeline.compile(&stdlib_src).expect("stdlib compile");
    let (fn_reg, ty_reg) = extract_registries(&stdlib_result.ir_program);
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);

    let user_result = pipeline
        .compile_with_externs(user_src, &fn_reg, &ty_reg)
        .expect("user compile");
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

#[test]
fn gui_path_putchar_basic() {
    let src = r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    putchar(72)
    putchar(105)
    putchar(10)
    return 0
}
"#;
    let (uart, exit) = run_gui_path(src);
    eprintln!("UART: {:?}", uart);
    eprintln!("Exit: {:?}", exit);
    assert_eq!(uart.trim_end_matches('\n'), "Hi", "expected 'Hi' on UART");
    assert_eq!(exit, Some(0));
}

#[test]
fn gui_path_printf_constexpr() {
    let src = std::fs::read_to_string("programs/example/compile_time_math.hll").unwrap();
    let (uart, exit) = run_gui_path(&src);
    eprintln!("UART:\n{}", uart);
    eprintln!("Exit: {:?}", exit);
    assert!(
        uart.contains("Factorial 7 = 5040"),
        "expected 'Factorial 7 = 5040' in UART, got: {:?}",
        uart
    );
    assert_eq!(exit, Some(0));
}

#[test]
fn gui_path_functions_and_io() {
    let src = std::fs::read_to_string("programs/test/qemu/05_functions_and_io.hll").unwrap();
    let (uart, exit) = run_gui_path(&src);
    eprintln!("UART: {:?}", uart);
    eprintln!("Exit: {:?}", exit);
    assert_eq!(uart.trim_end_matches('\n'), "PASS", "expected PASS on UART");
    assert_eq!(exit, Some(0));
}

#[test]
fn gui_path_printf_constexpr_debug() {
    use full_stack::virtual_machine::virtual_machine::StepOutcome;

    let pipeline = CompilationPipeline::new();
    let src = std::fs::read_to_string("programs/example/compile_time_math.hll").unwrap();
    let user_result = pipeline
        .compile(&src)
        .expect("user compile");
    let (user_asm, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);

    eprintln!("=== USER ASM ===\n{}", &user_asm[..user_asm.len().min(3000)]);

    let mut linked = user_tokens;
    linked.extend(Vec::new());
    let assembled = pipeline.assemble(&linked).expect("assemble");

    let mut syms: Vec<_> = assembled.symbol_table.iter().collect();
    syms.sort_by_key(|(_, v)| *v);
    eprintln!("=== SYMBOL TABLE (key symbols) ===");
    for (k, v) in syms.iter().filter(|(k, _)| {
        matches!(k.as_str(), "main" | "printf" | "_start" | "putchar" | "print_int")
    }) {
        eprintln!("  {:#010x}  {}", v, k);
    }
    eprintln!("=== SECTIONS ===");
    for s in &assembled.sections {
        eprintln!("  {:?}: {} bytes", s.kind, s.bytes.len());
    }

    let elf = assembled.to_elf(ELF_LOAD_BASE);
    let mut vm = VirtualMachine::from_elf(&elf).expect("load elf");

    // Step 200 times then check UART
    for _ in 0..200 {
        match vm.step() {
            Ok(StepOutcome::Halted(code)) => {
                eprintln!("Halted early: {code}");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Step error: {e:?}");
                break;
            }
        }
    }
    let uart = String::from_utf8_lossy(&vm.drain_uart_output()).into_owned();
    eprintln!("UART after 200 steps: {:?}", uart);
    eprintln!("PC after 200 steps: {:#x}", vm.peek_pc());
}

#[test]
fn gui_path_stdlib_assembly_is_non_empty() {
    let pipeline = CompilationPipeline::new();
    let stdlib_src = get_stdlib_source();
    let result = pipeline.compile(&stdlib_src).expect("stdlib compile");
    let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    assert!(!tokens.is_empty(), "stdlib tokens should not be empty");
    // Spot-check: malloc should be defined
    let has_malloc = tokens.iter().any(|t| {
        use full_stack::assembly_language::rv_instruction::RvInstruction;
        matches!(t, RvInstruction::Label(n) if n == "malloc")
    });
    assert!(has_malloc, "stdlib tokens should include malloc label");
}
