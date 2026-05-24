use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::{get_kernel_stdlib_source, get_stdlib_source};
use virtual_machine::rom::generate_rom_image;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

const MEM_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/os-runtime/stdlib/common/mem.hll"
));
const KLOG_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/os-runtime/stdlib/common/klog.hll"
));

fn run_hll(src: &str) -> (String, Option<i64>) {
    let pipeline = CompilationPipeline::new();
    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let user = pipeline.compile(src).expect("user compile");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);
    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);
    let assembled = pipeline.assemble(&linked).expect("assemble");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

// Prepend extra HLL source (e.g. mem.hll, klog.hll) to user_src and compile
// as one unit, linked against the stdlib.
fn run_with(extra: &str, user_src: &str) -> (String, Option<i64>) {
    run_hll(&format!("{extra}\n{user_src}"))
}

// ── ROM ─────────────────────────────────────────────────────────────────────

#[test]
fn rom_image_assembles() {
    let rom = generate_rom_image();
    assert!(!rom.is_empty(), "ROM image must be non-empty");
    assert_eq!(rom.len() % 4, 0, "ROM size must be word-aligned");
}

// ── mem.hll ─────────────────────────────────────────────────────────────────

#[test]
fn memset_fills_buffer() {
    let (uart, exit) = run_with(
        MEM_SRC,
        r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    buf: u8[4]
    buf_ptr: u8* = buf[0]
    memset(buf_ptr, 88, 4)
    putchar(i32(@buf_ptr))
    putchar(i32(@(buf_ptr + 1)))
    putchar(i32(@(buf_ptr + 2)))
    putchar(i32(@(buf_ptr + 3)))
    return 0
}
"#,
    );
    assert_eq!(uart, "XXXX");
    assert_eq!(exit, Some(0));
}

#[test]
fn memcpy_copies_bytes() {
    let (uart, exit) = run_with(
        MEM_SRC,
        r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    src: u8[3]
    src[0] = 72
    src[1] = 105
    src[2] = 33
    dst: u8[3]
    src_ptr: u8* = src[0]
    dst_ptr: u8* = dst[0]
    memcpy(dst_ptr, src_ptr, 3)
    putchar(i32(@dst_ptr))
    putchar(i32(@(dst_ptr + 1)))
    putchar(i32(@(dst_ptr + 2)))
    return 0
}
"#,
    );
    assert_eq!(uart, "Hi!");
    assert_eq!(exit, Some(0));
}

#[test]
fn memmove_dst_greater_than_src() {
    // memmove copies high-to-low, so dst > src overlaps are handled correctly.
    // buf = [A,B,C,D,E]; memmove(buf[1], buf[0], 4) → [A,A,B,C,D]
    let (uart, exit) = run_with(
        MEM_SRC,
        r#"
external putchar: (c: i32) -> i32
external malloc: (size: u64) -> u8*

main: () -> i32 {
    buf: u8* = malloc(5)
    buf[0] = 65
    buf[1] = 66
    buf[2] = 67
    buf[3] = 68
    buf[4] = 69
    one: u64 = 1
    dst: u8* = buf[one]
    memmove(dst, buf, 4)
    putchar(i32(@buf))
    putchar(i32(@(buf + 1)))
    putchar(i32(@(buf + 2)))
    putchar(i32(@(buf + 3)))
    putchar(i32(@(buf + 4)))
    return 0
}
"#,
    );
    assert_eq!(uart, "AABCD");
    assert_eq!(exit, Some(0));
}

#[test]
fn memcmp_equal_returns_zero() {
    let (uart, exit) = run_with(
        MEM_SRC,
        r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    a: u8[3]
    b: u8[3]
    a[0] = 65
    a[1] = 66
    a[2] = 67
    b[0] = 65
    b[1] = 66
    b[2] = 67
    a_ptr: u8* = a[0]
    b_ptr: u8* = b[0]
    result: i32 = memcmp(a_ptr, b_ptr, 3)
    if result == 0 {
        putchar(80)
        putchar(65)
        putchar(83)
        putchar(83)
    }
    return 0
}
"#,
    );
    assert_eq!(uart, "PASS");
    assert_eq!(exit, Some(0));
}

#[test]
fn memcmp_detects_difference() {
    // a = "AB", b = "AC" → a < b, memcmp returns -1.
    let (uart, exit) = run_with(
        MEM_SRC,
        r#"
external putchar: (c: i32) -> i32

main: () -> i32 {
    a: u8[2]
    b: u8[2]
    a[0] = 65
    a[1] = 66
    b[0] = 65
    b[1] = 67
    a_ptr: u8* = a[0]
    b_ptr: u8* = b[0]
    result: i32 = memcmp(a_ptr, b_ptr, 2)
    if result < 0 {
        putchar(80)
        putchar(65)
        putchar(83)
        putchar(83)
    }
    return 0
}
"#,
    );
    assert_eq!(uart, "PASS");
    assert_eq!(exit, Some(0));
}

// ── klog.hll ────────────────────────────────────────────────────────────────

#[test]
fn klog_ok_output() {
    let (uart, exit) = run_with(
        KLOG_SRC,
        r#"
main: () -> i32 {
    klog_ok("boot".data)
    return 0
}
"#,
    );
    assert_eq!(uart, "[  OK  ] boot\n");
    assert_eq!(exit, Some(0));
}

#[test]
fn klog_error_output() {
    let (uart, exit) = run_with(
        KLOG_SRC,
        r#"
main: () -> i32 {
    klog_error("fault".data)
    return 0
}
"#,
    );
    assert_eq!(uart, "[ ERR  ] fault\n");
    assert_eq!(exit, Some(0));
}

#[test]
fn klog_int_output() {
    let (uart, exit) = run_with(
        KLOG_SRC,
        r#"
main: () -> i32 {
    klog_int("count".data, 42)
    return 0
}
"#,
    );
    assert_eq!(uart, "count: 42\n");
    assert_eq!(exit, Some(0));
}

// ── Kernel boot ─────────────────────────────────────────────────────────────

// Compile user code linked against the kernel stdlib.
// The kernel stdlib is compiled with the "__kern_str_" string-label prefix so
// its rodata labels never clash with user-code labels (which use "str_").
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
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

#[test]
fn kernel_boot_prints_banner() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok: (msg: u8*) -> ()
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    klog_ok("boot".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] boot\n");
    assert_eq!(exit, Some(0));
}

#[test]
fn kernel_boot_multiple_log_levels() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok: (msg: u8*) -> ()
external klog_warn: (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external klog_int: (label: u8*, val: i64) -> ()
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    klog_ok("console init".data)
    klog_warn("heap not ready".data)
    klog_error("test error".data)
    klog_int("hart".data, 0)
    kshutdown(0)
}
"#,
    );
    assert_eq!(
        uart,
        "[  OK  ] console init\n[ WARN ] heap not ready\n[ ERR  ] test error\nhart: 0\n"
    );
    assert_eq!(exit, Some(0));
}

#[test]
fn kernel_boot_kshutdown_nonzero_exit() {
    let (uart, exit) = run_kernel_hll(
        r#"
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    kshutdown(42)
}
"#,
    );
    assert_eq!(uart, "");
    assert_eq!(exit, Some(42));
}

#[test]
fn kernel_boot_warn_error_format() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_warn: (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    klog_warn("memory low".data)
    klog_error("disk failed".data)
    kshutdown(1)
}
"#,
    );
    assert_eq!(uart, "[ WARN ] memory low\n[ ERR  ] disk failed\n");
    assert_eq!(exit, Some(1));
}

#[test]
fn kernel_boot_string_labels_no_collision() {
    // Kernel stdlib (compiled with __kern_str_ prefix) and user code (str_ prefix)
    // both emit string literals. Verifies the two rodata namespaces don't collide.
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok: (msg: u8*) -> ()
external klog_int: (label: u8*, val: i64) -> ()
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    klog_ok("user string A".data)
    klog_int("user string B".data, 99)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] user string A\nuser string B: 99\n");
    assert_eq!(exit, Some(0));
}

const MY_KERNEL_EXAMPLE: &str = os_runtime::kernel::MY_KERNEL;

#[test]
fn my_kernel_example_program() {
    let (uart, exit) = run_kernel_hll(MY_KERNEL_EXAMPLE);
    assert_eq!(
        uart,
        "[  OK  ] kernel starting\n\
         [  OK  ] console online\n\
         boot hart: 0\n\
         [  OK  ] trap handler installed\n\
         [  OK  ] timer armed\n\
         [ WARN ] device tree: not implemented\n\
         [ WARN ] interrupt controller: not implemented\n\
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
         [  OK  ] entering idle loop\n\
         timer tick: 1\n\
         timer tick: 2\n\
         timer tick: 3\n"
    );
    assert_eq!(exit, Some(0));
}

// Verify that linking kernel stdlib with user code that has no `kmain` fails
// at assemble time with an error mentioning `kmain`.
#[test]
fn kernel_boot_missing_kmain_is_assemble_error() {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    // A hosted `main` program - no `kmain` defined.
    let user_pipeline = CompilationPipeline::new();
    let user = user_pipeline
        .compile("main: () -> i32 { return 0 }")
        .expect("user compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);
    let result = user_pipeline.assemble(&linked);
    assert!(result.is_err(), "expected assemble to fail when kmain is missing");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("kmain"),
        "expected error to mention 'kmain', got: {}",
        err.message
    );
}

#[test]
fn kernel_boot_kmalloc_works() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok: (msg: u8*) -> ()
external kmalloc: (size: u64) -> u8*
external kshutdown: (code: i64) -> ()

kmain: () -> () {
    buf: u8* = kmalloc(16)
    buf[0] = 72
    buf[1] = 105
    buf[2] = 33
    buf[3] = 0
    klog_ok(buf)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] Hi!\n");
    assert_eq!(exit, Some(0));
}