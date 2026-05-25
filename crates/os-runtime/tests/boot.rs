use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::{Assembler, ObjectLinker};
use os_runtime::kernel;
use hll_to_ir::stdlib::get_kernel_stdlib_source;
use hll_to_ir::{CompileConfig, HllCompiler, TargetMode};
use ir_to_asm::CompilerRv64;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// -- helpers ------------------------------------------------------------------

fn run_kernel_hll(user_src: &str) -> (String, Option<i64>) {
    let stdlib_compiler = HllCompiler::new(CompileConfig {
        target: TargetMode::Kernel,
        strict: true,
        string_prefix: Some("__kern_str_".to_owned()),
        type_prelude: Vec::new(),
    });
    let stdlib_out = stdlib_compiler
        .compile(&get_kernel_stdlib_source())
        .unwrap_or_else(|diags| panic!("kernel stdlib compile failed: {diags:?}"));
    let mut stdlib_rv = CompilerRv64::new();
    let (_, stdlib_tokens) = stdlib_rv.compile_with_tokens(&stdlib_out.ir);

    let user_compiler = HllCompiler::new(CompileConfig {
        target: TargetMode::Kernel,
        strict: true,
        string_prefix: None,
        type_prelude: Vec::new(),
    });
    let user_out = user_compiler
        .compile(user_src)
        .unwrap_or_else(|diags| panic!("user compile failed: {diags:?}"));
    let mut user_rv = CompilerRv64::new();
    let (_, user_tokens) = user_rv.compile_with_tokens(&user_out.ir);

    let stdlib_obj = Assembler::assemble(&stdlib_tokens)
        .unwrap_or_else(|e| panic!("stdlib assemble failed: {e}"));
    let user_obj = Assembler::assemble(&user_tokens)
        .unwrap_or_else(|e| panic!("user assemble failed: {e}"));
    let mut assembled = ObjectLinker::link(&[("kernel_stdlib", &stdlib_obj), ("user", &user_obj)])
        .unwrap_or_else(|e| panic!("link failed: {e}"));
    let layout = LinkLayout::freestanding_kernel();
    if layout.emit_layout_symbols {
        assembled.inject_layout_symbols(&layout);
    }
    assembled.mark_entry_global("_kernel_start");

    let mut vm = VirtualMachine::new_kernel(&assembled);
    let run = vm.run(10_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

// -- ROM / boot assembly source content ---------------------------------------

#[test]
fn rom_source_is_startup_concatenated_with_trap() {
    let expected = format!("{}{}", os_runtime::BOOT_STARTUP, os_runtime::BOOT_TRAP);
    assert_eq!(
        os_runtime::ROM_SOURCE,
        expected,
        "ROM_SOURCE must equal BOOT_STARTUP + BOOT_TRAP"
    );
}

#[test]
fn boot_startup_mrets_into_smode() {
    assert!(
        os_runtime::BOOT_STARTUP.contains("mret"),
        "startup stub must mret into S-mode"
    );
    assert!(
        os_runtime::BOOT_STARTUP.contains("medeleg"),
        "startup stub must delegate exceptions via medeleg"
    );
    assert!(
        os_runtime::BOOT_STARTUP.contains("mideleg"),
        "startup stub must delegate interrupts via mideleg"
    );
}

#[test]
fn boot_trap_handles_ecalls() {
    assert!(
        os_runtime::BOOT_TRAP.contains("_dispatch_ecall"),
        "M-mode trap handler must dispatch ecalls"
    );
    assert!(
        os_runtime::BOOT_TRAP.contains("sys_exit"),
        "M-mode trap handler must implement sys_exit"
    );
    assert!(
        os_runtime::BOOT_TRAP.contains("sys_write"),
        "M-mode trap handler must implement sys_write"
    );
}

// -- Trap handler source content -----------------------------------------------

#[test]
fn trap_handler_rearms_timer_on_stip() {
    assert!(
        kernel::TRAP_HANDLER.contains("timer_set"),
        "trap handler must rearm timer on Supervisor Timer Interrupt (cause 5)"
    );
}

#[test]
fn trap_handler_advances_sepc_on_umode_ecall() {
    assert!(
        kernel::TRAP_HANDLER.contains("scause_u == 8"),
        "trap handler must advance sepc on U-mode ecall (cause 8)"
    );
}

// -- Reference kernel source content ------------------------------------------

#[test]
fn my_kernel_calls_trap_init() {
    assert!(
        kernel::MY_KERNEL.contains("trap_init()"),
        "reference kernel must call trap_init to install the S-mode trap handler"
    );
}

#[test]
fn my_kernel_arms_timer() {
    assert!(
        kernel::MY_KERNEL.contains("timer_set("),
        "reference kernel must arm the CLINT timer via timer_set"
    );
}

#[test]
fn my_kernel_initializes_interrupt_controller() {
    assert!(
        kernel::MY_KERNEL.contains("plic_init()"),
        "reference kernel must initialize the interrupt controller via plic_init"
    );
}

#[test]
fn my_kernel_warns_for_unimplemented_device_tree() {
    assert!(
        kernel::MY_KERNEL.contains("klog_warn") && kernel::MY_KERNEL.contains("device tree"),
        "unimplemented device-tree stub must emit a warning, not ok"
    );
}

// -- End-to-end kernel boot execution -----------------------------------------

#[test]
fn kernel_boots_and_exits_cleanly() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit with code 0; uart={uart:?}");
}

#[test]
fn trap_handler_installed_at_boot() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] trap handler installed\n"),
        "uart must confirm trap handler install; uart={uart:?}"
    );
}

#[test]
fn timer_armed_at_boot() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] timer armed\n"),
        "uart must confirm timer was armed; uart={uart:?}"
    );
}

#[test]
fn memory_self_test_passes() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "memory self-test must pass; uart={uart:?}"
    );
    assert!(
        !uart.contains("memory self-test failed"),
        "memory self-test must not fail; uart={uart:?}"
    );
}

#[test]
fn heap_smoke_test_passes() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] heap ready\n"),
        "heap smoke-test must pass; uart={uart:?}"
    );
}

#[test]
fn unimplemented_subsystems_warn() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[ WARN ] device tree:"),
        "device tree stub must emit warn; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] interrupt controller online\n"),
        "interrupt controller must initialize and report online; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] mmu: sv39 enabled"),
        "MMU must be enabled; uart={uart:?}"
    );
    assert!(
        uart.contains("[ WARN ] filesystem:"),
        "filesystem stub must emit warn; uart={uart:?}"
    );
}

#[test]
fn pmm_smoke_test_passes() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] pmm ready\n"),
        "PMM smoke-test must pass; uart={uart:?}"
    );
}

#[test]
fn full_boot_output_matches_expected() {
    let (uart, exit) = run_kernel_hll(kernel::MY_KERNEL);
    assert_eq!(exit, Some(0), "kernel must exit with code 0");
    assert_eq!(
        uart,
        "[  OK  ] kernel starting\n\
         [  OK  ] console online\n\
         boot hart: 0\n\
         [  OK  ] trap handler installed\n\
         [  OK  ] timer armed\n\
         [ WARN ] device tree: not implemented\n\
         [  OK  ] interrupt controller online\n\
         [  OK  ] running memory diagnostics...\n\
         [  OK  ] memory self-test passed\n\
         [  OK  ] heap ready\n\
         [  OK  ] pmm ready\n\
         [  OK  ] memory ops test passed\n\
         [  OK  ] vmm: initializing...\n\
         [  OK  ] vmm: root table allocated\n\
         [  OK  ] vmm: identity mappings created\n\
         [  OK  ] vmm: using canonical lower-half identity mapping\n\
         [  OK  ] vmm: enabling MMU...\n\
         [  OK  ] mmu: sv39 enabled\n\
         [ WARN ] filesystem: not implemented\n\
         [ WARN ] single hart, no SMP\n\
         hart id: 0\n\
         ram MB: 128\n\
         [  OK  ] boot complete\n\
         [  OK  ] entering idle loop\n"
    );
}
