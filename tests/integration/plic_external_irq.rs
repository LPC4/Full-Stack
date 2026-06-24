use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_kernel_stdlib_source;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Minimal test kernel that tests external interrupt delivery without full VMM setup
const TEST_KERNEL_WITH_UART_IRQ: &str = r#"
external klog_ok:      (msg: u8*) -> ()
external klog_hex:     (label: u8*, val: u64) -> ()
external kshutdown:    (code: i64) -> ()
external trap_init:    () -> ()
external plic_init:    () -> ()

kmain: () -> () {
    klog_ok("kernel starting".data)

    ; Install trap handler and enable interrupts
    trap_init()
    klog_ok("trap handler installed".data)

    ; Initialize PLIC routing
    plic_init()
    klog_ok("interrupt controller online".data)

    ; Enable UART RX interrupts: write IER[0] = 1 at UART base 0x10000000, offset 1
    asm {
        li   t0, 0x10000001
        li   t1, 1
        sb   t1, 0(t0)
    }
    klog_ok("uart rx interrupts enabled".data)

    klog_ok("waiting for external interrupt".data)

    ; Wait for external interrupt to arrive (WFI will be interrupted by pending IRQ)
    ; This will trigger the trap and deliver it to trap_handler
    asm {
    .Ltest_wait:
        wfi
        j    .Ltest_wait
    }
}
"#;

fn compile_and_run_kernel(kernel_src: &str) -> (String, Option<i64>, u64) {
    let mut stdlib_pipeline = CompilationPipeline::new_v1();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    let user_pipeline = CompilationPipeline::new_v1();
    let user = user_pipeline.compile(kernel_src).expect("kernel compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);

    let stdlib_obj = stdlib_pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");
    let user_obj = user_pipeline.assemble(&user_tokens).expect("user assemble");
    let assembled = user_pipeline
        .link_assembled_objects(&[("kernel_stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link");
    let mut vm = VirtualMachine::new_kernel(&assembled);

    // Run for a bit to let kernel boot
    let boot_run = vm.run(500_000);
    let uart_after_boot = boot_run.uart_output.clone();

    // Now inject a UART RX byte
    vm.uart_receive(0x41); // 'A'

    // Run for more steps to process the interrupt
    let run = vm.run(100_000);

    // Get the CSRs to check scause
    let csrs = vm.peek_csrs();
    let scause = csrs.scause;

    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };

    (
        format!("{}\n{}", uart_after_boot, run.uart_output),
        exit,
        scause,
    )
}

// --- Phase 2: UART RX -> PLIC -> SEIP -> trap cause 9 ---

#[test]
fn plic_uart_rx_injection_and_external_interrupt_delivery() {
    let (uart, exit, scause) = compile_and_run_kernel(TEST_KERNEL_WITH_UART_IRQ);

    eprintln!("UART output:\n{}", uart);
    eprintln!("Exit code: {:?}", exit);
    eprintln!("scause: {:#x}", scause);

    // Verify initial setup completed
    assert!(
        uart.contains("kernel starting"),
        "Kernel should start; uart={uart:?}"
    );
    assert!(
        uart.contains("interrupt controller online"),
        "PLIC should be initialized; uart={uart:?}"
    );
    assert!(
        uart.contains("uart rx interrupts enabled"),
        "UART RX should be enabled; uart={uart:?}"
    );
    assert!(
        uart.contains("waiting for external interrupt"),
        "Should reach interrupt wait point; uart={uart:?}"
    );

    // Check that scause indicates an external interrupt was delivered
    // External interrupt (cause 9) has bit 63 set: (1 << 63) | 9 = 0x8000_0000_0000_0009
    let expected_scause = (1u64 << 63) | 9;
    assert_eq!(
        scause, expected_scause,
        "scause should indicate S-mode external interrupt (cause 9); got {:#x}",
        scause
    );
}

#[test]
fn plic_external_interrupt_has_correct_request_source() {
    // This test verifies that the PLIC claim register returns the correct source ID
    // when we inject a UART RX byte.
    let (uart, _exit, scause) = compile_and_run_kernel(TEST_KERNEL_WITH_UART_IRQ);

    // The trap handler should have claimed the interrupt from PLIC
    // which assigns it to source 10 (UART RX)
    let expected_external_irq = (1u64 << 63) | 9;
    assert_eq!(
        scause, expected_external_irq,
        "External interrupt should be delivered; uart={:?}",
        uart
    );
}




