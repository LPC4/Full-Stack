use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::high_level_language::stdlib::prepend_stdlib;
use full_stack::virtual_machine::virtual_machine::VirtualMachine;

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
    let raw = std::fs::read_to_string("programs/example/generics_strings.hll").unwrap();
    let src = prepend_stdlib(&raw);
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
    let src = prepend_stdlib(
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
