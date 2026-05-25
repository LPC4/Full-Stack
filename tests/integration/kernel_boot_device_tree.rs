use os_runtime::kernel;
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::get_kernel_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

fn run_kernel_hll_multimodule() -> (String, Option<i64>) {
    // Set up full kernel stdlib compilation (includes all kernel modules except my_kernel)
    let mut kernel_stdlib_pipeline = CompilationPipeline::new();
    kernel_stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    kernel_stdlib_pipeline.set_write_artifacts(false); // Disable artifact output for tests
    let stdlib = kernel_stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        kernel_stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    // Assemble stdlib
    let stdlib_obj = kernel_stdlib_pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");

    // Compile my_kernel as a separate module (at object level, not concatenated)
    let mut kernel_pipeline = CompilationPipeline::new();
    kernel_pipeline.set_target_mode(TargetMode::Kernel);
    kernel_pipeline.set_write_artifacts(false); // Disable artifact output for tests

    let kernel_modules = vec![
        ("my_kernel", kernel::MY_KERNEL),
    ];

    // Compile my_kernel module to its own object file
    let kernel_objects = kernel_pipeline
        .compile_modules(&kernel_modules)
        .expect("kernel modules compile");

    // Link my_kernel with stdlib at object level
    let module_names: Vec<&str> = kernel_modules.iter().map(|(n, _)| *n).collect();
    let mut all_names = vec!["kernel_stdlib"];
    all_names.extend(&module_names);
    let mut object_refs = vec![&stdlib_obj];
    for obj in &kernel_objects {
        object_refs.push(obj);
    }

    // Link with entry point set at link time
    kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
    let final_assembled = kernel_pipeline
        .link_assembled_objects_named(
            &all_names.join("_"),
            &all_names.iter().zip(object_refs.iter())
                .map(|(n, o)| (*n, *o))
                .collect::<Vec<_>>()
        )
        .expect("kernel link");

    let mut vm = VirtualMachine::new_kernel(&final_assembled);
    let run = vm.run(10_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

// ---------------------------------------------------------------------------
// Full boot sequence
// ---------------------------------------------------------------------------

#[test]
fn kernel_boot_full_init_sequence() {
    let (uart, exit) = run_kernel_hll_multimodule();
    assert_eq!(exit, Some(0), "kernel should exit cleanly; uart={uart:?}");
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
    assert!(
        uart.contains("[  OK  ] entering idle loop\n"),
        "expected idle loop entry; uart={uart:?}"
    );
}

// ---------------------------------------------------------------------------
// Timer interrupt: kernel arms timer and the UART confirms timer setup
// ---------------------------------------------------------------------------

#[test]
fn kernel_timer_armed_and_boots() {
    let (uart, exit) = run_kernel_hll_multimodule();
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
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
    let (uart, exit) = run_kernel_hll_multimodule();
    assert_eq!(exit, Some(0));
    // The device-tree probe message must appear after the console is online and
    // before memory diagnostics, reflecting the intended init order.
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
    let (uart, exit) = run_kernel_hll_multimodule();
    assert_eq!(exit, Some(0));
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "uart={uart:?}"
    );
    assert!(
        !uart.contains("memory self-test failed"),
        "uart={uart:?}"
    );
}
