use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::{AssembledOutput, Assembler, ObjectLinker};
use hll_to_ir::stdlib::get_kernel_stdlib_source;
use hll_to_ir::{CompileConfig, HllCompiler, TargetMode};
use ir_to_asm::CompilerRv64;
use os_runtime::kernel;
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// --- Helpers ---

static STDLIB_OBJ: OnceLock<AssembledOutput> = OnceLock::new();

fn compiled_stdlib() -> &'static AssembledOutput {
    STDLIB_OBJ.get_or_init(|| {
        let compiler = HllCompiler::new(CompileConfig {
            target: TargetMode::Kernel,
            strict: true,
            string_prefix: Some("__kern_str_".to_owned()),
            type_prelude: Vec::new(),
            source_prelude: None,
        });
        let out = compiler
            .compile(&get_kernel_stdlib_source())
            .unwrap_or_else(|diags| panic!("kernel stdlib compile failed: {diags:?}"));
        let mut rv = CompilerRv64::new();
        let (_, tokens) = rv.compile_with_tokens(&out.ir);
        Assembler::assemble(&tokens).unwrap_or_else(|e| panic!("stdlib assemble failed: {e}"))
    })
}

fn run_kernel_hll(user_src: &str) -> (String, Option<i64>) {
    let stdlib_obj = compiled_stdlib();

    let user_compiler = HllCompiler::new(CompileConfig {
        target: TargetMode::Kernel,
        strict: true,
        string_prefix: None,
        type_prelude: Vec::new(),
        source_prelude: None,
    });
    let user_out = user_compiler
        .compile(user_src)
        .unwrap_or_else(|diags| panic!("user compile failed: {diags:?}"));
    let mut user_rv = CompilerRv64::new();
    let (_, user_tokens) = user_rv.compile_with_tokens(&user_out.ir);

    let user_obj =
        Assembler::assemble(&user_tokens).unwrap_or_else(|e| panic!("user assemble failed: {e}"));
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

// --- ROM / boot assembly source content ---

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

// --- Trap handler source content ---

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

// --- Reference kernel source content ---

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

// --- End-to-end kernel boot execution ---

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

// --- Process / syscall / scheduler source tests ---

#[test]
fn process_hll_defines_create_and_init() {
    assert!(
        kernel::PROCESS.contains("process_create"),
        "process.hll must define process_create"
    );
    assert!(
        kernel::PROCESS.contains("process_init"),
        "process.hll must define process_init"
    );
    // The PCB-layout consts live in the shared layout.hll, prepended to every kernel TU.
    assert!(
        kernel::LAYOUT.contains("PROC_READY"),
        "layout.hll must define PROC_READY constant"
    );
    assert!(
        kernel::LAYOUT.contains("PROC_EXITED"),
        "layout.hll must define PROC_EXITED constant"
    );
}

#[test]
fn kernel_layout_constants_defined() {
    assert!(
        kernel::LAYOUT.contains("PCB_SIZE"),
        "layout.hll must define PCB_SIZE"
    );
    assert!(
        kernel::LAYOUT.contains("PCB_OFF_TRAP_FRAME"),
        "layout.hll must define PCB_OFF_TRAP_FRAME"
    );
}

#[test]
fn syscall_hll_defines_dispatch() {
    assert!(
        kernel::SYSCALL.contains("syscall_dispatch"),
        "syscall.hll must define syscall_dispatch"
    );
    assert!(
        kernel::SYSCALL.contains("sys_write_impl"),
        "syscall.hll must define sys_write_impl"
    );
    assert!(
        kernel::SYSCALL.contains("SYSCALL_EXIT"),
        "syscall.hll must define SYSCALL_EXIT"
    );
    assert!(
        kernel::SYSCALL.contains("SYSCALL_WRITE"),
        "syscall.hll must define SYSCALL_WRITE"
    );
    assert!(
        kernel::SYSCALL.contains("SYSCALL_YIELD"),
        "syscall.hll must define SYSCALL_YIELD"
    );
}

#[test]
fn syscall_hll_action_constants_defined() {
    assert!(
        kernel::SYSCALL.contains("SYSACT_CONTINUE"),
        "syscall.hll must define SYSACT_CONTINUE"
    );
    assert!(
        kernel::SYSCALL.contains("SYSACT_SCHEDULE"),
        "syscall.hll must define SYSACT_SCHEDULE"
    );
    assert!(
        kernel::SYSCALL.contains("SYSACT_EXIT_SCHEDULE"),
        "syscall.hll must define SYSACT_EXIT_SCHEDULE"
    );
}

#[test]
fn scheduler_hll_defines_schedule() {
    assert!(
        kernel::SCHEDULER.contains("schedule:"),
        "scheduler.hll must define schedule"
    );
    assert!(
        kernel::SCHEDULER.contains("scheduler_add"),
        "scheduler.hll must define scheduler_add"
    );
    assert!(
        kernel::SCHEDULER.contains("scheduler_init"),
        "scheduler.hll must define scheduler_init"
    );
    assert!(
        kernel::SCHEDULER.contains("current_process"),
        "scheduler.hll must define current_process"
    );
    assert!(
        kernel::SCHEDULER.contains("ready_queue_head"),
        "scheduler.hll must define ready_queue_head"
    );
}

#[test]
fn trap_handler_calls_syscall_dispatch_on_umode_ecall() {
    assert!(
        kernel::TRAP_HANDLER.contains("syscall_dispatch"),
        "trap handler must call syscall_dispatch on U-mode ecall"
    );
    assert!(
        kernel::TRAP_HANDLER.contains("schedule("),
        "trap handler must call schedule when syscall action != 0"
    );
    assert!(
        kernel::TRAP_HANDLER.contains("scause_u == 8"),
        "trap handler must check for U-mode ecall (cause 8)"
    );
}

// --- Compile / boot smoke test ---

#[test]
fn kernel_boots_with_process_and_scheduler() {
    // Minimal kernel that initialises process + scheduler alongside normal boot.
    let test_kernel = "
external klog_ok:               (msg: u8*) -> ()
external klog_warn:             (msg: u8*) -> ()
external klog_error:            (msg: u8*) -> ()
external klog_int:              (label: u8*, val: i64) -> ()
external kmalloc:               (size: u64) -> u8*
external memset:                (dst: u8*, value: u8, n: u64) -> u8*
external kshutdown:             (code: i64) -> ()
external trap_init:             () -> ()
external plic_init:             () -> ()
external timer_set:             (interval: u64) -> ()
external pmm_init:              (start: u64, end: u64) -> ()
external pmm_alloc:             () -> u8*
external pmm_free:              (page: u8*) -> ()
external vmm_init:              () -> ()
external vmm_map_1gib:          (va: u64, pa: u64, flags: u64) -> ()
external vmm_enable:            () -> ()
external s_enable_interrupts: () -> ()
external memory_self_test:      (size: u64) -> i64
external pmm_ops_test:          () -> ()
external process_init:          () -> ()
external process_create:        (entry_pc: u64) -> u64*
external scheduler_init:        () -> ()
external scheduler_add:         (pcb: u64*) -> ()

kmain: () -> () {
    klog_ok(\"kernel starting\".data)
    trap_init()
    timer_set(1000000)
    plic_init()
    klog_ok(\"interrupt controller online\".data)
    memory_self_test(256)
    kmalloc(64)
    pmm_init(0x80100000, 0x87F00000)
    vmm_init()
    vmm_map_1gib(0x00000000, 0, 38)
    vmm_map_1gib(0x80000000, 0x80000000, 46)
    vmm_map_1gib(0xC0000000, 0xC0000000, 46)
    vmm_enable()
    process_init()
    scheduler_init()
    klog_ok(\"boot complete\".data)
    kshutdown(0)
}
";
    let (uart, exit) = run_kernel_hll(test_kernel);
    assert_eq!(exit, Some(0), "kernel must exit with code 0; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] process subsystem ready\n"),
        "process subsystem must initialise; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] scheduler ready\n"),
        "scheduler must initialise; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "kernel must complete boot; uart={uart:?}"
    );
}

// --- malloc / free unit tests ---

const MALLOC_PRELUDE: &str = r#"
external malloc:    (size: u64) -> u8*
external free:      (ptr: u8*) -> ()
external kshutdown: (code: i64) -> ()
"#;

#[test]
fn malloc_returns_non_null_for_small_allocation() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    p: u8* = malloc(8)
    if p == null {
        kshutdown(1)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(exit, Some(0), "malloc(8) must return a non-null pointer");
}

#[test]
fn malloc_zero_size_returns_non_null() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    p: u8* = malloc(0)
    if p == null {
        kshutdown(1)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc(0) must be coerced to malloc(1) and return non-null"
    );
}

#[test]
fn malloc_write_read_roundtrip() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    p: i64* = i64*(malloc(8))
    if p == null {
        kshutdown(1)
        return
    }
    @p = 12345
    if @p != 12345 {
        kshutdown(2)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "write to and read back from malloc'd memory must round-trip"
    );
}

#[test]
fn malloc_multiple_allocations_are_distinct() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    a: u8* = malloc(8)
    b: u8* = malloc(8)
    c: u8* = malloc(8)
    if a == null {
        kshutdown(1)
        return
    }
    if b == null {
        kshutdown(2)
        return
    }
    if c == null {
        kshutdown(3)
        return
    }
    if a == b {
        kshutdown(4)
        return
    }
    if b == c {
        kshutdown(5)
        return
    }
    if a == c {
        kshutdown(6)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "three consecutive malloc calls must return distinct pointers"
    );
}

#[test]
fn free_marks_block_reusable_on_next_same_size_malloc() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    p: u8* = malloc(64)
    if p == null {
        kshutdown(1)
        return
    }
    free(p)
    q: u8* = malloc(64)
    if q == null {
        kshutdown(2)
        return
    }
    if q != p {
        kshutdown(3)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc after free of same size must reuse the freed block"
    );
}

#[test]
fn free_null_is_a_noop() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    free(null)
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "free(null) must be a silent no-op and not crash"
    );
}

#[test]
fn malloc_large_allocation_succeeds() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    p: u8* = malloc(1024)
    if p == null {
        kshutdown(1)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc(1024) must succeed within the 64 KiB heap"
    );
}

#[test]
fn malloc_exhaustion_returns_null() {
    let src = MALLOC_PRELUDE.to_owned()
        + r#"
kmain: () -> () {
    got_null: u64 = 0
    i: u64 = 0
    while i < 20 {
        p: u8* = malloc(4096)
        if p == null {
            got_null = 1
        }
        i = i + 1
    }
    if got_null == 0 {
        kshutdown(1)
        return
    }
    kshutdown(0)
}
"#;
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc must return null once the 64 KiB heap is exhausted"
    );
}

// --- console / UART output unit tests ---
//
// These guard against a VA/PA mismatch bug where console_putchar used ecall with a
// stack-allocated byte buffer.  When called from an S-mode syscall handler with sp
// pointing at the user virtual stack (0x7FFFxxxx), M-mode read that virtual address
// as a physical address and wrote garbage to UART.
// The fix writes directly to the NS16550A UART MMIO address (0x10000000),
// which is identity-mapped and accessible from S-mode without an ecall.

#[test]
fn freestanding_console_contains_no_ecall() {
    // The freestanding console must use direct MMIO writes, never ecall.
    // An ecall here causes M-mode to treat the caller's sp as a physical
    // address, which breaks when sp is in non-identity-mapped user VA space.
    // Strip HLL comment lines (starting with ';') before checking so that
    // prose mentioning "ecall" in comments does not trigger a false positive.
    let non_comment: String = os_runtime::stdlib::FREESTANDING_CONSOLE
        .lines()
        .filter(|l| !l.trim_start().starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !non_comment.contains("ecall"),
        "freestanding console.hll must not use ecall as an instruction -- direct UART MMIO only"
    );
}

#[test]
fn freestanding_console_references_uart_mmio_address() {
    assert!(
        os_runtime::stdlib::FREESTANDING_CONSOLE.contains("0x10000000"),
        "freestanding console.hll must reference the NS16550A UART MMIO address 0x10000000"
    );
}

#[test]
fn freestanding_console_putchar_uses_store_byte() {
    // sb (store-byte) is the correct instruction for writing a single char to UART TX;
    // a wider store would corrupt adjacent UART registers.
    assert!(
        os_runtime::stdlib::FREESTANDING_CONSOLE.contains("sb   a0"),
        "console_putchar must use 'sb a0' to write a single byte to the UART TX register"
    );
}

#[test]
fn console_putchar_writes_exact_bytes_to_uart() {
    let src = r#"
external console_putchar: (c: i32) -> ()
external kshutdown: (code: i64) -> ()
kmain: () -> () {
    console_putchar(65)
    console_putchar(66)
    console_putchar(67)
    kshutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("ABC"),
        "console_putchar must produce the exact character value in UART; uart={uart:?}"
    );
}

#[test]
fn console_write_writes_exact_string_to_uart() {
    let src = r#"
external console_write: (str: u8*) -> ()
external kshutdown: (code: i64) -> ()
kmain: () -> () {
    console_write("uart-check".data)
    kshutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("uart-check"),
        "console_write must produce the exact string in UART; uart={uart:?}"
    );
}

#[test]
fn console_print_int_produces_correct_decimal_digits() {
    // This specifically guards the digit-printing path: before the fix,
    // console_putchar stored the digit on a (possibly user-virtual) stack slot,
    // passed that address to M-mode via ecall, and M-mode read garbage.
    let src = r#"
external console_print_int: (n: i64) -> ()
external kshutdown: (code: i64) -> ()
kmain: () -> () {
    console_print_int(0)
    console_print_int(42)
    console_print_int(-7)
    kshutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("0"),
        "console_print_int(0) must output '0'; uart={uart:?}"
    );
    assert!(
        uart.contains("42"),
        "console_print_int(42) must output '42'; uart={uart:?}"
    );
    assert!(
        uart.contains("-7"),
        "console_print_int(-7) must output '-7'; uart={uart:?}"
    );
}

#[test]
fn console_print_hex_produces_correct_hex_digits() {
    let src = r#"
external console_print_hex: (n: u64) -> ()
external kshutdown: (code: i64) -> ()
kmain: () -> () {
    console_print_hex(255)
    console_print_hex(0)
    kshutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("0x00000000000000ff"),
        "console_print_hex(255) must output '0x00000000000000ff'; uart={uart:?}"
    );
    assert!(
        uart.contains("0x0000000000000000"),
        "console_print_hex(0) must output '0x0000000000000000'; uart={uart:?}"
    );
}
