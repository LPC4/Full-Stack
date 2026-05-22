use os_runtime::kernel;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_kernel_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

const MY_KERNEL_SRC: &str = kernel::MY_KERNEL;

fn run_kernel_hll(user_src: &str) -> (String, Option<i64>) {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    let user_pipeline = CompilationPipeline::new();
    let user = user_pipeline.compile(user_src).expect("user compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);
    let assembled = user_pipeline.assemble(&linked).expect("assemble");
    let mut vm = VirtualMachine::new_kernel(&assembled);
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
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
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
// Timer interrupt fires 3 times and kernel exits cleanly
// ---------------------------------------------------------------------------

#[test]
fn kernel_timer_fires_three_ticks() {
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
    assert_eq!(exit, Some(0), "kernel must exit cleanly after 3 ticks; uart={uart:?}");
    assert!(uart.contains("timer tick: 1\n"), "expected tick 1; uart={uart:?}");
    assert!(uart.contains("timer tick: 2\n"), "expected tick 2; uart={uart:?}");
    assert!(uart.contains("timer tick: 3\n"), "expected tick 3; uart={uart:?}");
}

#[test]
fn kernel_timer_ticks_in_order() {
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
    assert_eq!(exit, Some(0));
    let pos1 = uart.find("timer tick: 1\n").expect("tick 1 missing");
    let pos2 = uart.find("timer tick: 2\n").expect("tick 2 missing");
    let pos3 = uart.find("timer tick: 3\n").expect("tick 3 missing");
    assert!(pos1 < pos2 && pos2 < pos3, "ticks must appear in order 1 < 2 < 3");
}

#[test]
fn kernel_timer_ticks_after_boot_complete() {
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
    assert_eq!(exit, Some(0));
    let boot_pos = uart.find("[  OK  ] boot complete\n").expect("boot complete missing");
    let tick1_pos = uart.find("timer tick: 1\n").expect("tick 1 missing");
    assert!(
        boot_pos < tick1_pos,
        "boot must complete before first timer tick fires; boot={boot_pos} tick1={tick1_pos}"
    );
}

// ---------------------------------------------------------------------------
// Device tree stub is reached
// ---------------------------------------------------------------------------

#[test]
fn device_tree_probe_logged() {
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
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
    let (uart, exit) = run_kernel_hll(MY_KERNEL_SRC);
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
