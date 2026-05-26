use os_runtime::{kernel, user};
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source,
    get_stdlib_source_for_mode,
    get_stdlib_type_prelude,
};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

/// Compile the kernel, compile the user hello program, inject the user binary
/// into RAM, and run the VM.  Returns UART output and final step outcome.
fn run_kernel_with_user() -> (String, StepOutcome) {
    // ---- 1. Compile kernel stdlib (as a single concatenated unit) ----
    let mut kernel_stdlib_pipeline = CompilationPipeline::new();
    kernel_stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    kernel_stdlib_pipeline.set_write_artifacts(false);
    let stdlib = kernel_stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        kernel_stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = kernel_stdlib_pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");

    // ---- 2. Compile my_kernel module ----
    let mut kernel_pipeline = CompilationPipeline::new();
    kernel_pipeline.set_target_mode(TargetMode::Kernel);
    kernel_pipeline.set_write_artifacts(false);

    let kernel_modules = vec![("my_kernel", kernel::MY_KERNEL)];
    let kernel_objects = kernel_pipeline
        .compile_modules(&kernel_modules)
        .expect("kernel modules compile");

    // ---- 3. Link kernel ----
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

    // ---- 4. Compile user_hello.hll as a hosted program with stdlib ----
    // Hosted mode uses ecall-based syscalls (sys_write=64) that the kernel
    // handles via its syscall dispatcher.  Freestanding would use direct UART
    // MMIO at 0x10000000 which is not mapped for user-mode processes.
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

    // ---- 5. Build a flat user binary (text + rodata + data, no page gaps) ----
    // The linker places sections consecutively starting at ELF_LOAD_BASE.
    // Concatenate non-BSS sections into a flat buffer, pad to 4 KiB.
    let mut flat = Vec::new();
    flat.extend_from_slice(user_assembled.text_bytes());
    flat.extend_from_slice(user_assembled.rodata_bytes());
    flat.extend_from_slice(user_assembled.data_bytes());
    // Pad to 4 KiB page.
    let page_size = 4096usize;
    let padded = (flat.len() + page_size - 1) / page_size * page_size;
    flat.resize(padded, 0u8);

    // ---- 6. Compute the absolute entry VA for the kernel ----
    // Symbol addresses from ObjectLinker::link are section-relative offsets.
    const USER_CODE_VA: u64 = 0x4000_0000;
    let entry_off = user_assembled
        .symbol_address("_start")
        .expect("_start symbol not found in user binary");
    let entry_va = USER_CODE_VA + entry_off;
    eprintln!("user binary: {} bytes flat, entry_off={:#x}, entry_va={:#x}", flat.len(), entry_off, entry_va);

    // ---- 7. Create VM and inject user binary + metadata into RAM ----
    const USER_BINARY_PA: u64 = 0x87F0_0000; // outside PMM range, inside 128 MiB RAM
    const USER_META_PA:   u64 = 0x87EF_F000;  // one page below: [0]=entry_va, [8]=size_bytes
    let user_size = flat.len() as u64;
    let mut vm = VirtualMachine::new_kernel(&final_assembled);
    vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
        .expect("write user entry VA to RAM");
    vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
        .expect("write user size to RAM");
    vm.write_ram(USER_BINARY_PA, &flat)
        .expect("write user binary to RAM");

    // ---- 7. Run ----
    let run = vm.run(10_000_000);
    eprintln!("=== VM UART OUTPUT ===\n{}=== END UART ===", run.uart_output);
    (run.uart_output, run.outcome)
}

// ---------------------------------------------------------------------------
// Full boot sequence
// ---------------------------------------------------------------------------

#[test]
fn kernel_boot_full_init_sequence() {
    let (uart, outcome) = run_kernel_with_user();
    match outcome {
        StepOutcome::Continue => {} // idle loop, expected
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}; uart={uart:?}"),
    }
    assert!(
        uart.contains("[  OK  ] kernel starting\n"),
        "expected boot banner; uart={uart:?}"
    );
    assert!(
        uart.contains("[ WARN ] device tree:"),
        "expected device-tree probe warning; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "expected memory self-test to pass; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] heap ready\n"),
        "expected heap smoke-test; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "expected boot complete; uart={uart:?}"
    );
    // The \"entering scheduler idle loop\" message may be preempted by a timer
    // tick that switches to the user process before kmain reaches that line.
    // It is not a required checkpoint.
    assert!(
        uart.contains("[  OK  ] user process spawned\n"),
        "expected user process spawn message; uart={uart:?}"
    );
}

// ---------------------------------------------------------------------------
// Timer interrupt: kernel arms timer and the UART confirms timer setup
// ---------------------------------------------------------------------------

#[test]
fn kernel_timer_armed_and_boots() {
    let (uart, outcome) = run_kernel_with_user();
    match outcome {
        StepOutcome::Continue => {}
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}; uart={uart:?}"),
    }
    assert!(
        uart.contains("[  OK  ] timer armed\n"),
        "expected timer armed message; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "expected boot complete message; uart={uart:?}"
    );
}

// ---------------------------------------------------------------------------
// Device tree stub is reached
// ---------------------------------------------------------------------------

#[test]
fn device_tree_probe_logged() {
    let (uart, outcome) = run_kernel_with_user();
    match outcome {
        StepOutcome::Continue => {}
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}"),
    }
    let console_pos = uart
        .find("[  OK  ] console online\n")
        .expect("console online missing");
    let dt_pos = uart
        .find("[ WARN ] device tree:")
        .expect("device tree warn missing");
    let mem_pos = uart
        .find("[  OK  ] running memory diagnostics...\n")
        .expect("memory diagnostics missing");
    assert!(
        console_pos < dt_pos && dt_pos < mem_pos,
        "init order wrong: console={console_pos} dt={dt_pos} mem={mem_pos}"
    );
}

// ---------------------------------------------------------------------------
// Memory self-test runs inside the kernel
// ---------------------------------------------------------------------------

#[test]
fn kernel_memory_self_test_passes() {
    let (uart, outcome) = run_kernel_with_user();
    match outcome {
        StepOutcome::Continue => {}
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}"),
    }
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "uart={uart:?}"
    );
    assert!(
        !uart.contains("memory self-test failed"),
        "uart={uart:?}"
    );
}

// ---------------------------------------------------------------------------
// User process runs and produces output
// ---------------------------------------------------------------------------

#[test]
fn user_process_writes_hello() {
    let (uart, outcome) = run_kernel_with_user();
    match outcome {
        StepOutcome::Continue => {}
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}"),
    }
    assert!(
        uart.contains("[  OK  ] user process spawned\n"),
        "expected user process spawn; uart={uart:?}"
    );
    assert!(
        uart.contains("hello from user mode!\n"),
        "expected user-mode greeting output; uart={uart:?}"
    );
}
