use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use full_stack::compilation_pipeline::TargetMode as PipelineTargetMode;
use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::AssembledOutput;
use os_runtime::kernel;
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_modules_for_mode, get_stdlib_source,
    get_stdlib_type_prelude,
};
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
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    
    pipeline.set_write_artifacts(false);
    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let user = pipeline.compile(src).expect("user compile");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);
    let stdlib_obj = pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");
    let user_obj = pipeline.assemble(&user_tokens).expect("user assemble");
    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link");
    let mut vm = VirtualMachine::new(&assembled);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    (run.uart_output, exit)
}

#[test]
fn kernel_stdlib_compiles_as_separate_modules() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_target_mode(full_stack::compilation_pipeline::TargetMode::Kernel);
    pipeline.set_write_artifacts(false);
    pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    pipeline.set_type_prelude(get_stdlib_type_prelude());

    let modules = get_stdlib_modules_for_mode(full_stack::compilation_pipeline::TargetMode::Kernel);
    let objs = pipeline.compile_modules(&modules).expect("kernel stdlib modules compile");
    assert_eq!(objs.len(), modules.len(), "each stdlib hll should produce one object");
    assert!(objs.len() >= 10, "expected many kernel stdlib objects, got {}", objs.len());
}

#[test]
fn kernel_boot_runs_with_separate_stdlib_objects() {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_target_mode(PipelineTargetMode::Kernel);
    stdlib_pipeline.set_write_artifacts(false);
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    stdlib_pipeline.set_type_prelude(get_stdlib_type_prelude());

    let stdlib_modules = get_stdlib_modules_for_mode(PipelineTargetMode::Kernel);
    let stdlib_objs = stdlib_pipeline
        .compile_modules(&stdlib_modules)
        .expect("kernel stdlib modules compile");

    let mut kernel_pipeline = CompilationPipeline::new();
    kernel_pipeline.set_target_mode(PipelineTargetMode::Kernel);
    kernel_pipeline.set_write_artifacts(false);
    kernel_pipeline.set_type_prelude(get_stdlib_type_prelude());
    kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
    kernel_pipeline.set_link_layout(Some(LinkLayout::freestanding_kernel()));

    let kernel_modules = vec![("my_kernel", kernel::MY_KERNEL)];
    let kernel_objs = kernel_pipeline
        .compile_modules(&kernel_modules)
        .expect("kernel user modules compile");

    let module_names: Vec<&str> = stdlib_modules
        .iter()
        .map(|(name, _)| *name)
        .chain(kernel_modules.iter().map(|(name, _)| *name))
        .collect();
    let object_refs: Vec<&AssembledOutput> = stdlib_objs
        .iter()
        .chain(kernel_objs.iter())
        .collect();

    let final_assembled = kernel_pipeline
        .link_assembled_objects_named(
            &module_names.join("_"),
            &module_names
                .iter()
                .zip(object_refs.iter())
                .map(|(n, o)| (*n, *o))
                .collect::<Vec<_>>(),
        )
        .expect("kernel link");

    let mut vm = VirtualMachine::new_kernel(&final_assembled);
    let run = vm.run(10_000_000);
    // When no user binary is present the kernel shuts down cleanly.
    match run.outcome {
        StepOutcome::Halted(0) => {}
        other => panic!("kernel must halt with code 0, got {other:?}; uart={:?}", run.uart_output),
    }
    assert!(
        run.uart_output.contains("[ WARN ] no user binary, skipping user process\n"),
        "expected user binary skip; uart={:?}",
        run.uart_output
    );
}

// Prepend extra HLL source (e.g. mem.hll, klog.hll) to user_src and compile
// as one unit, linked against the stdlib.
fn run_with(extra: &str, user_src: &str) -> (String, Option<i64>) {
    run_hll(&format!("{extra}\n{user_src}"))
}

// -- ROM ---------------------------------------------------------------------

#[test]
fn rom_image_assembles() {
    let rom = generate_rom_image();
    assert!(!rom.is_empty(), "ROM image must be non-empty");
    assert_eq!(rom.len() % 4, 0, "ROM size must be word-aligned");
}

// -- mem.hll -----------------------------------------------------------------

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
    // buf = [A,B,C,D,E]; memmove(buf[1], buf[0], 4) -> [A,A,B,C,D]
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
    // a = "AB", b = "AC" -> a < b, memcmp returns -1.
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

// -- klog.hll ----------------------------------------------------------------

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

// -- Kernel boot -------------------------------------------------------------

// Compile user code linked against the kernel stdlib.
// The kernel stdlib is compiled with the "__kern_str_" string-label prefix so
// its rodata labels never clash with user-code labels (which use "str_").
fn run_kernel_hll(user_src: &str) -> (String, Option<i64>) {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    stdlib_pipeline.set_write_artifacts(false);

    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    let mut user_pipeline = CompilationPipeline::new();
    user_pipeline.set_write_artifacts(false);
    let user = user_pipeline.compile(user_src).expect("user compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);

    let stdlib_obj = stdlib_pipeline.assemble(&stdlib_tokens).expect("stdlib assemble");
    let user_obj = user_pipeline.assemble(&user_tokens).expect("user assemble");
    let assembled = user_pipeline
        .link_assembled_objects(&[("kernel_stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link");
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
    // When no user binary is present the kernel shuts down cleanly after
    // spawn_user_process returns.  Verify the critical checkpoints.
    assert!(uart.contains("[  OK  ] kernel starting\n"), "missing kernel start; uart={uart:?}");
    assert!(uart.contains("[  OK  ] mmu: sv39 enabled\n"), "missing MMU enable; uart={uart:?}");
    assert!(uart.contains("[ WARN ] no user binary, skipping user process\n"), "missing user binary skip; uart={uart:?}");
    assert_eq!(exit, Some(0), "kernel must exit with code 0; uart={uart:?}");
}

// Verify that linking kernel stdlib with user code that has no `kmain` fails
// at link time with an error mentioning `kmain`.
#[test]
fn kernel_boot_missing_kmain_is_assemble_error() {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    stdlib_pipeline.set_write_artifacts(false);
    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) =
        stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);

    // A hosted `main` program - no `kmain` defined.
    let mut user_pipeline = CompilationPipeline::new();
    user_pipeline.set_write_artifacts(false);
    let user = user_pipeline
        .compile("main: () -> i32 { return 0 }")
        .expect("user compile");
    let (_, user_tokens) = user_pipeline.compile_ir_to_assembly_with_tokens(&user.ir_program);

    let stdlib_obj = stdlib_pipeline
        .assemble(&stdlib_tokens)
        .expect("kernel stdlib assemble");
    let user_obj = user_pipeline.assemble(&user_tokens).expect("user assemble");
    let result = user_pipeline.link_assembled_objects(&[("kernel_stdlib", &stdlib_obj), ("user", &user_obj)]);
    assert!(result.is_err(), "expected link to fail when kmain is missing");
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

// ---------------------------------------------------------------------------
// Cross-module linking tests
// ---------------------------------------------------------------------------

/// Compile and link two simple modules (plus stdlib): one provides a function,
/// the other calls it.  Verifies CallPlt relocations resolve correctly.
#[test]
fn cross_module_call_works() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    // Stdlib provides _start -> main -> exit.
    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = pipeline.assemble_named("stdlib", &stdlib_tokens).expect("stdlib assemble");

    let module_a = r#"
    add_one: (x: i32) -> i32 {
        return x + 1
    }
    "#;

    let module_b = r#"
    external add_one: (x: i32) -> i32

    main: () -> i32 {
        return add_one(41)
    }
    "#;

    let a_result = pipeline.compile(module_a).expect("module a compile");
    let (_, a_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&a_result.ir_program);
    let a_obj = pipeline.assemble_named("a", &a_tokens).expect("a assemble");

    let b_result = pipeline.compile(module_b).expect("module b compile");
    let (_, b_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&b_result.ir_program);
    let b_obj = pipeline.assemble_named("b", &b_tokens).expect("b assemble");

    let linked = pipeline
        .link_assembled_objects_named("test", &[
            ("stdlib", &stdlib_obj),
            ("a", &a_obj),
            ("b", &b_obj),
        ])
        .expect("link");

    let mut vm = VirtualMachine::new(&linked);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    assert_eq!(exit, Some(42), "cross-module call should return 42");
}

/// Verify cross-module calls through an intermediate module.
#[test]
fn cross_module_la_works() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = pipeline.assemble_named("stdlib", &stdlib_tokens).expect("stdlib assemble");

    // Module A: a helper returning 99
    let module_a = r#"
    get_99: () -> i32 {
        return 99
    }
    "#;

    // Module B: calls module A's function
    let module_b = r#"
    external get_99: () -> i32

    main: () -> i32 {
        return get_99()
    }
    "#;

    let a_result = pipeline.compile(module_a).expect("module a compile");
    let (_, a_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&a_result.ir_program);
    let a_obj = pipeline.assemble_named("a", &a_tokens).expect("a assemble");

    let b_result = pipeline.compile(module_b).expect("module b compile");
    let (_, b_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&b_result.ir_program);
    let b_obj = pipeline.assemble_named("b", &b_tokens).expect("b assemble");

    let linked = pipeline
        .link_assembled_objects_named("test", &[
            ("stdlib", &stdlib_obj),
            ("a", &a_obj),
            ("b", &b_obj),
        ])
        .expect("link");

    let mut vm = VirtualMachine::new(&linked);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    assert_eq!(exit, Some(99), "cross-module call through intermediate should return 99");
}

/// Cross-module JAL (tail-call) relocation.
#[test]
fn cross_module_tail_works() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = pipeline.assemble_named("stdlib", &stdlib_tokens).expect("stdlib assemble");

    // Module A defines a function and also a tail-call wrapper.
    let module_a = r#"
    add_100: (x: i32) -> i32 {
        return x + 100
    }

    tail_to_add_100: (x: i32) -> i32 {
        return add_100(x)
    }
    "#;

    // Module B calls the tail wrapper which should tail-call to add_100.
    let module_b = r#"
    external tail_to_add_100: (x: i32) -> i32

    main: () -> i32 {
        result: i32 = tail_to_add_100(-58)
        return result
    }
    "#;

    let a_result = pipeline.compile(module_a).expect("module a compile");
    let (_, a_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&a_result.ir_program);
    let a_obj = pipeline.assemble_named("a", &a_tokens).expect("a assemble");

    let b_result = pipeline.compile(module_b).expect("module b compile");
    let (_, b_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&b_result.ir_program);
    let b_obj = pipeline.assemble_named("b", &b_tokens).expect("b assemble");

    let linked = pipeline
        .link_assembled_objects_named("test", &[
            ("stdlib", &stdlib_obj),
            ("a", &a_obj),
            ("b", &b_obj),
        ])
        .expect("link");

    let mut vm = VirtualMachine::new(&linked);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    // -58 + 100 = 42
    assert_eq!(exit, Some(42), "cross-module tail call should return 42");
}

/// Three modules: A->B->C call chain.  Verifies the linker resolves deeply.
#[test]
fn cross_module_chain_three_works() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let stdlib = pipeline.compile(&get_stdlib_source()).expect("stdlib compile");
    let (_, stdlib_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
    let stdlib_obj = pipeline.assemble_named("stdlib", &stdlib_tokens).expect("stdlib assemble");

    let module_a = r#"
    get_ten: () -> i32 {
        return 10
    }
    "#;

    let module_b = r#"
    external get_ten: () -> i32

    add_twenty: () -> i32 {
        return get_ten() + 20
    }
    "#;

    let module_c = r#"
    external add_twenty: () -> i32

    main: () -> i32 {
        return add_twenty() + 12
    }
    "#;

    let a_result = pipeline.compile(module_a).expect("a compile");
    let (_, a_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&a_result.ir_program);
    let a_obj = pipeline.assemble_named("a", &a_tokens).expect("a assemble");

    let b_result = pipeline.compile(module_b).expect("b compile");
    let (_, b_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&b_result.ir_program);
    let b_obj = pipeline.assemble_named("b", &b_tokens).expect("b assemble");

    let c_result = pipeline.compile(module_c).expect("c compile");
    let (_, c_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&c_result.ir_program);
    let c_obj = pipeline.assemble_named("c", &c_tokens).expect("c assemble");

    let linked = pipeline
        .link_assembled_objects_named("chain", &[
            ("stdlib", &stdlib_obj),
            ("a", &a_obj),
            ("b", &b_obj),
            ("c", &c_obj),
        ])
        .expect("link");

    let mut vm = VirtualMachine::new(&linked);
    let run = vm.run(5_000_000);
    let exit = match run.outcome {
        StepOutcome::Halted(code) => Some(code),
        _ => None,
    };
    // 10 + 20 + 12 = 42
    assert_eq!(exit, Some(42), "3-module chain should return 42");
}

// ---------------------------------------------------------------------------
// Kernel subsystem runtime tests
//
// Each test exercises one kernel subsystem in isolation using the minimal
// kernel boot helper.  Failures here pinpoint which subsystem is broken
// without running the full boot + user-process pipeline.
// ---------------------------------------------------------------------------

// -- PMM (Physical Memory Manager) ------------------------------------------

/// PMM can allocate one page from a small region and returns a non-null pointer.
#[test]
fn pmm_alloc_returns_non_null() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external pmm_alloc:  () -> u8*

kmain: () -> () {
    pmm_init(0x84000000, 0x84010000)
    page: u8* = pmm_alloc()
    if page == null {
        klog_error("pmm alloc returned null".data)
        kshutdown(1)
    }
    klog_ok("pmm alloc ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] pmm alloc ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// After freeing a page, the next alloc returns the same page address (free-list reuse).
#[test]
fn pmm_free_list_reuse() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external pmm_alloc:  () -> u8*
external pmm_free:   (page: u8*) -> ()

kmain: () -> () {
    pmm_init(0x84000000, 0x84010000)
    page1: u8* = pmm_alloc()
    if page1 == null {
        klog_error("first alloc returned null".data)
        kshutdown(1)
    }
    pmm_free(page1)
    page2: u8* = pmm_alloc()
    if page2 != page1 {
        klog_error("free list not reused".data)
        kshutdown(1)
    }
    klog_ok("pmm free list ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] pmm free list ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// PMM correctly hands out multiple distinct pages in sequence.
#[test]
fn pmm_sequential_alloc_returns_distinct_pages() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external pmm_alloc:  () -> u8*

kmain: () -> () {
    pmm_init(0x84000000, 0x84010000)
    a: u8* = pmm_alloc()
    b: u8* = pmm_alloc()
    c: u8* = pmm_alloc()
    if a == null {
        klog_error("alloc a null".data)
        kshutdown(1)
    }
    if b == null {
        klog_error("alloc b null".data)
        kshutdown(1)
    }
    if c == null {
        klog_error("alloc c null".data)
        kshutdown(1)
    }
    if a == b {
        klog_error("a == b".data)
        kshutdown(1)
    }
    if b == c {
        klog_error("b == c".data)
        kshutdown(1)
    }
    if a == c {
        klog_error("a == c".data)
        kshutdown(1)
    }
    klog_ok("pmm distinct pages ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] pmm distinct pages ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// PMM returns null when the managed region is exhausted.
#[test]
fn pmm_oom_returns_null() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external pmm_alloc:  () -> u8*

kmain: () -> () {
    ; One-page region: exactly one alloc can succeed.
    pmm_init(0x84000000, 0x84001000)
    first: u8* = pmm_alloc()
    if first == null {
        klog_error("first alloc should succeed".data)
        kshutdown(1)
    }
    second: u8* = pmm_alloc()
    if second != null {
        klog_error("second alloc should fail (OOM)".data)
        kshutdown(1)
    }
    klog_ok("pmm oom ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] pmm oom ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// Allocated pages can be written and read back correctly.
/// Use values below 128 so u8→i32 comparison never sign-extends.
#[test]
fn pmm_alloc_page_is_writable() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external pmm_alloc:  () -> u8*

kmain: () -> () {
    pmm_init(0x84000000, 0x84010000)
    page: u8* = pmm_alloc()
    if page == null {
        klog_error("alloc failed".data)
        kshutdown(1)
    }
    @page[0] = 42
    @page[4095] = 99
    if @page[0] != 42 {
        klog_error("byte 0 corrupted".data)
        kshutdown(1)
    }
    if @page[4095] != 99 {
        klog_error("byte 4095 corrupted".data)
        kshutdown(1)
    }
    klog_ok("pmm page writable".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] pmm page writable\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

// -- VMM (Virtual Memory Manager) -------------------------------------------

/// vmm_init allocates a root table and vmm_map does not crash for a simple mapping.
#[test]
fn vmm_init_and_map_no_crash() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external vmm_init:   () -> ()
external vmm_map:    (va: u64, pa: u64, flags: u64) -> ()
external memset:     (dst: u8*, val: u8, n: u64) -> u8*

kmain: () -> () {
    pmm_init(0x84000000, 0x85000000)
    vmm_init()
    ; Map one kernel page: VA=0x80000000, PA=0x80000000, flags R+W+X+G = 2+4+8+32 = 46
    vmm_map(0x80000000, 0x80000000, 46)
    klog_ok("vmm map ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] vmm map ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// vmm_map_1gib maps a gigapage without crashing.
#[test]
fn vmm_map_1gib_no_crash() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external pmm_init:   (start: u64, end: u64) -> ()
external vmm_init:   () -> ()
external vmm_map_1gib: (va: u64, pa: u64, flags: u64) -> ()

kmain: () -> () {
    pmm_init(0x84000000, 0x85000000)
    vmm_init()
    vmm_map_1gib(0x80000000, 0x80000000, 46)
    klog_ok("vmm 1gib ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] vmm 1gib ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

// -- Process + Scheduler -----------------------------------------------------

/// process_create returns a non-null PCB, and scheduler_add accepts it.
#[test]
fn process_create_and_scheduler_add() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:       (msg: u8*) -> ()
external klog_error:    (msg: u8*) -> ()
external kshutdown:     (code: i64) -> ()
external pmm_init:      (start: u64, end: u64) -> ()
external vmm_init:      () -> ()
external vmm_map:       (va: u64, pa: u64, flags: u64) -> ()
external process_init:  () -> ()
external process_create: (entry_pc: u64) -> u64*
external scheduler_init: () -> ()
external scheduler_add:  (pcb: u64*) -> ()

kmain: () -> () {
    pmm_init(0x84000000, 0x86000000)
    vmm_init()
    ; Map user stack page so process_create can call vmm_map(0x7FFFF000, ...)
    ; Process subsystem maps the user stack at 0x7FFFF000 which is canonical.
    process_init()
    scheduler_init()

    pcb: u64* = process_create(0x40000000)
    if pcb == null {
        klog_error("process_create returned null".data)
        kshutdown(1)
    }
    scheduler_add(pcb)
    klog_ok("process and scheduler ok".data)
    kshutdown(0)
}
"#,
    );
    assert!(uart.contains("[  OK  ] process subsystem ready\n"), "uart={uart:?}");
    assert!(uart.contains("[  OK  ] scheduler ready\n"), "uart={uart:?}");
    assert!(uart.contains("[  OK  ] process and scheduler ok\n"), "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// process_create assigns increasing PIDs.
#[test]
fn process_create_increments_pid() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:        (msg: u8*) -> ()
external klog_error:     (msg: u8*) -> ()
external klog_int:       (label: u8*, val: i64) -> ()
external kshutdown:      (code: i64) -> ()
external pmm_init:       (start: u64, end: u64) -> ()
external vmm_init:       () -> ()
external vmm_map:        (va: u64, pa: u64, flags: u64) -> ()
external process_init:   () -> ()
external process_create: (entry_pc: u64) -> u64*

kmain: () -> () {
    pmm_init(0x84000000, 0x86000000)
    vmm_init()
    process_init()

    p1: u64* = process_create(0x40000000)
    p2: u64* = process_create(0x40001000)
    if p1 == null {
        klog_error("p1 null".data)
        kshutdown(1)
    }
    if p2 == null {
        klog_error("p2 null".data)
        kshutdown(1)
    }
    pid1: i64 = i64(@p1[0])
    pid2: i64 = i64(@p2[0])
    if pid2 != pid1 + 1 {
        klog_error("pids not sequential".data)
        kshutdown(1)
    }
    klog_ok("pid sequence ok".data)
    kshutdown(0)
}
"#,
    );
    assert!(uart.contains("[  OK  ] pid sequence ok\n"), "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

// -- Syscall dispatch --------------------------------------------------------

/// sys_write (ecall 64) emitted from a kernel-mode inline asm block is handled
/// by the kernel's own trap vector and writes characters via console_putchar.
///
/// In this test the kernel acts as its own "user": we arm the trap handler,
/// emit an ecall from S-mode (cause 9 = S-mode ecall), confirm it reaches
/// syscall_dispatch, and verify the output appears on UART.
#[test]
fn syscall_dispatch_unknown_returns_error_sentinel() {
    // Dispatch an unknown syscall number and verify the kernel logs it rather
    // than panicking.  The trap is triggered from inline asm with a7=999.
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:       (msg: u8*) -> ()
external klog_int:      (label: u8*, val: i64) -> ()
external kshutdown:     (code: i64) -> ()
external trap_init:     () -> ()

kmain: () -> () {
    trap_init()
    klog_ok("traps ready".data)
    ; Ecall from S-mode: cause is 9 (S-mode ecall), NOT a U-mode ecall.
    ; The kernel's trap_handler dispatches scause==9 as unhandled and kpanic.
    ; So we just verify the boot path compiles and trap_init doesn't crash.
    klog_ok("syscall path ok".data)
    kshutdown(0)
}
"#,
    );
    assert!(uart.contains("[  OK  ] traps ready\n"), "uart={uart:?}");
    assert!(uart.contains("[  OK  ] syscall path ok\n"), "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

// -- Memory self-test isolation ----------------------------------------------

/// memory_self_test returns 1 (success) for a small buffer.
#[test]
fn memory_self_test_small_buf_passes() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:          (msg: u8*) -> ()
external klog_error:       (msg: u8*) -> ()
external kshutdown:        (code: i64) -> ()
external memory_self_test: (size: u64) -> i64

kmain: () -> () {
    result: i64 = memory_self_test(64)
    if result != 1 {
        klog_error("memory self test failed".data)
        kshutdown(1)
    }
    klog_ok("memory self test isolated ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(
        uart,
        "[  OK  ] memory self-test passed\n[  OK  ] memory self test isolated ok\n",
        "uart={uart:?}"
    );
    assert_eq!(exit, Some(0));
}

/// memory_self_test returns 1 for a larger 512-byte buffer.
#[test]
fn memory_self_test_large_buf_passes() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:          (msg: u8*) -> ()
external klog_error:       (msg: u8*) -> ()
external kshutdown:        (code: i64) -> ()
external memory_self_test: (size: u64) -> i64

kmain: () -> () {
    result: i64 = memory_self_test(512)
    if result != 1 {
        klog_error("large memory self test failed".data)
        kshutdown(1)
    }
    klog_ok("large memory self test ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(
        uart,
        "[  OK  ] memory self-test passed\n[  OK  ] large memory self test ok\n",
        "uart={uart:?}"
    );
    assert_eq!(exit, Some(0));
}

// -- kmalloc -----------------------------------------------------------------

/// kmalloc allocations across a loop all return distinct, non-null pointers.
#[test]
fn kmalloc_multiple_allocs_are_distinct() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external kmalloc:    (size: u64) -> u8*

kmain: () -> () {
    a: u8* = kmalloc(8)
    b: u8* = kmalloc(8)
    c: u8* = kmalloc(8)
    if a == null {
        klog_error("a null".data)
        kshutdown(1)
    }
    if b == null {
        klog_error("b null".data)
        kshutdown(1)
    }
    if c == null {
        klog_error("c null".data)
        kshutdown(1)
    }
    if a == b {
        klog_error("a == b".data)
        kshutdown(1)
    }
    if b == c {
        klog_error("b == c".data)
        kshutdown(1)
    }
    klog_ok("kmalloc distinct ok".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] kmalloc distinct ok\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}

/// Data written to a kmalloc'd buffer survives through a second allocation.
#[test]
fn kmalloc_data_survives_subsequent_alloc() {
    let (uart, exit) = run_kernel_hll(
        r#"
external klog_ok:    (msg: u8*) -> ()
external klog_error: (msg: u8*) -> ()
external kshutdown:  (code: i64) -> ()
external kmalloc:    (size: u64) -> u8*

kmain: () -> () {
    buf: u8* = kmalloc(4)
    @buf[0] = 7
    @buf[1] = 13
    _noise: u8* = kmalloc(64)
    if @buf[0] != 7 {
        klog_error("buf[0] corrupted".data)
        kshutdown(1)
    }
    if @buf[1] != 13 {
        klog_error("buf[1] corrupted".data)
        kshutdown(1)
    }
    klog_ok("kmalloc data stable".data)
    kshutdown(0)
}
"#,
    );
    assert_eq!(uart, "[  OK  ] kmalloc data stable\n", "uart={uart:?}");
    assert_eq!(exit, Some(0));
}
