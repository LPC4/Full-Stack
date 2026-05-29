// Integration tests: compile each example program, inject it as a hosted user process into the
// full kernel, run the VM, and verify the program exits cleanly.
//
// The flat user binary now includes BSS via to_flat_binary, so heap_buffer is mapped in user
// VA space and malloc/free work correctly.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::{get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude};
use os_runtime::kernel;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

use std::sync::OnceLock;

// --- Kernel binary (compiled once per test run) ---

static KERNEL_BINARY: OnceLock<AssembledOutput> = OnceLock::new();

fn get_kernel_binary() -> &'static AssembledOutput {
    KERNEL_BINARY.get_or_init(|| {
        let mut stdlib_pipeline = CompilationPipeline::new();
        stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
        stdlib_pipeline.set_write_artifacts(false);
        let stdlib = stdlib_pipeline
            .compile(&get_kernel_stdlib_source())
            .expect("kernel stdlib compile");
        let (_, stdlib_tokens) =
            stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
        let stdlib_obj = stdlib_pipeline
            .assemble(&stdlib_tokens)
            .expect("stdlib assemble");

        let mut kernel_pipeline = CompilationPipeline::new();
        kernel_pipeline.set_target_mode(TargetMode::Kernel);
        kernel_pipeline.set_write_artifacts(false);
        let kernel_objs = kernel_pipeline
            .compile_modules(&[("my_kernel", kernel::MY_KERNEL)])
            .expect("kernel modules compile");

        kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
        kernel_pipeline
            .link_assembled_objects_named(
                "kernel_stdlib_my_kernel",
                &[("kernel_stdlib", &stdlib_obj), ("my_kernel", &kernel_objs[0])],
            )
            .expect("kernel link")
    })
}

// --- Helper: compile a user program, inject into kernel, run ---

/// Compile `user_src` with the hosted stdlib, inject into the kernel, run for
/// up to 10 M steps, and return (uart_output, step_outcome).
fn run_user_in_kernel(user_src: &str) -> (String, StepOutcome) {
    let kernel_binary = get_kernel_binary();

    let mut user_pipeline = CompilationPipeline::new();
    user_pipeline.set_target_mode(TargetMode::Hosted);
    user_pipeline.set_write_artifacts(false);
    user_pipeline.set_type_prelude(get_stdlib_type_prelude());

    let full_source =
        format!("{}\n{}", get_stdlib_source_for_mode(TargetMode::Hosted), user_src);
    let user_result = user_pipeline
        .compile(&full_source)
        .expect("user program compile");
    let (_, user_tokens) =
        user_pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_assembled = user_pipeline
        .assemble(&user_tokens)
        .expect("user program assemble");

    // Include BSS (heap_buffer lives there) so malloc works in user VA space.
    let mut flat = user_assembled.to_flat_binary();
    let page_size = 4096usize;
    let padded = (flat.len() + page_size - 1) / page_size * page_size;
    flat.resize(padded, 0u8);

    const USER_CODE_VA: u64 = 0x4000_0000;
    const USER_BINARY_PA: u64 = 0x87F0_0000;
    const USER_META_PA: u64 = 0x87EF_F000;

    let entry_off = user_assembled
        .symbol_address("_start")
        .expect("_start symbol missing from user binary");
    let entry_va = USER_CODE_VA + entry_off;
    let user_size = flat.len() as u64;

    let mut vm = VirtualMachine::new_kernel(kernel_binary);
    vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
        .expect("write user entry VA");
    vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
        .expect("write user size");
    vm.write_ram(USER_BINARY_PA, &flat)
        .expect("write user binary");

    let run = vm.run(10_000_000);
    (run.uart_output, run.outcome)
}

/// Assert that the user program exited with code 0.
fn assert_user_exit_ok(uart: &str, outcome: &StepOutcome, label: &str) {
    // The last process exiting now halts the VM cleanly via SYSCON; a zero exit
    // code is expected. Only a non-zero halt indicates a failure.
    if let StepOutcome::Halted(c) = outcome
        && *c != 0
    {
        panic!("{label}: unexpected VM halt with code {c}; uart={uart:?}");
    }
    assert!(
        !uart.contains("PANIC!"),
        "{label}: kernel panicked; uart={uart:?}"
    );
    assert!(
        !uart.contains("unhandled exception"),
        "{label}: unhandled CPU exception; uart={uart:?}"
    );
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "{label}: user process was not spawned; uart={uart:?}"
    );
    assert!(
        uart.contains("sys_exit code: 0"),
        "{label}: user process did not exit with code 0; uart={uart:?}"
    );
}

// --- Example program sources ---

const CORE_BASICS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/core_basics.hll"
));

const POINTER_ARRAYS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/pointer_arrays.hll"
));

const ARRAY_INITIALIZATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/array_initialization.hll"
));

const STRUCT_BINDING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/struct_binding.hll"
));

const CONTROL_FLOW_BASICS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/control_flow_basics.hll"
));

const CASTING_AND_POINTERS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/casting_and_pointers.hll"
));

const COMPILE_TIME_MATH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/compile_time_math.hll"
));

const GENERICS_AND_STRINGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/example/generics_and_strings.hll"
));

// --- Tests: one per example program ---

#[test]
fn example_core_basics_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(CORE_BASICS);
    assert_user_exit_ok(&uart, &outcome, "core_basics");
}

#[test]
fn example_pointer_arrays_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(POINTER_ARRAYS);
    assert_user_exit_ok(&uart, &outcome, "pointer_arrays");
}

#[test]
fn example_array_initialization_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(ARRAY_INITIALIZATION);
    assert_user_exit_ok(&uart, &outcome, "array_initialization");
}

#[test]
fn example_struct_binding_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(STRUCT_BINDING);
    assert_user_exit_ok(&uart, &outcome, "struct_binding");
}

#[test]
fn example_control_flow_basics_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(CONTROL_FLOW_BASICS);
    assert_user_exit_ok(&uart, &outcome, "control_flow_basics");
}

#[test]
fn example_casting_and_pointers_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(CASTING_AND_POINTERS);
    assert_user_exit_ok(&uart, &outcome, "casting_and_pointers");
}

#[test]
fn example_compile_time_math_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(COMPILE_TIME_MATH);
    assert_user_exit_ok(&uart, &outcome, "compile_time_math");
}

#[test]
fn example_generics_and_strings_runs_in_kernel_userspace() {
    let (uart, outcome) = run_user_in_kernel(GENERICS_AND_STRINGS);
    assert_user_exit_ok(&uart, &outcome, "generics_and_strings");
}

// --- Regression test: malloc/free work in user space (BSS fix) ---

#[test]
fn user_malloc_and_free_work_in_kernel_userspace() {
    let src = r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 42
    if @p != 42 {
        free(p)
        return 1
    }
    free(p)

    q: i32* = new(i32)
    defer free(q)
    @q = 99
    if @q != 99 {
        return 2
    }

    return 0
}
"#;
    let (uart, outcome) = run_user_in_kernel(src);
    assert_user_exit_ok(&uart, &outcome, "user_malloc_and_free");
}

#[test]
fn user_free_then_realloc_reuses_block() {
    let src = r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 1
    free(p)
    q: i32* = new(i32)
    if q != p {
        free(q)
        return 1
    }
    @q = 7
    if @q != 7 {
        free(q)
        return 2
    }
    free(q)
    return 0
}
"#;
    let (uart, outcome) = run_user_in_kernel(src);
    assert_user_exit_ok(&uart, &outcome, "user_free_then_realloc");
}
