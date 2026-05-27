use os_runtime::{kernel, user};
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source,
    get_stdlib_source_for_mode,
    get_stdlib_type_prelude,
};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

/// Compile kernel + user_hello, inject and run.
fn run_kernel_with_injected_user() -> (String, StepOutcome) {
    eprintln!("=== Starting kernel + user_hello injection test ===");

    // --- 1. Compile kernel stdlib (as a single concatenated unit) ---
    let mut kernel_stdlib_pipeline = CompilationPipeline::new();
    kernel_stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    kernel_stdlib_pipeline.set_write_artifacts(false);
    let stdlib = kernel_stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        kernel_stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = kernel_stdlib_pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");
    eprintln!("Kernel stdlib compiled: {} bytes", stdlib_obj.text_bytes().len());

    // --- 2. Compile my_kernel module ---
    let mut kernel_pipeline = CompilationPipeline::new();
    kernel_pipeline.set_target_mode(TargetMode::Kernel);
    kernel_pipeline.set_write_artifacts(false);

    let kernel_modules = vec![("my_kernel", kernel::MY_KERNEL)];
    let kernel_objects = kernel_pipeline
        .compile_modules(&kernel_modules)
        .expect("kernel modules compile");
    eprintln!("Kernel module compiled: {} bytes", kernel_objects[0].text_bytes().len());

    // --- 3. Link kernel ---
    let module_names: Vec<&str> = kernel_modules.iter().map(|(n, _)| *n).collect();
    let mut all_names = vec!["kernel_stdlib"];
    all_names.extend(&module_names);
    let mut object_refs = vec![&stdlib_obj];
    for obj in &kernel_objects {
        object_refs.push(obj);
    }

    kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
    let final_assembled = kernel_pipeline
        .link_assembled_objects_named(
            &all_names.join("_"),
            &all_names
                .iter()
                .zip(object_refs.iter())
                .map(|(n, o)| (*n, *o))
                .collect::<Vec<_>>(),
        )
        .expect("kernel link");
    eprintln!("Kernel linked: {} bytes total", final_assembled.text_bytes().len());

    // --- 4. Compile user_hello as Hosted ---
    let mut user_pipeline = CompilationPipeline::new();
    user_pipeline.set_target_mode(TargetMode::Hosted);
    user_pipeline.set_write_artifacts(false);
    user_pipeline.set_type_prelude(get_stdlib_type_prelude());

    let user_source = format!("{}\n{}", get_stdlib_source_for_mode(TargetMode::Hosted), user::USER_HELLO);
    let user_result = user_pipeline
        .compile(&user_source)
        .expect("user program compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_assembled = user_pipeline
        .assemble(&user_tokens)
        .expect("user program assemble");
    eprintln!("User program compiled: {} bytes", user_assembled.text_bytes().len());

    // --- 5. Build a flat user binary ---
    // BSS must be included so heap_buffer (used by malloc) is mapped in user VA space.
    let mut flat = user_assembled.to_flat_binary();
    let page_size = 4096usize;
    let padded = (flat.len() + page_size - 1) / page_size * page_size;
    flat.resize(padded, 0u8);
    eprintln!("User flat binary: {} bytes padded to {} bytes ({} pages)", flat.len(), padded, padded / page_size);

    // --- 6. Compute entry VA ---
    const USER_CODE_VA: u64 = 0x4000_0000;
    let entry_off = user_assembled
        .symbol_address("_start")
        .expect("_start symbol not found");
    let entry_va = USER_CODE_VA + entry_off;
    eprintln!("User entry: offset={:#x}, va={:#x}", entry_off, entry_va);

    // --- 7. Create VM and inject ---
    const USER_BINARY_PA: u64 = 0x87F0_0000;
    const USER_META_PA: u64 = 0x87EF_F000;
    let user_size = flat.len() as u64;
    let mut vm = VirtualMachine::new_kernel(&final_assembled);
    eprintln!("VM created, injecting user binary...");

    vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
        .expect("write user entry VA");
    vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
        .expect("write user size");
    vm.write_ram(USER_BINARY_PA, &flat)
        .expect("write user binary");
    eprintln!("User binary injected: metadata at {:#x}, binary at {:#x} ({} bytes)", USER_META_PA, USER_BINARY_PA, flat.len());

    // Verify the first 32 bytes of the injected binary
    let first_32 = &flat[0..32.min(flat.len())];
    eprintln!("First 32 bytes of user binary to be injected:");
    for (i, byte) in first_32.iter().enumerate() {
        if i % 16 == 0 {
            eprint!("\n  {:#06x}: ", i);
        }
        eprint!("{:02x} ", byte);
    }
    eprintln!();

    // --- 8. Run ---
    eprintln!("Running VM for 10M steps...");
    let run = vm.run(10_000_000);
    eprintln!("=== VM OUTPUT ===\n{}\n=== END ===", run.uart_output);
    (run.uart_output, run.outcome)
}

#[test]
fn test_kernel_with_user_hello_injection() {
    let (uart, outcome) = run_kernel_with_injected_user();
    match outcome {
        StepOutcome::Continue => {} // idle loop, expected
        StepOutcome::Halted(c) => panic!("unexpected halt {c}\nuart={uart:?}"),
    }

    // Check that user process was spawned
    assert!(
        uart.contains("[  OK  ] user process spawned"),
        "expected user spawned message\nuart={uart:?}"
    );

    // Check that we got the hello message from user program
    assert!(
        uart.contains("hello from user mode!"),
        "expected hello message from user program\nuart={uart:?}"
    );
}

