// Consolidated kernel integration suite.
//
// Every test here needs a booted VM, which is the slowest thing in the project.
// To keep the suite fast the kernel is compiled at most twice for the whole run
// (single-concat stdlib + the multi-module "GUI" link path) via OnceLock caches,
// and a single boot drives each scenario. This file replaces the former
// kernel_boot_device_tree, kernel_user_injection, kernel_example_injection,
// kernel_shell, kernel_editor, kernel_fork, kernel_isolation, and kernel_fs
// files, which each recompiled the kernel independently.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{
    assembled_to_elf_file, build_fs_image, CompilationPipeline, FsEntry, TargetMode,
};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_modules_for_mode, get_stdlib_source_for_mode,
    get_stdlib_type_prelude,
};
use os_runtime::{kernel, user};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Physical layout constants matching the OS spec.
const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

// --- Cached kernel binaries ---

// Kernel built from the single-concatenated stdlib source plus my_kernel. This
// is what most tests boot.
fn cached_kernel() -> &'static AssembledOutput {
    static KERNEL: OnceLock<AssembledOutput> = OnceLock::new();
    KERNEL.get_or_init(|| {
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
                &[
                    ("kernel_stdlib", &stdlib_obj),
                    ("my_kernel", &kernel_objs[0]),
                ],
            )
            .expect("kernel link")
    })
}

// Kernel built the way the GUI does it: each stdlib module compiled separately
// and linked together. This covers the separate-object link path (e.g. the
// "9 enable" page-fault regression) which the single-concat path does not.
fn cached_kernel_multi_module() -> &'static AssembledOutput {
    static KERNEL: OnceLock<AssembledOutput> = OnceLock::new();
    KERNEL.get_or_init(|| {
        let mut stdlib_pipeline = CompilationPipeline::new();
        stdlib_pipeline.set_target_mode(TargetMode::Kernel);
        stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
        stdlib_pipeline.set_write_artifacts(false);
        stdlib_pipeline.set_type_prelude(get_stdlib_type_prelude());

        let modules = get_stdlib_modules_for_mode(TargetMode::Kernel);
        let stdlib_objects = stdlib_pipeline
            .compile_modules(&modules.iter().map(|(n, s)| (*n, *s)).collect::<Vec<_>>())
            .expect("stdlib multi-module compile");

        let mut kernel_pipeline = CompilationPipeline::new();
        kernel_pipeline.set_target_mode(TargetMode::Kernel);
        kernel_pipeline.set_write_artifacts(false);
        kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));

        let kernel_modules = vec![("my_kernel", kernel::MY_KERNEL)];
        let kernel_objects = kernel_pipeline
            .compile_modules(&kernel_modules)
            .expect("kernel modules compile");

        let module_names: Vec<&str> = modules.iter().map(|(n, _)| *n).collect();
        let mut all_names: Vec<&str> = module_names.clone();
        all_names.extend(kernel_modules.iter().map(|(n, _)| *n));

        let mut object_refs: Vec<&AssembledOutput> = stdlib_objects.iter().collect();
        for obj in &kernel_objects {
            object_refs.push(obj);
        }

        kernel_pipeline
            .link_assembled_objects_named(
                &all_names.join("_"),
                &all_names
                    .iter()
                    .zip(object_refs.iter())
                    .map(|(n, o)| (*n, *o))
                    .collect::<Vec<_>>(),
            )
            .expect("kernel link")
    })
}

// --- Shared helpers ---

// Compile a hosted user program (links the hosted stdlib).
fn compile_hosted(src: &str) -> AssembledOutput {
    let full = format!(
        "{}\n{}",
        get_stdlib_source_for_mode(TargetMode::Hosted),
        src
    );
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_target_mode(TargetMode::Hosted);
    pipeline.set_write_artifacts(false);
    pipeline.set_type_prelude(get_stdlib_type_prelude());
    let result = pipeline.compile(&full).expect("hosted compile");
    let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    pipeline.assemble(&tokens).expect("hosted assemble")
}

// Boot the kernel, optionally injecting an FS image and a pid-1 user binary, and
// optionally pre-loading a UART session. Returns the VM (for post-run peeks),
// the final outcome, and the captured UART output.
// Set up a booted kernel VM (RAM layout + optional FS image + pid-1 user binary
// + front-loaded UART input) without running it, so callers can either run it in
// one shot (boot_kernel) or step it manually and inject input mid-run.
fn setup_kernel_vm(
    kernel: &AssembledOutput,
    user: Option<&AssembledOutput>,
    fs_image: Option<&[u8]>,
    input: &str,
) -> VirtualMachine {
    let mut vm = VirtualMachine::new_kernel(kernel);

    if let Some(image) = fs_image {
        vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes())
            .expect("write fs image PA");
        vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes())
            .expect("write fs image size");
        vm.write_ram(FS_IMAGE_PA, image).expect("write fs image");
    }

    if let Some(u) = user {
        let mut flat = u.to_flat_binary();
        let page = 4096usize;
        flat.resize(flat.len().div_ceil(page) * page, 0u8);
        let entry_off = u.symbol_address("_start").expect("_start missing");
        let entry_va = USER_CODE_VA + entry_off;
        let user_size = flat.len() as u64;
        vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
            .expect("write user entry VA");
        vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
            .expect("write user size");
        vm.write_ram(USER_BINARY_PA, &flat)
            .expect("write user binary");
    }

    for b in input.bytes() {
        vm.push_uart_rx(b);
    }

    vm
}

fn boot_kernel(
    kernel: &AssembledOutput,
    user: Option<&AssembledOutput>,
    fs_image: Option<&[u8]>,
    input: &str,
    max_steps: u64,
) -> (VirtualMachine, StepOutcome, String) {
    let mut vm = setup_kernel_vm(kernel, user, fs_image, input);
    let run = vm.run(max_steps);
    (vm, run.outcome, run.uart_output)
}

// A hosted user program injected as pid 1 must spawn and exit with code 0.
fn assert_user_exit_ok(uart: &str, outcome: &StepOutcome, label: &str) {
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
    // A pid-1 program exiting cleanly calls kshutdown(0), halting the VM with 0.
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "{label}: user process did not exit cleanly with code 0; got {outcome:?}, uart={uart:?}"
    );
}

// Compile a hosted program and inject it as pid 1 (no FS image), then assert a
// clean spawn-and-exit.
fn run_example_in_kernel(user_src: &str, label: &str) {
    let user = compile_hosted(user_src);
    let (_, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 10_000_000);
    assert_user_exit_ok(&uart, &outcome, label);
}

// --- Boot sequence (merged from kernel_boot_device_tree.rs + kernel_user_injection.rs) ---
// All of these formerly recompiled the kernel and asserted on different parts of
// the same boot. One boot now covers the whole sequence.

#[test]
fn kernel_boot_full_sequence() {
    let user = compile_hosted(user::USER_HELLO);
    let (_, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 10_000_000);

    match outcome {
        StepOutcome::Continue => {} // idle loop, expected
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}; uart={uart:?}"),
    }

    // Banner and init order: console -> device tree -> memory diagnostics.
    assert!(
        uart.contains("[  OK  ] kernel starting\n"),
        "expected boot banner; uart={uart:?}"
    );
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

    // Self-tests and arming.
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "expected memory self-test to pass; uart={uart:?}"
    );
    assert!(
        !uart.contains("memory self-test failed"),
        "memory self-test must not fail; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] heap ready\n"),
        "expected heap smoke-test; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] timer armed\n"),
        "expected timer armed; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "expected boot complete; uart={uart:?}"
    );

    // User process spawns and greets.
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "expected user process spawn; uart={uart:?}"
    );
    assert!(
        uart.contains("hello from user mode!\n"),
        "expected user-mode greeting; uart={uart:?}"
    );
}

#[test]
fn kernel_boot_multi_module_with_user() {
    let user = compile_hosted(user::USER_HELLO);
    let (_, outcome, uart) = boot_kernel(
        cached_kernel_multi_module(),
        Some(&user),
        None,
        "",
        10_000_000,
    );
    match outcome {
        StepOutcome::Continue => {}
        StepOutcome::Halted(c) => panic!("unexpected halt with code {c}; uart={uart:?}"),
    }
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "expected boot complete; uart={uart:?}"
    );
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "expected user process spawn; uart={uart:?}"
    );
    assert!(
        uart.contains("hello from user mode!\n"),
        "expected user-mode greeting; uart={uart:?}"
    );
}

#[test]
fn kernel_boot_multi_module_no_user_binary() {
    let (_, outcome, uart) = boot_kernel(cached_kernel_multi_module(), None, None, "", 10_000_000);
    // Without a user binary the kernel calls kshutdown(0) after spawn returns.
    match outcome {
        StepOutcome::Halted(0) => {}
        other => panic!("expected clean shutdown, got {other:?}; uart={uart:?}"),
    }
    assert!(
        uart.contains("[ PROC ] no user binary present"),
        "expected user binary skip; uart={uart:?}"
    );
    assert!(
        uart.contains("[ VMM ] sv39 enabled\n"),
        "expected MMU enable; uart={uart:?}"
    );
}

// --- Example programs in kernel userspace (merged from kernel_example_injection.rs) ---

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

#[test]
fn example_core_basics_runs_in_kernel_userspace() {
    run_example_in_kernel(CORE_BASICS, "core_basics");
}

#[test]
fn example_pointer_arrays_runs_in_kernel_userspace() {
    run_example_in_kernel(POINTER_ARRAYS, "pointer_arrays");
}

#[test]
fn example_array_initialization_runs_in_kernel_userspace() {
    run_example_in_kernel(ARRAY_INITIALIZATION, "array_initialization");
}

#[test]
fn example_struct_binding_runs_in_kernel_userspace() {
    run_example_in_kernel(STRUCT_BINDING, "struct_binding");
}

#[test]
fn example_control_flow_basics_runs_in_kernel_userspace() {
    run_example_in_kernel(CONTROL_FLOW_BASICS, "control_flow_basics");
}

#[test]
fn example_casting_and_pointers_runs_in_kernel_userspace() {
    run_example_in_kernel(CASTING_AND_POINTERS, "casting_and_pointers");
}

#[test]
fn example_compile_time_math_runs_in_kernel_userspace() {
    run_example_in_kernel(COMPILE_TIME_MATH, "compile_time_math");
}

#[test]
fn example_generics_and_strings_runs_in_kernel_userspace() {
    run_example_in_kernel(GENERICS_AND_STRINGS, "generics_and_strings");
}

#[test]
fn user_malloc_and_free_work_in_kernel_userspace() {
    run_example_in_kernel(
        r#"
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
"#,
        "user_malloc_and_free",
    );
}

#[test]
fn user_free_then_realloc_reuses_block() {
    run_example_in_kernel(
        r#"
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
"#,
        "user_free_then_realloc",
    );
}

// --- Framebuffer device (map_fb syscall + fbdemo program) ---

// Reference Mandelbrot membership using the exact Q16.16 integer math from
// fbdemo.hll. Returns true when the point is inside the set (black pixel).
// Kept identical to the HLL so the VM render can be compared pixel-for-pixel;
// any codegen distortion (e.g. a mixed-width truncation that shears the set)
// shows up as a large disagreement here.
fn fbdemo_ref_in_set(cx: i64, cy: i64) -> bool {
    const ONE: i64 = 65536;
    const FOUR: i64 = 262144;
    const MAXIT: i64 = 64;
    let (mut zx, mut zy, mut it) = (0i64, 0i64, 0i64);
    while it < MAXIT {
        let zx2 = (zx * zx) >> 16;
        let zy2 = (zy * zy) >> 16;
        if zx2 + zy2 >= FOUR {
            break;
        }
        let nzy = (2 * zx * zy) / ONE + cy;
        zx = zx2 - zy2 + cx;
        zy = nzy;
        it += 1;
    }
    it >= MAXIT
}

// Boot fbdemo as pid 1, read the framebuffer back, and check the rendered image
// against the reference fractal. Covers the whole path: map_fb -> MMU
// translation -> bus routing -> device store, plus the compiler's integer
// codegen (the set is sheared if a wide multiply is truncated to 32 bits).
#[test]
fn fbdemo_renders_mandelbrot_set() {
    const FB_W: usize = 320;
    const FB_H: usize = 240;
    const XMIN: i64 = -163840;
    const YMIN: i64 = -86016;
    const STEP: i64 = 717;

    let user = compile_hosted(user::MANDELBROT);
    let (vm, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 2_000_000_000);
    assert_user_exit_ok(&uart, &outcome, "mandelbrot");
    assert!(
        uart.contains("mandelbrot: rendered"),
        "mandelbrot did not report success; uart={uart:?}"
    );

    let px = vm.peek_framebuffer();

    // Every pixel must be fully opaque (the alpha byte is always written).
    for (i, p) in px.chunks_exact(4).enumerate() {
        assert_eq!(p[3], 255, "pixel {i} not opaque: {p:?}");
    }

    // Compare the rendered set membership (black vs coloured) to the reference
    // for every pixel. With correct codegen these agree exactly; a sheared or
    // truncated render disagrees on thousands of pixels.
    let mut in_set = 0usize;
    let mut mismatches = 0usize;
    for y in 0..FB_H {
        for x in 0..FB_W {
            let cx = XMIN + (x as i64) * STEP;
            let cy = YMIN + (y as i64) * STEP;
            let want_black = fbdemo_ref_in_set(cx, cy);
            if want_black {
                in_set += 1;
            }
            let o = (y * FB_W + x) * 4;
            let got_black = px[o] == 0 && px[o + 1] == 0 && px[o + 2] == 0;
            if got_black != want_black {
                mismatches += 1;
            }
        }
    }

    // Sanity: the window genuinely contains a chunk of the set.
    assert!(
        in_set > 10_000,
        "reference window has too little set: {in_set} px"
    );
    // The render must match the reference fractal almost exactly.
    assert!(
        mismatches < (FB_W * FB_H) / 200,
        "rendered set disagrees with reference on {mismatches} pixels (likely a \
         codegen distortion); in_set={in_set}"
    );
}

// Boot the spinning-cube demo as pid 1 for a bounded number of cycles (it loops
// forever) and confirm it compiled, mapped the framebuffer, and drove the
// per-pixel store path. A timeout almost always lands inside the full-screen
// clear, so we assert on the clear (opaque black) rather than the edge pixels,
// which would be timing-dependent.
#[test]
fn cube_demo_drives_framebuffer() {
    let user = compile_hosted(user::CUBE);
    let (vm, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 10_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "unhandled CPU exception; uart={uart:?}"
    );
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "cube process was not spawned; uart={uart:?}"
    );
    assert!(
        uart.contains("cube: rendering"),
        "cube did not reach its render loop; uart={uart:?}"
    );
    // It never exits, so the bounded run must still be ticking, not halted.
    assert!(
        matches!(outcome, StepOutcome::Continue),
        "cube should run continuously, got {outcome:?}"
    );

    // A large opaque-black region proves the FILL clear path ran.
    let px = vm.peek_framebuffer();
    let opaque_black = px
        .chunks_exact(4)
        .filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 255)
        .count();
    assert!(
        opaque_black > 10_000,
        "expected the clear to fill the framebuffer, got {opaque_black} black pixels"
    );
    // The device-side FILL must actually have run at least once in this budget.
    assert!(
        vm.framebuffer_fill_count() >= 1,
        "cube did not clear the framebuffer via the FILL register"
    );
}

// Boot the Game of Life demo as pid 1 for a bounded number of cycles (it loops
// forever) and confirm it compiled, mapped the framebuffer, seeded a grid, and
// drove both the device FILL clear and the per-pixel store path. Like the cube
// test, a timeout almost always lands inside the clear, so we assert on the clear
// plus at least one live cell drawn (a non-black pixel) rather than exact cells.
#[test]
fn life_demo_drives_framebuffer() {
    let user = compile_hosted(user::LIFE);
    let (vm, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 20_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "unhandled CPU exception; uart={uart:?}"
    );
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "life process was not spawned; uart={uart:?}"
    );
    assert!(
        uart.contains("life: running"),
        "life did not reach its render loop; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Continue),
        "life should run continuously, got {outcome:?}"
    );

    // The FILL clear must have run (opaque-black field) ...
    let px = vm.peek_framebuffer();
    let opaque_black = px
        .chunks_exact(4)
        .filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 255)
        .count();
    assert!(
        opaque_black > 10_000,
        "expected the clear to fill the framebuffer, got {opaque_black} black pixels"
    );
    assert!(
        vm.framebuffer_fill_count() >= 1,
        "life did not clear the framebuffer via the FILL register"
    );
    // ... and at least one live cell drawn (a green, non-black pixel).
    let live = px.chunks_exact(4).any(|p| p[1] > 0 && p[3] == 255);
    assert!(live, "life rendered no live cells onto the framebuffer");
}

// Headless framebuffer throughput bench (a measurement, not a pass/fail). Run with:
//   cargo test --release --test all -- --ignored cube_framebuffer_bench --nocapture
#[test]
#[ignore]
fn cube_framebuffer_bench() {
    let user = compile_hosted(user::CUBE);
    let steps: u64 = 40_000_000;

    let start = std::time::Instant::now();
    let (vm, _outcome, _uart) = boot_kernel(cached_kernel(), Some(&user), None, "", steps);
    let elapsed = start.elapsed().as_secs_f64();

    let frames = vm.framebuffer_fill_count();
    let cps = steps as f64 / elapsed;
    let fps = frames as f64 / elapsed;
    let cycles_per_frame = if frames > 0 {
        steps as f64 / frames as f64
    } else {
        f64::NAN
    };
    println!(
        "cube bench: {steps} steps in {elapsed:.3}s => {cps:.0} cycles/s; \
         {frames} frames; {cycles_per_frame:.0} cycles/frame; {fps:.1} cube fps"
    );
}

// --- Interactive shell (merged from kernel_shell.rs) ---

#[test]
fn kernel_shell_ls_cd_run_exit() {
    let shell = compile_hosted(user::SHELL);

    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("HELLO_FROM_CHILD".data)
    return 0
}
"#,
    );
    let exec_file = assembled_to_elf_file(&child);

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/home" },
        FsEntry::File {
            path: "/home/hello.elf",
            data: &exec_file,
        },
    ]);

    let session = "ls\ncd /home\nls\nrun hello.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        80_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("HLL shell ready"),
        "shell did not start; uart={uart:?}"
    );
    assert!(
        uart.contains("home"),
        "ls of root did not list home; uart={uart:?}"
    );
    assert!(
        uart.contains("hello.elf"),
        "ls of /home did not list the program; uart={uart:?}"
    );
    assert!(
        uart.contains("HELLO_FROM_CHILD"),
        "run did not execute the program; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt the VM cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Non-zero global initializers must survive the ELF/sys_exec load path too, not
// just the direct VM loader the run_hll tests cover.
#[test]
fn kernel_shell_exec_carries_global_initializers() {
    let shell = compile_hosted(user::SHELL);

    let child = compile_hosted(
        r#"
external print_int: (value: i64) -> i32
external console_writeln: (str: u8*)
g: i64 = 12345
arr: i64[3] = [100, 200, 300]
main: () -> i32 {
    console_writeln("SCALAR".data)
    print_int(g)
    console_writeln("ARRAY".data)
    print_int(@arr[0] + @arr[1] + @arr[2])
    return 0
}
"#,
    );
    let exec_file = assembled_to_elf_file(&child);

    let image = build_fs_image(&[FsEntry::File {
        path: "/g.elf",
        data: &exec_file,
    }]);

    let session = "run /g.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        80_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("12345"),
        "scalar global initializer did not survive exec; uart={uart:?}"
    );
    assert!(
        uart.contains("600"),
        "array global initializer did not survive exec; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// In-VM assembler: `as <src> <out>` assembles a small RV64I program from a source
// file into a runnable ELF, then `run` executes it. The test program exercises
// li/add/addi/bne/label/ecall (sum 1..=7 = 28, then exit(28)). The shell reaps the
// child via wait and prints "[exit 28]", proving every encoding -- the branch
// offset, arithmetic, and immediates -- is correct end to end (a wrong branch
// offset would fault or loop forever instead). The shell then survives to run the
// trailing `exit`, so the VM halts cleanly with 0.
#[test]
fn kernel_shell_assembles_and_runs_program() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    // Sum 1..=7 into a0 (= 28), then exit(a0). Pure subset: li/add/addi/bne/ecall.
    let source = b"\
; sum 1..7 then exit with the result
  li a0, 0
  li t0, 1
  li t1, 8
loop:
  add a0, a0, t0
  addi t0, t0, 1
  bne t0, t1, loop
  li a7, 93
  ecall
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/prog.s",
            data: source,
        },
    ]);

    let session = "as /prog.s /prog.elf\nrun /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the assembled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    assert!(
        !uart.contains("run: not an executable"),
        "sys_exec rejected the produced ELF; uart={uart:?}"
    );
    assert!(
        !uart.contains("run: cannot exec"),
        "sys_exec failed to load the produced ELF; uart={uart:?}"
    );
    // The assembled program computes 1+2+...+7 = 28 and exits with it; the shell
    // reaps it and reports the status, proving correct execution.
    assert!(
        uart.contains("[exit 28]"),
        "shell did not report the assembled program's exit code 28; uart={uart:?}"
    );
    // The shell survived the run and processed the trailing `exit`.
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// `as` emits a relocatable ET_REL object when the output ends in
// ".o". This program has a `.globl` export, an external `call` (undefined locally
// -> CALL_PLT relocation), and a `la` to a local `.data` symbol (always relocated).
// The smoke test proves the object writer runs in-VM without faulting and reports
// success; full correctness is covered once ld.hll links it (Phase C).
#[test]
fn kernel_as_emits_object() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    let source = b"\
.globl _start
.text
_start:
  la a0, msg
  call puts
  li a7, 93
  ecall
.data
msg:
  .asciz \"hi\"
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/prog.s",
            data: source,
        },
    ]);

    let session = "as /prog.s /prog.o\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the object writer faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.o"),
        "assembler did not report writing the object; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive object emission; outcome={outcome:?} uart={uart:?}"
    );
}

// Exercises the expanded assembler subset (PLAN 1.1): loads/stores with the
// `offset(reg)` syntax, the full branch set, shifts, register-immediate ALU ops,
// `slt`/`sltu`, and `lui`. The program computes 42 through every new instruction,
// self-checks each result with a branch to `fail`, and exits with a0. A single
// wrong encoding either faults or jumps to `fail` (exit 1), so `[exit 42]` proves
// the whole batch encodes correctly end to end.
#[test]
fn kernel_shell_assembles_expanded_subset() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    let source = b"\
; build 42 through the expanded instruction subset, self-checking as we go
  addi sp, sp, -16
  li t0, 5
  slli t1, t0, 2       ; 20
  srli t2, t1, 1       ; 10
  sub  t3, t1, t2      ; 10
  add  a0, t2, t3      ; 20
  sd a0, 0(sp)
  ld a1, 0(sp)         ; 20
  sw a1, 8(sp)
  lw a2, 8(sp)         ; 20
  add a0, a1, a2       ; 40
  li t0, 2
  add a0, a0, t0       ; 42
  andi a0, a0, 63      ; 42
  li t1, 42
  bge a0, t1, ok1
  j fail
ok1:
  blt a0, t1, fail
  li t2, 100
  bltu a0, t2, ok2
  j fail
ok2:
  bgeu a0, t2, fail
  slt t3, a0, t2       ; 42 < 100 -> 1
  beq t3, zero, fail
  li t0, 6
  li t1, 2
  sll t2, t0, t1       ; 24
  li t3, 24
  bne t2, t3, fail
  srl t2, t2, t1       ; 6
  bne t2, t0, fail
  li t0, -8
  srai t4, t0, 1       ; -4
  li t3, -4
  bne t4, t3, fail
  li t0, 12
  ori t2, t0, 3        ; 15
  li t3, 15
  bne t2, t3, fail
  xori t2, t2, 15      ; 0
  bne t2, zero, fail
  li t0, 5
  slti t2, t0, 10      ; 1
  li t3, 1
  bne t2, t3, fail
  sltu t2, t0, t3      ; 5 < 1 unsigned -> 0
  bne t2, zero, fail
  lui t2, 1            ; 0x1000 = 4096
  li t3, 4096
  bne t2, t3, fail
  li t0, 42
  sb t0, 4(sp)
  lb t1, 4(sp)         ; 42
  li t3, 42
  bne t1, t3, fail
  addi sp, sp, 16
  li a7, 93
  ecall
fail:
  addi sp, sp, 16
  li a0, 1
  li a7, 93
  ecall
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/prog.s",
            data: source,
        },
    ]);

    let session = "as /prog.s /prog.elf\nrun /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the assembled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "expanded-subset program did not exit 42 (a wrong encoding jumps to fail/1); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Exercises the M-extension + addiw (PLAN 1.1 final gap): the last mnemonics the
// HLL backend emits that the in-VM assembler was missing. Computes 42 through
// mul/div/divu/rem/remu and addiw, self-checking each with a branch to `fail`.
// A wrong encoding jumps to `fail` (exit 1) or faults, so `[exit 42]` proves the
// whole M-extension batch encodes byte-correctly end to end.
#[test]
fn kernel_shell_assembles_m_extension() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    let source = b"\
; build 42 through the M-extension and addiw, self-checking as we go
  li t0, 6
  li t1, 7
  mul a0, t0, t1       ; 42
  li t2, 42
  bne a0, t2, fail
  li t0, 85
  li t1, 2
  div t3, t0, t1       ; 42
  bne t3, t2, fail
  rem t4, t0, t1       ; 85 % 2 = 1
  li t5, 1
  bne t4, t5, fail
  li t0, 84
  li t1, 2
  divu t3, t0, t1      ; 42
  bne t3, t2, fail
  remu t4, t0, t1      ; 84 % 2 = 0
  bne t4, zero, fail
  li t0, 40
  addiw a0, t0, 2      ; 42 (exit code)
  bne a0, t2, fail
  li a7, 93
  ecall
fail:
  li a0, 1
  li a7, 93
  ecall
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/prog.s",
            data: source,
        },
    ]);

    let session = "as /prog.s /prog.elf\nrun /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the assembled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "M-extension program did not exit 42 (a wrong encoding jumps to fail/1); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// A non-ELF file or a missing name must be reported as an unknown command
// rather than failing opaquely inside sys_exec. Bare-name execution (PLAN 1.1)
// folds `run` into a single PATH-resolving path, so both report the same way.
#[test]
fn kernel_shell_run_rejects_non_executable() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::File {
        path: "/notes.txt",
        data: b"just some text, not a program\n",
    }]);

    let session = "run /notes.txt\nrun /missing.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        80_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("unknown command: /notes.txt"),
        "run did not reject a non-ELF file; uart={uart:?}"
    );
    assert!(
        uart.contains("unknown command: /missing.elf"),
        "run did not report a missing file; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Bare-name execution (PLAN 1.1): /utyping a program name (no `run`) resolves it
// through the PATH search (cwd, /bin, /home/demo) and runs it. Also covers a
// relative `./child.elf` path and `&` backgrounding via the shared launch path.
#[test]
fn kernel_shell_bare_name_runs_program() {
    let shell = compile_hosted(user::SHELL);

    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("BARE_NAME_RAN".data)
    return 0
}
"#,
    );
    let exec_file = assembled_to_elf_file(&child);

    // Install one demo under /home/demo (hit via PATH) and the same binary under
    // /home so a relative `./demo.elf` resolves from cwd.
    let image = build_fs_image(&[
        FsEntry::Dir { path: "/home" },
        FsEntry::Dir { path: "/home/demo" },
        FsEntry::File {
            path: "/home/demo/widget.elf",
            data: &exec_file,
        },
        FsEntry::File {
            path: "/home/child.elf",
            data: &exec_file,
        },
    ]);

    // `widget` (no run, no path) resolves via /home/demo; `./child.elf` resolves
    // relative to cwd /home; `widget &` exercises the backgrounded launch path.
    let session = "widget\ncd /home\n./child.elf\nwidget &\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    let runs = uart.matches("BARE_NAME_RAN").count();
    assert!(
        runs >= 3,
        "bare-name/relative/background launches did not all run (got {runs}); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// --- File editor (merged from kernel_editor.rs) ---

// Old contents of /notes.txt: distinctive filler, longer than the new contents,
// so a missing truncate would leave some of it behind.
const EDITOR_OLD_CONTENTS: &[u8] = b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";
const EDITOR_NEW_LINE_1: &str = "hello world";
const EDITOR_NEW_LINE_2: &str = "second line";

// Locate the inode for the named file in a built FS image and return its size
// field. Inode table starts at block 1 (offset 4096); each inode is 128 bytes
// with type at +0 (u16), size at +4 (u32), and name at +8.
fn inode_size_of(image: &[u8], name: &str) -> Option<u32> {
    const BLOCK_SIZE: usize = 4096;
    const INODE_SIZE: usize = 128;
    const INODE_COUNT: usize = 256;
    const IN_TYPE: usize = 0;
    const IN_SIZE: usize = 4;
    const IN_NAME: usize = 8;
    for idx in 0..INODE_COUNT {
        let base = BLOCK_SIZE + idx * INODE_SIZE;
        let ty = u16::from_le_bytes([image[base + IN_TYPE], image[base + IN_TYPE + 1]]);
        if ty != 1 {
            continue; // not a file
        }
        let name_bytes = &image[base + IN_NAME..base + IN_NAME + 32];
        let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
        if &name_bytes[..end] == name.as_bytes() {
            let size = u32::from_le_bytes([
                image[base + IN_SIZE],
                image[base + IN_SIZE + 1],
                image[base + IN_SIZE + 2],
                image[base + IN_SIZE + 3],
            ]);
            return Some(size);
        }
    }
    None
}

#[test]
fn editor_loads_clears_appends_writes_and_truncates() {
    let shell = compile_hosted(user::SHELL);
    let editor = compile_hosted(user::EDIT);
    let editor_exec = assembled_to_elf_file(&editor);

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/edit.elf",
            data: &editor_exec,
        },
        FsEntry::File {
            path: "/notes.txt",
            data: EDITOR_OLD_CONTENTS,
        },
    ]);

    // Open the editor on /notes.txt, clear the loaded filler, append two lines,
    // write (truncating), quit; then cat the file and exit.
    let session = format!(
        "edit /notes.txt\nc\na\n{EDITOR_NEW_LINE_1}\n{EDITOR_NEW_LINE_2}\n.\nw\nq\ncat /notes.txt\nexit\n"
    );
    let (vm, _outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        &session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("edit: p=print"),
        "editor did not start; uart={uart:?}"
    );
    assert!(
        uart.contains("edit: written"),
        "editor did not write the file; uart={uart:?}"
    );

    // The cat output (after "edit: written") must show exactly the new lines and
    // none of the old filler.
    let after_write = &uart[uart.find("edit: written").unwrap()..];
    assert!(
        after_write.contains(EDITOR_NEW_LINE_1) && after_write.contains(EDITOR_NEW_LINE_2),
        "cat did not show the new contents; uart={uart:?}"
    );
    assert!(
        !after_write.contains("ZZZ"),
        "old filler survived (truncate failed); uart={uart:?}"
    );

    // The on-disk inode size must equal the new contents exactly (truncate).
    let final_image = vm.peek_bytes_raw(FS_IMAGE_PA, image.len());
    let expected = (EDITOR_NEW_LINE_1.len() + 1 + EDITOR_NEW_LINE_2.len() + 1) as u32;
    let size = inode_size_of(&final_image, "notes.txt").expect("notes.txt inode");
    assert_eq!(
        size, expected,
        "inode size not truncated to new contents; got {size}, want {expected}"
    );
}

// The ed-style line editor: insert/goto/substitute/delete on individual lines,
// then write. Builds a three-line file, edits the middle line by substitution,
// inserts a line before it, deletes the first, and writes; the on-disk contents
// must reflect every operation.
#[test]
fn editor_line_operations_insert_substitute_delete() {
    let shell = compile_hosted(user::SHELL);
    let editor_exec = assembled_to_elf_file(&compile_hosted(user::EDIT));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/edit.elf",
            data: &editor_exec,
        },
        FsEntry::File {
            path: "/doc.txt",
            data: b"alpha\nbravo\ncharlie\n",
        },
    ]);

    // edit: go to line 2, substitute bravo->BRAVO, insert "inserted" before it,
    // go to line 1 (alpha) and delete it, then write and quit.
    //   start:  alpha / bravo / charlie
    //   2       -> current = bravo
    //   s/bravo/BRAVO/
    //   i ... inserted .   -> alpha / inserted / BRAVO / charlie
    //   1 d    -> inserted / BRAVO / charlie
    let session = "edit /doc.txt\n\
2\n\
s/bravo/BRAVO/\n\
i\ninserted\n.\n\
g 1\nd\n\
w\nq\n\
cat /doc.txt\nexit\n";
    let (_, _outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("edit: written"),
        "editor did not write; uart={uart:?}"
    );
    let after = &uart[uart.find("edit: written").unwrap()..];
    assert!(
        after.contains("inserted"),
        "insert did not take; uart={uart:?}"
    );
    assert!(
        after.contains("BRAVO"),
        "substitute did not take; uart={uart:?}"
    );
    assert!(after.contains("charlie"), "tail line lost; uart={uart:?}");
    assert!(
        !after.contains("alpha"),
        "delete of first line failed; uart={uart:?}"
    );
}

// A foreground interactive child that blocks waiting for input must not be
// declared dead by its parent's `waitpid`. When the editor sleeps on `sc_readchar`
// it becomes the scheduler's input_waiter and leaves the ready queue; `waitpid`
// must treat that as alive (scheduler_pid_alive, not just pid_in_queue) or it
// returns -1 and the shell abandons the still-running child. The normal harness
// front-loads all UART input so the child never blocks and never hit this; here we
// launch the editor, run with NO command available so it blocks (and gets timer-
// preempted), then inject the command -- mirroring the GUI, where `edit` printed
// `[exit -1]` at the first prompt before the user typed anything.
#[test]
fn editor_waits_while_idle_for_input() {
    let shell = compile_hosted(user::SHELL);
    let editor_exec = assembled_to_elf_file(&compile_hosted(user::EDIT));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/edit.elf",
            data: &editor_exec,
        },
        FsEntry::Dir { path: "/home" },
        FsEntry::Dir { path: "/home/src" },
        FsEntry::File {
            path: "/home/src/array.s",
            data: user::EXAMPLE_ARRAY_S.as_bytes(),
        },
    ]);

    // Launch only: cd + edit. No editor command yet, so the editor idles.
    let mut vm = setup_kernel_vm(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        "cd /home/src\nedit array.s\n",
    );

    // Let it boot, launch the editor, and idle (spinning on sc_readchar/sc_yield
    // and getting timer-preempted) for a good while with no input available.
    let r1 = vm.run(60_000_000);
    let uart1 = r1.uart_output.clone();
    assert!(
        uart1.contains("edit: p=print"),
        "editor did not start; uart={uart1:?}"
    );

    // Now the user "types" a command. Feed it and continue.
    for b in "p\nq\nexit\n".bytes() {
        vm.push_uart_rx(b);
    }
    let r2 = vm.run(60_000_000);
    let uart = format!("{uart1}{}", r2.uart_output);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("[exit -1]"),
        "editor faulted after idling/preemption; uart={uart:?}"
    );
    assert!(
        uart.contains("li t0, 3"),
        "editor did not print the file after input arrived; uart={uart:?}"
    );
}

// The bundled `array.s` example assembles and runs: it builds a five-element
// array on the stack with `sd`, sums it via `slli`-scaled `ld offset(reg)` in a
// `bge`-controlled loop, and exits with the total (42). Guards both the shipped
// example and the expanded assembler subset through the in-VM `as`.
#[test]
fn example_array_assembles_and_runs() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/array.s",
            data: user::EXAMPLE_ARRAY_S.as_bytes(),
        },
    ]);

    let session = "as /array.s /array.elf\nrun /array.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("as: wrote /array.elf"),
        "assembler failed; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "shell did not report the array sum (42) as the exit code; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// The expanded assembler handles a data section, `la`, and `call`/`ret`: a
// hand-written program loads a `.asciz` string with `la`, prints it via the write
// syscall, then computes its exit code through a called function. Proves the
// text/pad/data layout, PC-relative `la`/`call` encoding, and `jalr`-based return
// all round-trip through the in-VM `as`.
#[test]
fn kernel_shell_assembles_data_and_calls() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));

    let source = b"\
.text
.globl _start
_start:
  la a1, msg          ; address of the string in .data
  li a0, 1            ; fd = stdout (UART)
  li a2, 5            ; length of \"HELLO\"
  li a7, 64
  ecall               ; write(1, msg, 5)
  li a0, 40
  call add_two        ; a0 = 42 via a real call/ret
  li a7, 93
  ecall               ; exit(42)
add_two:
  addi a0, a0, 2
  ret
.data
msg:
  .asciz \"HELLO\"
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/prog.s",
            data: source,
        },
    ]);

    let session = "as /prog.s /prog.elf\nrun /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the assembled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("HELLO"),
        "`la` + .asciz did not print the data-section string; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "call/ret did not produce exit 42; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// A static ELF executable produced by the host toolchain loads and runs in the
// kernel. sys_exec validates the ELF magic, then parses the ELF64 header + PT_LOAD
// program headers, maps each segment at its p_vaddr, zeroes BSS, and jumps
// e_entry. The shell's executable check requires ELF magic, so `run /prog.elf`
// reaches the loader. Proves host-built ELF binaries run in our kernel unmodified.
#[test]
fn kernel_loads_and_runs_elf() {
    let shell = compile_hosted(user::SHELL);
    let prog = compile_hosted(
        "external console_writeln: (s: u8*)\n\
         main: () -> i32 {\n\
         \tconsole_writeln(\"elf ok\".data)\n\
         \treturn 7\n\
         }\n",
    );
    // Link at the same base the kernel maps user code to, so e_entry / p_vaddr
    // land in the user code region.
    let elf = prog.to_elf(USER_CODE_VA);

    let image = build_fs_image(&[FsEntry::File {
        path: "/prog.elf",
        data: &elf,
    }]);

    let session = "run /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the ELF program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("elf ok"),
        "ELF program did not print its marker; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 7]"),
        "ELF program did not exit 7 through the loader; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the ELF run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// PLAN 1.2 Phase A: pin the HLL-0 codegen target. Run the host-compiled hello.hll
// and the /bin/as-assembled hand-written hello.s side by side; both must print
// "HLL0" and exit 36, so source and frozen target agree.
#[test]
fn kernel_cc_target_roundtrips() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let host_elf = assembled_to_elf_file(&compile_hosted(user::CC_HELLO_HLL));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/hello.s",
            data: user::CC_HELLO_S.as_bytes(),
        },
        FsEntry::File {
            path: "/hello_host.elf",
            data: &host_elf,
        },
    ]);

    // Run the host-compiled source, then assemble + run the hand-written target.
    let session = "run /hello_host.elf\nas /hello.s /hello.elf\nrun /hello.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the HLL-0 target faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /hello.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    // Both the host-compiled source and the hand-written target must print the
    // marker and exit 36 -- so each appears twice.
    assert!(
        uart.matches("HLL0").count() >= 2,
        "source and target did not both print the marker; uart={uart:?}"
    );
    assert!(
        uart.matches("[exit 36]").count() >= 2,
        "source and target did not both exit 36; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the runs and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// PLAN 1.2 Phase B/C: the in-VM compiler `cc.hll` must itself host-compile (it is
// built into /bin/cc.elf). Fast guard that catches HLL syntax/semantic errors
// without booting the kernel.
#[test]
fn cc_host_compiles() {
    let out = compile_hosted(user::CC);
    assert!(
        !out.to_flat_binary().is_empty(),
        "cc.hll produced an empty binary"
    );
}

// PLAN 1.2 Phase D: the self-hosting headline. Inject a pure HLL-0 program, then
// `cc src.hll out.s && as out.s out.elf && run out.elf` -- all inside the VM. The
// program prints "HLL0\nY" and exits sum_to(8)=36, exercising the whole in-VM
// toolchain (compiler -> assembler -> loader). The source mirrors hello.hll but
// drops the inline-asm putc (cc emits putc as an intrinsic helper).
// PLAN 3 Phase C: the separate-compilation headline. Two hand-written .s files are
// assembled to ET_REL objects, linked into one ELF, and run -- all in-VM:
//   as a.s a.o && as b.s b.o && ld a.o b.o prog && prog
// a.s calls an external `get_val` (CALL_PLT relocation across modules); b.s defines
// it global and loads its own `.data` via `la` (PCREL_HI20 relocation + data merge).
// get_val returns 42, so a clean `[exit 42]` proves linking + every relocation kind.
#[test]
fn kernel_ld_links_objects_and_runs() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let ld_exec = assembled_to_elf_file(&compile_hosted(user::LD));

    let a_s = b"\
.globl _start
.text
_start:
  call get_val
  li a7, 93
  ecall
";
    let b_s = b"\
.globl get_val
.text
get_val:
  la a0, val
  lw a0, 0(a0)
  ret
.data
val:
  .word 42
";

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/bin/ld.elf",
            data: &ld_exec,
        },
        FsEntry::File {
            path: "/a.s",
            data: a_s,
        },
        FsEntry::File {
            path: "/b.s",
            data: b_s,
        },
    ]);

    let session = "as /a.s /a.o\nas /b.s /b.o\nld /a.o /b.o /prog\nrun /prog\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the linked program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /a.o") && uart.contains("as: wrote /b.o"),
        "assembler did not emit both objects; uart={uart:?}"
    );
    assert!(
        uart.contains("ld: wrote /prog"),
        "linker did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "linked program did not exit 42 (relocations wrong?); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// PLAN 3 Phase C, the separate-compilation payoff with a real stdlib: a client
// program (`hello_ld.s`) inlines no I/O -- it calls puts/putc/exit, all resolved
// at link time against a separately assembled `stdlib.s`. Exercises cross-module
// CALL_PLT relocations (the externals), an intra-module local call (puts -> putc,
// no relocation), and a PCREL_HI20 `la` into the client's own merged `.data`.
// Prints "hello from stdlib!" and exits 42.
#[test]
fn kernel_ld_links_stdlib_and_runs() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let ld_exec = assembled_to_elf_file(&compile_hosted(user::LD));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/bin/ld.elf",
            data: &ld_exec,
        },
        FsEntry::File {
            path: "/stdlib.s",
            data: user::EXAMPLE_STDLIB_S.as_bytes(),
        },
        FsEntry::File {
            path: "/hello_ld.s",
            data: user::EXAMPLE_HELLO_LD_S.as_bytes(),
        },
    ]);

    let session = "as /stdlib.s /stdlib.o\nas /hello_ld.s /hello_ld.o\n\
ld /stdlib.o /hello_ld.o /hello\nrun /hello\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the linked program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /stdlib.o") && uart.contains("as: wrote /hello_ld.o"),
        "assembler did not emit both objects; uart={uart:?}"
    );
    assert!(
        uart.contains("ld: wrote /hello"),
        "linker did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("hello from stdlib!"),
        "stdlib puts/putc output missing; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "linked program did not exit 42 (relocations wrong?); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

#[test]
fn kernel_cc_compiles_and_runs() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let cc_exec = assembled_to_elf_file(&compile_hosted(user::CC));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/bin/cc.elf",
            data: &cc_exec,
        },
        FsEntry::File {
            path: "/prog.hll",
            data: user::CC_DEMO_HLL.as_bytes(),
        },
    ]);

    let session = "cc /prog.hll /prog.s\nas /prog.s /prog.elf\nrun /prog.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the compiled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("cc: wrote /prog.s"),
        "compiler did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "assembler did not report success; uart={uart:?}"
    );
    assert!(
        uart.contains("HLL0"),
        "program marker missing; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 36]"),
        "compiled program did not exit 36; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive the run and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Repro of the interactive self-hosting session: cwd /home/src, relative paths to
// cc/as, and bare-name execution (`hello`, not `run /abs/path`).
#[test]
fn kernel_cc_interactive_relative_paths_and_barename() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let cc_exec = assembled_to_elf_file(&compile_hosted(user::CC));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/bin/cc.elf",
            data: &cc_exec,
        },
        FsEntry::Dir { path: "/home" },
        FsEntry::Dir { path: "/home/src" },
        FsEntry::File {
            path: "/home/src/hello.hll",
            data: user::CC_DEMO_HLL.as_bytes(),
        },
    ]);

    let session = "cd /home/src\ncc hello.hll hello.s\nas hello.s hello.elf\nhello\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the compiled program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("HLL0"),
        "program marker missing; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 36]"),
        "compiled program did not exit 36; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Regression: a stray trailing space on a `cc`/`as` output operand must not become
// part of the output filename (it did, so `as hello.s` could not find the file cc
// wrote as "hello.s "). The output is also named without an extension and run by
// bare name, exercising cwd resolution of an extensionless ELF.
#[test]
fn kernel_cc_tool_arg_trailing_space() {
    let shell = compile_hosted(user::SHELL);
    let as_exec = assembled_to_elf_file(&compile_hosted(user::AS));
    let cc_exec = assembled_to_elf_file(&compile_hosted(user::CC));

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/as.elf",
            data: &as_exec,
        },
        FsEntry::File {
            path: "/bin/cc.elf",
            data: &cc_exec,
        },
        FsEntry::Dir { path: "/home" },
        FsEntry::Dir { path: "/home/src" },
        FsEntry::File {
            path: "/home/src/hello.hll",
            data: user::CC_DEMO_HLL.as_bytes(),
        },
    ]);

    // Trailing spaces after the output operand on both tool invocations.
    let session = "cd /home/src\ncc hello.hll hello.s \nas hello.s hello \nhello\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("cannot open"),
        "tool could not open its input (trailing space leaked into a path); uart={uart:?}"
    );
    assert!(
        uart.contains("HLL0"),
        "program marker missing (round trip failed); uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 36]"),
        "compiled program did not exit 36; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not survive and exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Running a program must return to the prompt (not halt the VM) so the shell can
// run further commands. Runs the same child twice and then exits: the marker must
// appear twice and the VM halts only on the shell's own `exit`.
#[test]
fn kernel_shell_survives_run_and_continues() {
    let shell = compile_hosted(user::SHELL);
    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("RAN_CHILD".data)
    return 7
}
"#,
    );
    let exec_file = assembled_to_elf_file(&child);
    let image = build_fs_image(&[FsEntry::File {
        path: "/hi.elf",
        data: &exec_file,
    }]);

    let session = "run /hi.elf\nrun /hi.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        80_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The child ran both times: the shell stayed alive across the first run.
    assert_eq!(
        uart.matches("RAN_CHILD").count(),
        2,
        "child did not run twice (shell did not survive the first run); uart={uart:?}"
    );
    // Each run reaped the child and reported its non-zero exit code.
    assert_eq!(
        uart.matches("[exit 7]").count(),
        2,
        "shell did not report the child's exit code on both runs; uart={uart:?}"
    );
    // The VM halts only on the shell's own `exit`, not on a child's exit code.
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly after the runs; outcome={outcome:?} uart={uart:?}"
    );
}

// Ctrl-C (UART byte 0x03) while a foreground program runs must tear it down and
// return to the prompt. The child loops forever, so without an interrupt the shell
// would block in wait and never reach the trailing `exit`. The 0x03 between the
// `run` line and `exit` makes the kernel kill the child; the shell prints "^C",
// reads `exit`, and the VM halts cleanly -- which only happens if Ctrl-C worked.
#[test]
fn kernel_shell_ctrl_c_interrupts_foreground() {
    let shell = compile_hosted(user::SHELL);
    let child = compile_hosted(
        r#"
main: () -> i32 {
    x: i64 = 0
    while 1 == 1 {
        x = x + 1
    }
    return 0
}
"#,
    );
    let exec_file = assembled_to_elf_file(&child);
    let image = build_fs_image(&[FsEntry::File {
        path: "/loop.elf",
        data: &exec_file,
    }]);

    // 0x03 (Ctrl-C) is delivered right after the run command's newline.
    let session = "run /loop.elf\n\x03exit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        80_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("^C"),
        "shell did not acknowledge Ctrl-C; uart={uart:?}"
    );
    // The shell survived the interrupt and processed the trailing `exit`. If Ctrl-C
    // had not worked, the shell would still be blocked on the looping child.
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "Ctrl-C did not return control to the shell; outcome={outcome:?} uart={uart:?}"
    );
}

// Two concurrent background jobs must each occupy a distinct job-table slot.
#[test]
fn kernel_shell_two_background_jobs_keep_distinct_slots() {
    let shell = compile_hosted(user::SHELL);
    let spin = r#"
external sc_exit: (code: i64)
main: () -> i32 {
    i: u64 = 0
    while 1 == 1 { i = i + 1 }
    sc_exit(0)
    return 0
}
"#;
    let a = compile_hosted(spin);
    let b = compile_hosted(spin);
    let a_exec = assembled_to_elf_file(&a);
    let b_exec = assembled_to_elf_file(&b);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/a.elf",
            data: &a_exec,
        },
        FsEntry::File {
            path: "/b.elf",
            data: &b_exec,
        },
    ]);

    let session = "run /a.elf &\njobs\nrun /b.elf &\njobs\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        60_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // No slot was corrupted: no job is ever reaped with a bogus/garbage pid, and
    // neither real pid (2, 3) is ever reported "done" -- both spin forever.
    assert!(
        !uart.contains("] done   pid "),
        "a live job was wrongly reaped; uart={uart:?}"
    );
    assert!(
        !uart.contains("no background jobs"),
        "a background job vanished from the table; uart={uart:?}"
    );
    // Both jobs must appear, each on its own job id, in the final listing.
    assert!(uart.contains("[1] pid 2"), "job 1 missing; uart={uart:?}");
    assert!(uart.contains("[2] pid 3"), "job 2 missing; uart={uart:?}");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Signals + `kill` (PLAN v3 sec.3): background a forever-spinner, `kill <pid>` it
// by pid, and confirm the shell survives, the job leaves the table, and a later
// `jobs` reports nothing. The spinner never exits on its own, so its disappearance
// proves the SIGKILL teardown (scheduler_kill_pid via syscall 129) actually ran.
#[test]
fn kernel_kill_background_job() {
    let shell = compile_hosted(user::SHELL);
    let spin = compile_hosted(
        r#"
external sc_exit: (code: i64)
main: () -> i32 {
    i: u64 = 0
    while 1 == 1 { i = i + 1 }
    sc_exit(0)
    return 0
}
"#,
    );
    let spin_exec = assembled_to_elf_file(&spin);
    let image = build_fs_image(&[FsEntry::File {
        path: "/spin.elf",
        data: &spin_exec,
    }]);

    let session = "run /spin.elf &\nkill 2\njobs\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        60_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The job was announced as pid 2 ...
    assert!(
        uart.contains("[1] pid 2"),
        "background job not announced; uart={uart:?}"
    );
    // ... reaped after the kill ...
    assert!(
        uart.contains("] done   pid 2"),
        "killed job was not reaped from the table; uart={uart:?}"
    );
    // ... and the later `jobs` listing is empty.
    assert!(
        uart.contains("no background jobs"),
        "job still listed after kill; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly after kill; outcome={outcome:?} uart={uart:?}"
    );
}

// `kill %<job>` maps a job id through the shell's job table to the same SIGKILL
// path as `kill <pid>`. Same scenario as kernel_kill_background_job, addressed by
// job id instead of pid.
#[test]
fn kernel_kill_by_jobid() {
    let shell = compile_hosted(user::SHELL);
    let spin = compile_hosted(
        r#"
external sc_exit: (code: i64)
main: () -> i32 {
    i: u64 = 0
    while 1 == 1 { i = i + 1 }
    sc_exit(0)
    return 0
}
"#,
    );
    let spin_exec = assembled_to_elf_file(&spin);
    let image = build_fs_image(&[FsEntry::File {
        path: "/spin.elf",
        data: &spin_exec,
    }]);

    let session = "run /spin.elf &\nkill %1\njobs\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        60_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("[1] pid 2"),
        "background job not announced; uart={uart:?}"
    );
    assert!(
        uart.contains("] done   pid 2"),
        "killed job was not reaped from the table; uart={uart:?}"
    );
    assert!(
        uart.contains("no background jobs"),
        "job still listed after kill %1; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly after kill; outcome={outcome:?} uart={uart:?}"
    );
}

// I/O redirection (PLAN v3 1.2, first slice): `prog > file` binds the child's
// stdout (fd 1) to an FS file via the per-PCB fd table, so its console output is
// captured instead of hitting the UART. We prove the capture by ordering: the
// marker must appear only AFTER `cat` reads the file back -- if redirection had
// failed, the program would have printed it straight to the UART before `cat`.
#[test]
fn kernel_redirect_stdout_to_file() {
    let shell = compile_hosted(user::SHELL);
    let printer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("REDIRECT_OK".data)
    sc_exit(0)
    return 0
}
"#,
    );
    let printer_exec = assembled_to_elf_file(&printer);
    let image = build_fs_image(&[FsEntry::File {
        path: "/printer.elf",
        data: &printer_exec,
    }]);

    let session = "run /printer.elf > /out.txt\ncat /out.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    let mark = uart
        .find("REDIRECT_OK")
        .expect("marker never reached the UART");
    let cat = uart.find("cat /out.txt").expect("cat command not echoed");
    assert!(
        mark > cat,
        "marker appeared before `cat`, so stdout was NOT redirected; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Append redirection (`>>`): two runs appending to the same file, then `cat`
// shows both lines -- proving `>>` seeks to end instead of truncating.
#[test]
fn kernel_redirect_append() {
    let shell = compile_hosted(user::SHELL);
    let printer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("LINE".data)
    sc_exit(0)
    return 0
}
"#,
    );
    let printer_exec = assembled_to_elf_file(&printer);
    let image = build_fs_image(&[FsEntry::File {
        path: "/p.elf",
        data: &printer_exec,
    }]);

    let session = "run /p.elf > /log.txt\nrun /p.elf >> /log.txt\ncat /log.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        160_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // After cat, both appended lines are present (the `cat` echo is the only LINE
    // before this point, so a >=2 count past it proves the file holds two lines).
    let cat = uart.find("cat /log.txt").expect("cat not echoed");
    let after = &uart[cat..];
    let lines = after.matches("LINE").count();
    assert!(
        lines >= 2,
        "append did not accumulate two lines; found {lines} in {after:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Input redirection (`prog < file`): the child's stdin (fd 0 / sc_readchar) reads
// from an FS file and sees EOF (-1) at its end. The echo program copies stdin to
// stdout (the UART here), so the file's contents appear on the console.
#[test]
fn kernel_redirect_stdin_from_file() {
    let shell = compile_hosted(user::SHELL);
    let echo = compile_hosted(
        r#"
external sc_readchar: () -> i64
external console_putchar: (c: i32)
external sc_exit: (code: i64)
main: () -> i32 {
    while 1 == 1 {
        c: i64 = sc_readchar()
        if c < 0 {
            break
        }
        console_putchar(c as i32)
    }
    sc_exit(0)
    return 0
}
"#,
    );
    let echo_exec = assembled_to_elf_file(&echo);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/echo.elf",
            data: &echo_exec,
        },
        FsEntry::File {
            path: "/in.txt",
            data: b"HELLO_STDIN",
        },
    ]);

    let session = "run /echo.elf < /in.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("HELLO_STDIN"),
        "stdin redirection did not feed the file to the program; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Command polish (PLAN v3 1.3): `ls <dir>` lists a directory other than cwd.
// Previously only a bare `ls` (current directory) was recognised; `ls /sub` fell
// through to bare-name execution and failed with "unknown command".
#[test]
fn kernel_ls_dir_lists_target() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[
        FsEntry::Dir { path: "/sub" },
        FsEntry::File {
            path: "/sub/alpha.txt",
            data: b"A",
        },
        FsEntry::File {
            path: "/sub/beta.txt",
            data: b"B",
        },
    ]);

    let session = "ls /sub\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("alpha.txt") && uart.contains("beta.txt"),
        "ls <dir> did not list the target directory; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Command polish (PLAN v3 1.3): builtin output redirection. `echo` is a builtin
// running inside the shell (not an exec'd child), so its `>` redirect is served by
// the shell-side output sink, not the kernel per-PCB fd table. We prove the text
// landed in the file (and not the console) by reading it back with `cat`.
#[test]
fn kernel_echo_redirect_to_file() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::Dir { path: "/tmp" }]);
    let session = "echo MARKER_ECHO > /e.txt\ncat /e.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // After the `cat /e.txt` echo, the marker can only come from the file: if the
    // sink had failed, echo would have hit the console and /e.txt stayed empty.
    let cat = uart.find("cat /e.txt").expect("cat command not echoed");
    let after = &uart[cat..];
    assert!(
        after.contains("MARKER_ECHO"),
        "echo output was not captured into the file; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Command polish (PLAN v3 1.3): a builtin `cat <file> >> <dst>` appends through the
// shell sink (seeking to end via sc_lseek). Two appends of the same source file
// then a read-back must show its contents twice.
#[test]
fn kernel_cat_append_builtin() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::File {
        path: "/src.txt",
        data: b"DATA\n",
    }]);

    let session = "cat /src.txt >> /dst.txt\ncat /src.txt >> /dst.txt\ncat /dst.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        160_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The append commands echo "src.txt", never "DATA"; so every DATA after the
    // final read-back is file content. Two appends => two lines.
    let cat = uart.find("cat /dst.txt").expect("read-back not echoed");
    let after = &uart[cat..];
    let lines = after.matches("DATA").count();
    assert!(
        lines >= 2,
        "builtin append did not accumulate two copies; found {lines} in {after:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Pipes (PLAN v3 1.2): `producer | filter`. The producer writes to stdout, which
// the shell binds to a temp file; the filter's stdin reads that file back. The
// producer's payload therefore never hits the UART directly -- only the filter,
// the pipeline's last stage, prints it -- so seeing the payload on the console
// proves it travelled through the pipe.
#[test]
fn kernel_pipe_two_stage() {
    let shell = compile_hosted(user::SHELL);
    let producer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("PIPE_PAYLOAD".data)
    sc_exit(0)
    return 0
}
"#,
    );
    // Copy stdin to stdout until EOF (-1): a transparent filter.
    let filter = compile_hosted(
        r#"
external sc_readchar: () -> i64
external console_putchar: (c: i32)
external sc_exit: (code: i64)
main: () -> i32 {
    while 1 == 1 {
        c: i64 = sc_readchar()
        if c < 0 {
            break
        }
        console_putchar(c as i32)
    }
    sc_exit(0)
    return 0
}
"#,
    );
    let producer_exec = assembled_to_elf_file(&producer);
    let filter_exec = assembled_to_elf_file(&filter);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/producer.elf",
            data: &producer_exec,
        },
        FsEntry::File {
            path: "/filter.elf",
            data: &filter_exec,
        },
    ]);

    let session = "/producer.elf | /filter.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        160_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The command echo contains "producer"/"filter", never the payload, so any
    // PIPE_PAYLOAD on the UART came out of the filter, i.e. through the pipe.
    let cmd = uart.find("filter.elf").expect("pipeline not echoed");
    let after = &uart[cmd..];
    assert!(
        after.contains("PIPE_PAYLOAD"),
        "payload did not travel through the pipe; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Pipes combined with a trailing redirect: `producer | filter > out.txt`. The
// filter's stdout is captured to a file instead of the console, proven by reading
// it back with `cat` -- the payload appears only after the `cat`, never before.
#[test]
fn kernel_pipe_to_file() {
    let shell = compile_hosted(user::SHELL);
    let producer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("VIA_PIPE".data)
    sc_exit(0)
    return 0
}
"#,
    );
    let filter = compile_hosted(
        r#"
external sc_readchar: () -> i64
external console_putchar: (c: i32)
external sc_exit: (code: i64)
main: () -> i32 {
    while 1 == 1 {
        c: i64 = sc_readchar()
        if c < 0 {
            break
        }
        console_putchar(c as i32)
    }
    sc_exit(0)
    return 0
}
"#,
    );
    let producer_exec = assembled_to_elf_file(&producer);
    let filter_exec = assembled_to_elf_file(&filter);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/producer.elf",
            data: &producer_exec,
        },
        FsEntry::File {
            path: "/filter.elf",
            data: &filter_exec,
        },
    ]);

    let session = "/producer.elf | /filter.elf > /piped.txt\ncat /piped.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    let cat = uart.find("cat /piped.txt").expect("cat not echoed");
    let before = &uart[..cat];
    let after = &uart[cat..];
    assert!(
        !before.contains("VIA_PIPE"),
        "pipeline output leaked to console instead of the file; uart={uart:?}"
    );
    assert!(
        after.contains("VIA_PIPE"),
        "piped output was not captured into the file; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Pipes with a builtin source: `cat /src.txt | filter`. The builtin `cat` runs in
// the shell with its output bound to the pipe temp file (the sink), and the
// external filter reads it back -- exercising the builtin-producer branch of
// sh_run_stage.
#[test]
fn kernel_pipe_builtin_source() {
    let shell = compile_hosted(user::SHELL);
    let filter = compile_hosted(
        r#"
external sc_readchar: () -> i64
external console_putchar: (c: i32)
external sc_exit: (code: i64)
main: () -> i32 {
    while 1 == 1 {
        c: i64 = sc_readchar()
        if c < 0 {
            break
        }
        console_putchar(c as i32)
    }
    sc_exit(0)
    return 0
}
"#,
    );
    let filter_exec = assembled_to_elf_file(&filter);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/filter.elf",
            data: &filter_exec,
        },
        FsEntry::File {
            path: "/src.txt",
            data: b"CAT_PIPED\n",
        },
    ]);

    let session = "cat /src.txt | /filter.elf\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        160_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    let cmd = uart.find("filter.elf").expect("pipeline not echoed");
    let after = &uart[cmd..];
    assert!(
        after.contains("CAT_PIPED"),
        "builtin cat output did not travel through the pipe; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// `cat` with no file argument is a stdin consumer: it copies its input source to
// the sink. As a pipe consumer, `echo TEXT | cat` must print TEXT (the builtin
// `cat` reads the pipe temp file the shell wired to its stdin).
#[test]
fn kernel_pipe_echo_into_cat() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::Dir { path: "/tmp" }]);

    let session = "echo CAT_STDIN | cat\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The command echo prints "CAT_STDIN" once (the typed line). If the pipe worked,
    // cat prints it a second time, so the count is >= 2.
    let n = uart.matches("CAT_STDIN").count();
    assert!(
        n >= 2,
        "echo did not pipe into cat; saw CAT_STDIN {n} time(s); uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// `ls | cat` must list the real directory contents only. The pipe temp file the
// shell creates is named with a leading dot, and `ls` hides dotfiles, so it never
// appears in the listing (regression for a leaked `.pipe0` entry).
#[test]
fn kernel_pipe_ls_hides_temp_file() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[
        FsEntry::File {
            path: "/real.txt",
            data: b"x",
        },
        FsEntry::Dir { path: "/sub" },
    ]);

    let session = "ls | cat\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    let cmd = uart.find("ls | cat").expect("pipeline not echoed");
    let after = &uart[cmd..];
    assert!(
        after.contains("real.txt"),
        "real file missing from ls; uart={uart:?}"
    );
    assert!(
        !after.contains(".pipe"),
        "pipe temp leaked into ls output; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// `cat < file` (no file operand, stdin redirected): cat reads the redirected file
// and prints it. Proves the builtin input source also serves explicit `<`.
#[test]
fn kernel_cat_stdin_redirect() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::File {
        path: "/in.txt",
        data: b"FROM_STDIN\n",
    }]);

    let session = "cat < /in.txt\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // "FROM_STDIN" is only in the file, never in the typed command, so any sighting
    // is cat printing the redirected input.
    assert!(
        uart.contains("FROM_STDIN"),
        "cat did not read redirected stdin; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Console interrupt takeover + `jobs` while a background job runs (PLAN 1.1/1.2).
// Unlike kernel_shell_background_job_runs_and_fg_reaps_it (which front-loads the
// whole session into the UART, so the shell never actually blocks), this drives
// input the way the GUI does: input is injected MID-RUN, only after the shell has
// reached its blocking read. That exercises the path that was previously untested
// -- the shell sleeping in readchar (UART RX interrupt armed), a background job
// running on the CPU in the meantime, and an incoming byte raising the RX
// interrupt to wake the shell ("the console takes over") so it can list the job.
// Regression guard for "jobs shows nothing while a background job is running".
#[test]
fn kernel_shell_interrupt_wake_lists_running_background_job() {
    let shell = compile_hosted(user::SHELL);
    // A background job that runs forever and never reads input, so it stays the
    // sole runnable process while the shell is blocked waiting for a keystroke.
    let spinner = compile_hosted(
        r#"
external sc_exit: (code: i64)
main: () -> i32 {
    i: u64 = 0
    while 1 == 1 {
        i = i + 1
    }
    sc_exit(0)
    return 0
}
"#,
    );
    let spinner_exec = assembled_to_elf_file(&spinner);
    let image = build_fs_image(&[FsEntry::File {
        path: "/spin.elf",
        data: &spinner_exec,
    }]);

    // No front-loaded input: the shell must block in readchar between commands.
    let mut vm = setup_kernel_vm(cached_kernel(), Some(&shell), Some(&image), "");
    let mut out = String::new();

    // Helper: inject a line, then step until `marker` shows in the UART output.
    // Stepping past the blocking read proves the RX interrupt woke the shell.
    let feed = |vm: &mut VirtualMachine, out: &mut String, line: &[u8], marker: &str| {
        for b in line {
            vm.push_uart_rx(*b);
        }
        for _ in 0..30_000_000u64 {
            let _ = vm.step();
            let bytes = vm.uart_output();
            out.push_str(&String::from_utf8_lossy(&bytes));
            if out.contains(marker) {
                return true;
            }
        }
        false
    };

    // Boot to the prompt (the shell is now blocked in its first readchar).
    for _ in 0..5_000_000u64 {
        let _ = vm.step();
        let bytes = vm.uart_output();
        out.push_str(&String::from_utf8_lossy(&bytes));
        if out.contains("shell ready") {
            break;
        }
    }
    assert!(
        out.contains("shell ready"),
        "shell never booted; out={out:?}"
    );

    // Background a forever-spinner; the announcement proves exec returned to the
    // prompt instead of blocking, and the shell then sleeps in readchar again.
    assert!(
        feed(&mut vm, &mut out, b"run /spin.elf &\n", "] pid "),
        "background job was not announced; out={out:?}"
    );

    // While the spinner runs, type `jobs`. The byte must raise the RX interrupt,
    // wake the blocked shell, and list the job as running -- not "no background
    // jobs" (which would mean the live job had been wrongly reaped from the table).
    assert!(
        feed(&mut vm, &mut out, b"jobs\n", "running"),
        "interactive `jobs` did not list the running background job; out={out:?}"
    );
    assert!(
        !out.contains("no background jobs"),
        "shell reported no jobs while one was running; out={out:?}"
    );
}

// Background jobs (PLAN 1.2): `run <file> &` execs without waiting and returns to
// the prompt; `jobs` lists the job; `fg <n>` waits on it and reaps the exact exit
// code via the pid-targeted waitpid syscall (261). The background child spins
// before printing, so its announcement "[1] pid N" (printed the instant exec
// returns) must appear before the child's "SLOW_DONE" -- proof the shell returned
// to the prompt and kept running concurrently with the child rather than blocking.
#[test]
fn kernel_shell_background_job_runs_and_fg_reaps_it() {
    let shell = compile_hosted(user::SHELL);

    let slow = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
spin: () {
    i: u64 = 0
    while i < 2000000 {
        i = i + 1
    }
}
main: () -> i32 {
    spin()
    console_writeln("SLOW_DONE".data)
    sc_exit(7)
    return 0
}
"#,
    );
    let slow_exec = assembled_to_elf_file(&slow);

    let image = build_fs_image(&[FsEntry::File {
        path: "/slow.elf",
        data: &slow_exec,
    }]);

    let session = "run /slow.elf &\njobs\nfg 1\nexit\n";
    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    // The job was backgrounded (announced with a job id and pid).
    assert!(
        uart.contains("] pid "),
        "background job was not announced; uart={uart:?}"
    );
    assert!(
        uart.contains("SLOW_DONE"),
        "background child did not run; uart={uart:?}"
    );
    // fg reaped the exact exit code through the pid-targeted wait.
    assert!(
        uart.contains("[exit 7]"),
        "fg did not reap the child's exit code (7); uart={uart:?}"
    );

    // Concurrency: the prompt returned and printed the job announcement before the
    // child finished its spin (the shell did not block on the background job).
    let announce = uart.find("] pid ").expect("job announcement");
    let slow_done = uart.find("SLOW_DONE").expect("slow done");
    assert!(
        announce < slow_done,
        "shell blocked on the background job instead of returning to the prompt; uart={uart:?}"
    );
    // fg waited for completion: the exit report follows the child's own output.
    let exit_at = uart.find("[exit 7]").expect("exit report");
    assert!(
        slow_done < exit_at,
        "fg reported exit before the child finished; uart={uart:?}"
    );

    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly after the job; outcome={outcome:?} uart={uart:?}"
    );
}

// --- fork / wait (merged from kernel_fork.rs) ---

#[test]
fn fork_child_runs_and_parent_reaps_exit_code() {
    // pid 1: fork. The child (a0==0) writes a marker and exits with 42; the
    // parent waits and confirms the reaped code is exactly 42.
    let parent = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_fork: () -> i64
external sc_wait: () -> i64
external sc_exit: (code: i64)

main: () -> i32 {
    pid: i64 = sc_fork()
    if pid == 0 {
        console_writeln("CHILD_RAN".data)
        sc_exit(42)
    }

    console_writeln("PARENT_FORKED".data)
    code: i64 = sc_wait()
    if code == 42 {
        console_writeln("PARENT_REAPED_42".data)
    } else {
        console_writeln("PARENT_REAPED_WRONG".data)
    }
    sc_exit(0)
    return 0
}
"#,
    );

    // fork needs no child binary, but the kernel still expects a valid (empty)
    // FS image at the agreed physical address.
    let image = build_fs_image(&[]);
    let (_, _outcome, uart) =
        boot_kernel(cached_kernel(), Some(&parent), Some(&image), "", 80_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("PARENT_FORKED"),
        "parent did not return from fork; uart={uart:?}"
    );
    assert!(
        uart.contains("CHILD_RAN"),
        "child did not run; uart={uart:?}"
    );
    assert!(
        uart.contains("PARENT_REAPED_42"),
        "parent did not reap the child's exit code; uart={uart:?}"
    );
    assert!(
        !uart.contains("PARENT_REAPED_WRONG"),
        "parent reaped the wrong exit code; uart={uart:?}"
    );

    // Ordering: the child must have run and exited before the parent's wait
    // returned (the parent busy-yields until a zombie child appears).
    let child_at = uart.find("CHILD_RAN").unwrap();
    let reaped_at = uart.find("PARENT_REAPED_42").unwrap();
    assert!(
        child_at < reaped_at,
        "parent reaped before the child ran; uart={uart:?}"
    );
}

// Preemptive scheduling: the timer interrupt must time-slice two compute-bound
// processes, not just switch on voluntary syscalls. pid 1 forks; both parent and
// child emit a marker char after a tight compute-only spin (no syscalls between
// marks), so ONLY the timer interrupt can switch from one to the other mid-loop.
// Interleaved A/B output proves preemption; a fully sequential AAAA...BBBB run
// would mean the timer never preempts (each process runs to completion, then
// yields at wait). This is the foundation for background jobs (PLAN 1.2).
#[test]
fn scheduler_timer_preempts_compute_bound_processes() {
    let parent = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_fork:  () -> i64
external sc_wait:  () -> i64
external sc_exit:  (code: i64)
external sc_write: (fd: i64, buf: u8*, len: u64) -> i64

; A compute-only delay long enough that the 1e6-cycle timer fires within it.
spin: () {
    i: u64 = 0
    while i < 600000 {
        i = i + 1
    }
}

main: () -> i32 {
    console_writeln("SENTINEL_START".data)
    pid: i64 = sc_fork()
    if pid == 0 {
        n: u64 = 0
        while n < 8 {
            spin()
            sc_write(1, "B".data, 1)
            n = n + 1
        }
        sc_exit(0)
    }
    m: u64 = 0
    while m < 8 {
        spin()
        sc_write(1, "A".data, 1)
        m = m + 1
    }
    sc_wait()
    console_writeln("SENTINEL_END".data)
    sc_exit(0)
    return 0
}
"#,
    );

    let image = build_fs_image(&[]);
    let (_, _outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&parent),
        Some(&image),
        "",
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");

    // Everything between the two sentinels is emitted only by the two user
    // processes (the kernel logs nothing to the UART once pid 1 is running).
    let start =
        uart.find("SENTINEL_START").expect("start sentinel missing") + "SENTINEL_START".len();
    let end = uart.find("SENTINEL_END").expect("end sentinel missing");
    assert!(start <= end, "sentinels out of order; uart={uart:?}");
    let marks: String = uart[start..end]
        .chars()
        .filter(|c| *c == 'A' || *c == 'B')
        .collect();

    let a_count = marks.chars().filter(|c| *c == 'A').count();
    let b_count = marks.chars().filter(|c| *c == 'B').count();
    assert!(
        a_count > 0 && b_count > 0,
        "both processes must emit marks; marks={marks:?}"
    );

    // Count A<->B transitions. A fully sequential run (no preemption) has exactly
    // one transition; genuine time-slicing produces several.
    let transitions = marks.as_bytes().windows(2).filter(|w| w[0] != w[1]).count();
    assert!(
        transitions >= 3,
        "timer did not preempt: output is nearly sequential ({transitions} transitions); marks={marks:?}"
    );
}

// --- Per-process address-space isolation (merged from kernel_isolation.rs) ---

// A fixed user VA inside the lowest stack page (mapped R+W+U in every process,
// well below the live stack frames of a shallow program).
const SHARED_VA: u64 = 0x7FFF_C000;

#[test]
fn per_process_address_spaces_are_isolated() {
    // Parent (pid 1): write a marker to SHARED_VA, run the child, then confirm
    // the marker is intact after the child has written its own value there.
    let parent_src = format!(
        r#"
external console_writeln: (str: u8*)
external sc_exec: (path: u8*) -> i64
external sc_pidalive: (pid: u64) -> i64
external sc_yield: ()
external sc_exit: (code: i64)

main: () -> i32 {{
    p: u8* = {shared} as u8*
    @p = 0xAA as u8
    console_writeln("PARENT_WROTE".data)

    pid: i64 = sc_exec("/child.elf".data)
    if pid < 0 {{
        console_writeln("PARENT_EXEC_FAIL".data)
        sc_exit(1)
    }}
    while sc_pidalive(pid as u64) == 1 {{
        sc_yield()
    }}

    v: u8 = @p
    if v == 0xAA as u8 {{
        console_writeln("PARENT_FINAL_AA".data)
    }} else {{
        console_writeln("PARENT_FINAL_CORRUPT".data)
    }}
    sc_exit(0)
    return 0
}}
"#,
        shared = SHARED_VA
    );

    let child_src = format!(
        r#"
external console_writeln: (str: u8*)

main: () -> i32 {{
    p: u8* = {shared} as u8*
    @p = 0xBB as u8
    v: u8 = @p
    if v == 0xBB as u8 {{
        console_writeln("CHILD_FINAL_BB".data)
    }} else {{
        console_writeln("CHILD_FINAL_BAD".data)
    }}
    return 0
}}
"#,
        shared = SHARED_VA
    );

    let parent = compile_hosted(&parent_src);
    let child = compile_hosted(&child_src);
    let child_exec = assembled_to_elf_file(&child);

    let image = build_fs_image(&[FsEntry::File {
        path: "/child.elf",
        data: &child_exec,
    }]);

    let (_, _outcome, uart) =
        boot_kernel(cached_kernel(), Some(&parent), Some(&image), "", 80_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("PARENT_WROTE"),
        "parent did not start; uart={uart:?}"
    );
    assert!(
        uart.contains("CHILD_FINAL_BB"),
        "child did not run/write its own VA; uart={uart:?}"
    );
    assert!(
        uart.contains("PARENT_FINAL_AA"),
        "parent's VA was clobbered by the child (no isolation); uart={uart:?}"
    );
    assert!(
        !uart.contains("PARENT_FINAL_CORRUPT"),
        "parent saw the child's write (no isolation); uart={uart:?}"
    );
}

// --- Filesystem (merged from kernel_fs.rs) ---

// User program that exercises the full FS API and prints FS_ALL_PASS on success.
const FS_EXERCISER: &str = r#"
external sc_open:   (path: u8*, flags: u64) -> i64
external sc_close:  (fd: i64)
external sc_read:   (fd: i64, buf: u8*, offset: u64, len: u64) -> i64
external sc_write:  (fd: i64, buf: u8*, len: u64) -> i64
external sc_mkdir:  (path: u8*) -> i64
external sc_rename: (old_path: u8*, new_path: u8*) -> i64

fail: (label: u8*) -> i32 {
    console_write("FS_FAIL: ".data)
    console_writeln(label)
    return 1
}

bytes_eq: (a: u8*, b: u8*, n: u64) -> i64 {
    i: u64 = 0
    while i < n {
        if @(a + i) != @(b + i) {
            return 0
        }
        i = i + 1
    }
    return 1
}

main: () -> i32 {
    buf: u8[64]

    ; 1. Read a pre-populated file.
    fd: i64 = sc_open("/hello.txt".data, 0)
    if fd < 2 { return fail("open hello".data) }
    n: i64 = sc_read(fd, buf[0], 0, 13)
    if n != 13 { return fail("read hello count".data) }
    if bytes_eq(buf[0], "hello from fs".data, 13) == 0 { return fail("read hello data".data) }
    sc_close(fd)

    ; 2. Create a directory.
    if sc_mkdir("/out".data) < 0 { return fail("mkdir out".data) }

    ; 3. Create a file in it and write to it.
    wfd: i64 = sc_open("/out/result.txt".data, 2)
    if wfd < 2 { return fail("create result".data) }
    w: i64 = sc_write(wfd, "data123".data, 7)
    if w != 7 { return fail("write count".data) }
    sc_close(wfd)

    ; 4. Re-open and read back the written contents.
    rfd: i64 = sc_open("/out/result.txt".data, 0)
    if rfd < 2 { return fail("reopen result".data) }
    rn: i64 = sc_read(rfd, buf[0], 0, 7)
    if rn != 7 { return fail("readback count".data) }
    if bytes_eq(buf[0], "data123".data, 7) == 0 { return fail("readback data".data) }
    sc_close(rfd)

    ; 5. Rename within the directory; old name must disappear.
    if sc_rename("/out/result.txt".data, "/out/final.txt".data) != 0 {
        return fail("rename".data)
    }
    ffd: i64 = sc_open("/out/final.txt".data, 0)
    if ffd < 2 { return fail("open final".data) }
    sc_close(ffd)
    gone: i64 = sc_open("/out/result.txt".data, 0)
    if gone >= 0 { return fail("old name still present".data) }

    console_writeln("FS_ALL_PASS".data)
    return 0
}
"#;

#[test]
fn kernel_fs_full_lifecycle() {
    let image = build_fs_image(&[FsEntry::File {
        path: "/hello.txt",
        data: b"hello from fs\n",
    }]);
    let user = compile_hosted(FS_EXERCISER);
    let (_, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&user), Some(&image), "", 50_000_000);

    assert_user_exit_ok(&uart, &outcome, "fs_full_lifecycle");
    assert!(
        uart.contains("boot complete"),
        "expected boot to complete; uart={uart:?}"
    );
    assert!(
        uart.contains("[  FS  ] mounted"),
        "expected FS mount message; uart={uart:?}"
    );
    assert!(
        !uart.contains("FS_FAIL"),
        "an FS operation failed; uart={uart:?}"
    );
    assert!(
        uart.contains("FS_ALL_PASS"),
        "FS exerciser did not report success; uart={uart:?}"
    );
}

// Scan the inode table for an entry of the given type (1=file, 2=dir) and name.
// Returns true if present. Used by the file-management test to assert on-disk
// state directly, since the shell echoes typed commands and UART substring
// checks for path names would match the echo rather than real FS effects.
fn inode_present(image: &[u8], name: &str, ty: u16) -> bool {
    const BLOCK_SIZE: usize = 4096;
    const INODE_SIZE: usize = 128;
    const INODE_COUNT: usize = 256;
    const IN_TYPE: usize = 0;
    const IN_NAME: usize = 8;
    for idx in 0..INODE_COUNT {
        let base = BLOCK_SIZE + idx * INODE_SIZE;
        let t = u16::from_le_bytes([image[base + IN_TYPE], image[base + IN_TYPE + 1]]);
        if t != ty {
            continue;
        }
        let name_bytes = &image[base + IN_NAME..base + IN_NAME + 32];
        let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
        if &name_bytes[..end] == name.as_bytes() {
            return true;
        }
    }
    false
}

// Drive the shell's file-management commands (mkdir/touch/rm/mv/rmdir) and
// verify their effects on the on-disk inode table.
#[test]
fn kernel_shell_file_management() {
    let shell = compile_hosted(user::SHELL);

    // Start from an empty FS so the only inodes are the root plus what the
    // session creates.
    let image = build_fs_image(&[]);

    // mkdir a directory, touch two files in it, rm one, mv (rename) the other,
    // then exit. Final state: /work exists, /work/renamed.txt exists,
    // keep.txt and temp.txt are gone.
    let session = "mkdir /work\n\
                   touch /work/keep.txt\n\
                   touch /work/temp.txt\n\
                   rm /work/temp.txt\n\
                   mv /work/keep.txt /work/renamed.txt\n\
                   exit\n";

    let (vm, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("HLL shell ready"),
        "shell did not start; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt the VM cleanly; outcome={outcome:?} uart={uart:?}"
    );

    // No command should have reported a failure.
    assert!(
        !uart.contains("cannot create"),
        "a create command failed; uart={uart:?}"
    );
    assert!(!uart.contains("cannot remove"), "rm failed; uart={uart:?}");
    assert!(!uart.contains("cannot move"), "mv failed; uart={uart:?}");

    let final_image = vm.peek_bytes_raw(FS_IMAGE_PA, image.len());
    assert!(
        inode_present(&final_image, "work", 2),
        "mkdir did not create /work"
    );
    assert!(
        inode_present(&final_image, "renamed.txt", 1),
        "mv did not produce renamed.txt"
    );
    assert!(
        !inode_present(&final_image, "keep.txt", 1),
        "mv left the old name behind"
    );
    assert!(
        !inode_present(&final_image, "temp.txt", 1),
        "rm did not remove temp.txt"
    );
}

// Verify rmdir removes an empty directory but refuses a non-empty one.
#[test]
fn kernel_shell_rmdir_empty_and_nonempty() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[]);

    // /full holds a file (rmdir must refuse it); /empty is removable.
    let session = "mkdir /full\n\
                   touch /full/f.txt\n\
                   mkdir /empty\n\
                   rmdir /full\n\
                   rmdir /empty\n\
                   exit\n";

    let (vm, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        120_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt cleanly; outcome={outcome:?} uart={uart:?}"
    );
    // rmdir /full must fail (non-empty); rmdir /empty must succeed.
    assert!(
        uart.contains("rmdir: cannot remove: /full"),
        "rmdir should refuse a non-empty directory; uart={uart:?}"
    );

    let final_image = vm.peek_bytes_raw(FS_IMAGE_PA, image.len());
    assert!(
        inode_present(&final_image, "full", 2),
        "non-empty dir was wrongly removed"
    );
    assert!(
        !inode_present(&final_image, "empty", 2),
        "empty dir was not removed"
    );
}

#[test]
fn kernel_boots_without_fs_image() {
    let user = compile_hosted(
        r#"
main: () -> i32 {
    return 0
}
"#,
    );
    let (_, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 50_000_000);
    assert_user_exit_ok(&uart, &outcome, "no_fs_image");
    assert!(
        !uart.contains("[ FS ] mounted"),
        "FS should not mount when no image; uart={uart:?}"
    );
}

// PLAN 4.1: the OS process inspector reads scheduler/PCB state straight from guest
// memory. This guards the PCB field offsets and symbol resolution end to end: after
// boot the shell is pid 1, the running process, parented by the kernel (0).
#[test]
fn os_inspector_sees_shell_as_pid1() {
    use full_stack::view::debug::os_view::{self, OsSymbols, Role};

    let kernel = cached_kernel();
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::Dir { path: "/home" }]);

    let mut vm = setup_kernel_vm(kernel, Some(&shell), Some(&image), "");
    let _ = vm.run(80_000_000);

    let sym = OsSymbols::from_kernel(kernel).expect("kernel scheduler symbols resolve");
    let procs = os_view::capture(&vm, &sym);

    let running: Vec<_> = procs.iter().filter(|p| p.role == Role::Running).collect();
    assert_eq!(
        running.len(),
        1,
        "expected exactly one running process; got {}",
        running.len()
    );
    let shell_proc = running[0];
    assert_eq!(shell_proc.pid, 1, "shell should be pid 1");
    assert_eq!(shell_proc.parent, 0, "pid 1 is parented by the kernel (0)");
    assert!(
        shell_proc.state <= 3,
        "decoded a nonsense state {} (PCB offsets wrong?)",
        shell_proc.state
    );
}

// --- FS layout / symbol checks (no VM needed) ---

#[test]
fn kernel_fs_user_globals_in_mapped_range() {
    let assembled = compile_hosted(FS_EXERCISER);
    let flat = assembled.to_flat_binary();
    let padded = (flat.len().div_ceil(4096) * 4096) as u64;

    for sym in &["_a0", "_a1", "_a2", "_a3", "_ret"] {
        let off = assembled
            .symbol_address(sym)
            .unwrap_or_else(|| panic!("symbol '{sym}' missing from user binary"));
        assert!(
            off + 8 <= padded,
            "global '{sym}' at offset {off:#x} extends past mapped region end {padded:#x}"
        );
    }
}

#[test]
fn kernel_fs_path_literals_in_mapped_range() {
    let assembled = compile_hosted(FS_EXERCISER);
    let flat = assembled.to_flat_binary();
    let padded = flat.len().div_ceil(4096) * 4096;

    for needle in &[
        &b"/hello.txt\0"[..],
        &b"/out\0"[..],
        &b"/out/result.txt\0"[..],
        &b"/out/final.txt\0"[..],
    ] {
        let pos = flat
            .windows(needle.len())
            .position(|w| w == *needle)
            .unwrap_or_else(|| panic!("path literal {:?} not found in user binary", needle));
        assert!(
            pos + needle.len() <= padded,
            "path literal {needle:?} ends past mapped region"
        );
    }
}
