// Booting a VM is the slowest thing in the project, so the whole suite lives
// here and shares at most two OnceLock-cached kernel builds.

use asm_to_binary::AssembledOutput;
use full_stack::build::{BuildExecutor, BuildManifest};
use full_stack::compilation_pipeline::{FsEntry, assembled_to_elf_file, build_fs_image};
use full_stack::userspace as user;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Physical layout constants matching the OS spec.
const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

// --- Cached kernel binary ---

// The kernel every test boots, built from kernel.build the way the app does:
// per-module stdlib objects linked with my_kernel (the separate-object path).
fn cached_kernel() -> &'static AssembledOutput {
    static KERNEL: OnceLock<AssembledOutput> = OnceLock::new();
    KERNEL.get_or_init(|| {
        let manifest = BuildManifest::from_file(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("crates/os-runtime/kernel/kernel.build"),
        )
        .expect("kernel build file parse");
        BuildExecutor::build(&manifest)
            .expect("kernel manifest build")
            .linked
    })
}

// --- Shared helpers ---

// Compile a hosted user program and link the hosted stdlib.
fn compile_hosted(src: &str) -> AssembledOutput {
    let manifest = BuildManifest::hosted("main", src);
    BuildExecutor::build(&manifest)
        .expect("hosted manifest build")
        .linked
}

// Compile a catalog program the way the app boot path does. Memoized by name so
// the many tests that rebuild `shell`/`as`/`cc`/`ld` share one compile + a clone.
fn compile_hosted_program(name: &str) -> AssembledOutput {
    static CACHE: OnceLock<Mutex<HashMap<String, AssembledOutput>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(out) = cache.lock().unwrap().get(name) {
        return out.clone();
    }
    // Compile outside the lock so unrelated names build in parallel; a rare race
    // just rebuilds one program twice, which is harmless.
    let prog = user::program(name).expect("program in catalog");
    let out = BuildExecutor::build(&BuildManifest::from_user_program(prog))
        .expect("catalog hosted manifest build")
        .linked;
    cache.lock().unwrap().insert(name.to_owned(), out.clone());
    out
}

// Builds the VM without running it, so a caller can either run it in one shot
// (boot_kernel) or step it manually and inject input mid-run.
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

// Run in chunks until `done(uart_so_far)`, a halt, or the step cap. An idle
// shell never halts the VM, so a flat run would burn its whole cap.
fn run_until(
    vm: &mut VirtualMachine,
    cap: u64,
    done: impl Fn(&str) -> bool,
) -> (StepOutcome, String) {
    const CHUNK: u64 = 1_000_000;
    let mut uart = String::new();
    let mut outcome = StepOutcome::Continue;
    let mut steps = 0u64;
    while steps < cap && !done(&uart) {
        let r = vm.run(CHUNK.min(cap - steps));
        uart.push_str(&r.uart_output);
        steps += CHUNK;
        outcome = r.outcome;
        if matches!(outcome, StepOutcome::Halted(_)) {
            break;
        }
    }
    (outcome, uart)
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

fn run_example_manifest_in_kernel(name: &str) {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("programs/example")
        .join(name)
        .join(format!("{name}.build"));
    let manifest = BuildManifest::from_file(&manifest_path)
        .unwrap_or_else(|e| panic!("{}: parse failed: {e}", manifest_path.display()));
    let user = BuildExecutor::build(&manifest)
        .unwrap_or_else(|e| panic!("{}: build failed: {e}", manifest_path.display()))
        .linked;
    let (_, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 10_000_000);
    assert_user_exit_ok(&uart, &outcome, name);
}

// Collapses the compile-shell, elf-ify-tools, build-image and boot boilerplate
// every toolchain test repeats. Tests keep their own assertions.
fn run_tool_session(
    tools: &[&str],
    files: &[(&str, &[u8])],
    session: &str,
    max_steps: u64,
) -> (StepOutcome, String) {
    let shell = compile_hosted_program("shell");
    let tool_elfs: Vec<(&str, Vec<u8>)> = tools
        .iter()
        .map(|name| {
            let prog = user::program(name).expect("tool not in user catalog");
            let path = prog.install_path.expect("tool has no install path");
            let elf = assembled_to_elf_file(&compile_hosted_program(name));
            (path, elf)
        })
        .collect();

    // Create every parent directory the tool/file paths need, shallowest first.
    let mut dirs: Vec<String> = Vec::new();
    for path in tool_elfs
        .iter()
        .map(|(p, _)| *p)
        .chain(files.iter().map(|(p, _)| *p))
    {
        let mut idx = 0;
        while let Some(slash) = path[idx + 1..].find('/') {
            let end = idx + 1 + slash;
            let dir = path[..end].to_owned();
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
            idx = end;
        }
    }
    dirs.sort_by_key(|d| d.matches('/').count());

    let mut entries: Vec<FsEntry<'_>> = dirs.iter().map(|d| FsEntry::Dir { path: d }).collect();
    for (path, elf) in &tool_elfs {
        entries.push(FsEntry::File { path, data: elf });
    }
    for (path, data) in files {
        entries.push(FsEntry::File { path, data });
    }
    let image = build_fs_image(&entries);

    let (_, outcome, uart) = boot_kernel(
        cached_kernel(),
        Some(&shell),
        Some(&image),
        session,
        max_steps,
    );
    (outcome, uart)
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

// Booting with no user binary: the kernel spawns nothing and shuts down cleanly.
#[test]
fn kernel_boot_no_user_binary() {
    let (_, outcome, uart) = boot_kernel(cached_kernel(), None, None, "", 10_000_000);
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

#[test]
fn example_core_basics_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("core_basics");
}

#[test]
fn example_operators_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("operators");
}

#[test]
fn example_pointers_and_places_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("pointers_and_places");
}

#[test]
fn example_arrays_slices_and_ranges_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("arrays_slices_and_ranges");
}

#[test]
fn example_structs_and_binding_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("structs_and_binding");
}

#[test]
fn example_generics_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("generics");
}

#[test]
fn example_enums_match_and_result_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("enums_match_and_result");
}

#[test]
fn example_strings_and_iteration_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("strings_and_iteration");
}

#[test]
fn example_compile_time_math_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("compile_time_math");
}

#[test]
fn example_hll2_completion_showcase_runs_in_kernel_userspace() {
    run_example_manifest_in_kernel("hll2_completion_showcase");
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

// Five 50 KB blocks force several sbrk page grows past the old 64 KB limit.
// Touching the far end of each block proves the new pages are really mapped.
#[test]
fn user_heap_grows_past_64k() {
    run_example_in_kernel(
        r#"external malloc: (size: u64) -> u8*
main: () -> i32 {
    i: u64 = 0
    while i < 5 {
        p: u8* = malloc(50000)
        if p == null {
            return 1
        }
        far: u8* = p + 49999
        v: u8 = (i + 1) as u8
        @far = v
        if (@far) != v {
            return 2
        }
        i = i + 1
    }
    return 0
}
"#,
        "user_heap_grows_past_64k",
    );
}

// A grow above USER_ARG_BASE (0x7FF00000) would eat the arg page and stack, so
// brk returns the unchanged break and a later in-bounds grow still succeeds.
#[test]
fn user_brk_refuses_growth_into_arg_page() {
    run_example_in_kernel(
        r#"external heap_brk: (addr: u64) -> u64
main: () -> i32 {
    base: u64 = heap_brk(0)
    big: u64 = 0x7FFFF000
    got: u64 = heap_brk(big)
    if got >= big {
        return 1
    }
    if got != base {
        return 2
    }
    ok: u64 = heap_brk(base + 4096)
    if ok < base + 4096 {
        return 3
    }
    return 0
}
"#,
        "user_brk_refuses_growth_into_arg_page",
    );
}

// --- Framebuffer device (map_fb syscall + fbdemo program) ---

// Independent Q16.16 reference for the same set the f64 demo renders (different
// arithmetic on purpose); the two agree except on boundary pixels.
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

// Covers map_fb -> MMU translation -> bus routing -> device store, plus f64
// codegen: any float distortion shears the rendered set off the reference.
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

    // Correct codegen agrees with the reference exactly. A sheared or truncated
    // render disagrees on thousands of pixels.
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

// The demo loops forever, so the bounded run almost always stops inside the
// full-screen clear. Assert on the clear, not on timing-dependent edge pixels.
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

// Bounded like the cube run, so assert on the device FILL clear plus at least
// one live cell rather than on exact cell positions.
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

// Headless framebuffer throughput bench: a measurement, not a pass/fail, so it
// carries no assertion and stays #[ignore]d for manual release runs.
#[test]
#[ignore = "manual framebuffer throughput benchmark"]
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
    let shell = compile_hosted_program("shell");

    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("HELLO_FROM_CHILD".ptr)
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
    let shell = compile_hosted_program("shell");

    let child = compile_hosted(
        r#"
external print_int: (value: i64) -> i32
external console_writeln: (str: u8*)
g: i64 = 12345
arr: i64[3] = [100, 200, 300]
main: () -> i32 {
    console_writeln("SCALAR".ptr)
    print_int(g)
    console_writeln("ARRAY".ptr)
    print_int(arr[0] + arr[1] + arr[2])
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

// Sums 1..=7 and exits 28 through the in-VM `as`. A wrong branch offset or
// immediate faults or loops forever instead of printing "[exit 28]".
#[test]
fn kernel_shell_assembles_and_runs_program() {
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

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.elf\nrun /prog.elf\nexit\n",
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

// The shell chains `cmd1 && cmd2` on one line, running the second only if the first
// exits 0. Assemble then run in a single line and confirm both stages ran. PLAN 4.2.
#[test]
fn kernel_shell_and_chains_on_success() {
    let source = b"\
  li a0, 7
  li a7, 93
  ecall
";

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.elf && run /prog.elf\nexit\n",
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("as: wrote /prog.elf"),
        "first stage did not run; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 7]"),
        "second stage did not run after `&&`; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// A failing first command (a non-zero exit) short-circuits the `&&` chain: the
// second command must not run. Guards the exit-code gating in run_line.
#[test]
fn kernel_shell_and_stops_on_failure() {
    let (outcome, uart) = run_tool_session(
        &["as"],
        &[],
        "as /missing.s /out.elf && echo SECOND_RAN\nexit\n",
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("as: cannot open"),
        "first stage did not fail as expected; uart={uart:?}"
    );
    // The marker appears once as the echoed command line; if `echo` had run it would
    // appear a second time as output. Gating means exactly one occurrence.
    assert_eq!(
        uart.matches("SECOND_RAN").count(),
        1,
        "second stage ran despite the first failing; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// A ".o" output makes `as` emit ET_REL. This only proves the object writer runs
// without faulting; correctness is covered where ld.hll links the result.
#[test]
fn kernel_as_emits_object() {
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

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.o\nexit\n",
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

// The program self-checks each result and branches to `fail`, so one wrong
// encoding gives exit 1 or a fault. Only `[exit 42]` means the batch is right.
#[test]
fn kernel_shell_assembles_expanded_subset() {
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

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.elf\nrun /prog.elf\nexit\n",
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

// M-extension plus addiw were the last backend mnemonics the in-VM assembler
// lacked. Self-checked like the subset test, so only `[exit 42]` passes.
#[test]
fn kernel_shell_assembles_m_extension() {
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

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.elf\nrun /prog.elf\nexit\n",
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

// A non-ELF or missing name must be an unknown command, not an opaque failure
// inside sys_exec. `run` and bare names share one PATH-resolving path.
#[test]
fn kernel_shell_run_rejects_non_executable() {
    let shell = compile_hosted_program("shell");
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

// A bare program name resolves through the PATH search (cwd, /bin, /home/demo).
// Also covers a relative `./child.elf` and `&` on the shared launch path.
#[test]
fn kernel_shell_bare_name_runs_program() {
    let shell = compile_hosted_program("shell");

    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("BARE_NAME_RAN".ptr)
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

// Inode table starts at block 1 (offset 4096). Each inode is 128 bytes: type at
// +0 (u16), size at +4 (u32), name at +8.
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
    let shell = compile_hosted_program("shell");
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

// The ed-style editor: goto, substitute, insert, delete, write. The on-disk
// contents must reflect every operation, not just the last.
#[test]
fn editor_line_operations_insert_substitute_delete() {
    // alpha / bravo / charlie
    //   2, s/bravo/BRAVO/   -> alpha / BRAVO / charlie
    //   i inserted          -> alpha / inserted / BRAVO / charlie
    //   1 d                 -> inserted / BRAVO / charlie
    let (_outcome, uart) = run_tool_session(
        &["edit"],
        &[("/doc.txt", b"alpha\nbravo\ncharlie\n")],
        "edit /doc.txt\n\
2\n\
s/bravo/BRAVO/\n\
i\ninserted\n.\n\
g 1\nd\n\
w\nq\n\
cat /doc.txt\nexit\n",
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

// A child asleep in sc_readchar leaves the ready queue, so `waitpid` must use
// scheduler_pid_alive or it returns -1 and abandons a live child.
#[test]
fn editor_waits_while_idle_for_input() {
    let shell = compile_hosted_program("shell");
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

    // Let it boot and launch the editor, then idle (spinning on
    // sc_readchar/sc_yield under timer preemption) with no input available.
    let (_, boot_uart) = run_until(&mut vm, 60_000_000, |u| u.contains("edit: p=print"));
    let idle = vm.run(5_000_000);
    let uart1 = format!("{boot_uart}{}", idle.uart_output);
    assert!(
        uart1.contains("edit: p=print"),
        "editor did not start; uart={uart1:?}"
    );

    // Now the user "types" a command. Feed it and continue.
    for b in "p\nq\nexit\n".bytes() {
        vm.push_uart_rx(b);
    }
    let (_, uart2) = run_until(&mut vm, 60_000_000, |u| u.contains("li t0, 3"));
    let uart = format!("{uart1}{uart2}");

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

// Guards the shipped `array.s` example and the expanded assembler subset at
// once: the stack array sums to 42 through the in-VM `as`.
#[test]
fn example_array_assembles_and_runs() {
    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/array.s", user::EXAMPLE_ARRAY_S.as_bytes())],
        "as /array.s /array.elf\nrun /array.elf\nexit\n",
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

// Text/pad/data layout, PC-relative `la` and `call` encoding, and the `jalr`
// return must all round-trip through the in-VM `as`.
#[test]
fn kernel_shell_assembles_data_and_calls() {
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

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/prog.s", source)],
        "as /prog.s /prog.elf\nrun /prog.elf\nexit\n",
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

#[test]
fn kernel_shell_assembles_section_and_data_directives() {
    let source = b"\
.section .text
.globl _start
_start:
  la t0, bytes
  lbu t1, 0(t0)
  li t2, 7
  bne t1, t2, fail
  lw t1, 4(t0)
  li t2, 42
  bne t1, t2, fail
  lbu t1, 8(t0)
  li t2, 65
  bne t1, t2, fail
  lbu t1, 9(t0)
  li t2, 66
  bne t1, t2, fail
  lbu t1, 10(t0)
  bne t1, zero, fail
  la t0, robyte
  lbu t1, 0(t0)
  li t2, 9
  bne t1, t2, fail
  la t0, zeroed
  lbu t1, 0(t0)
  bne t1, zero, fail
  li a0, 42
  li a7, 93
  ecall
fail:
  li a0, 1
  li a7, 93
  ecall
.section .data
bytes:
  .byte 7
  .zero 3
  .word 42
  .ascii \"A\"
  .asciz \"B\"
.rodata
robyte:
  .byte 9
.bss
zeroed:
  .zero 1
";

    let (outcome, uart) = run_tool_session(
        &["as"],
        &[("/directives.s", source)],
        "as /directives.s /directives.elf\nrun /directives.elf\nexit\n",
        200_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("as: wrote /directives.elf"),
        "assembler failed; uart={uart:?}"
    );
    assert!(
        uart.contains("[exit 42]"),
        "section or data directive encoding changed; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "shell did not exit cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// Host-built static ELFs must run unmodified: sys_exec maps each PT_LOAD at its
// p_vaddr, zeroes BSS, and jumps e_entry.
#[test]
fn kernel_loads_and_runs_elf() {
    let shell = compile_hosted_program("shell");
    let prog = compile_hosted(
        "external console_writeln: (s: u8*)\n\
         main: () -> i32 {\n\
         \tconsole_writeln(\"elf ok\".ptr)\n\
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

// Pins the HLL-0 codegen target: host-compiled hello.hll and the hand-written
// hello.s must agree, both printing "HLL0" and exiting 36.
#[test]
fn kernel_cc_target_roundtrips() {
    let host_elf = assembled_to_elf_file(&compile_hosted(user::CC_HELLO_HLL));

    // Run the host-compiled source, then assemble + run the hand-written target.
    let (outcome, uart) = run_tool_session(
        &["as"],
        &[
            ("/hello.s", user::CC_HELLO_S.as_bytes()),
            ("/hello_host.elf", &host_elf),
        ],
        "run /hello_host.elf\nas /hello.s /hello.elf\nrun /hello.elf\nexit\n",
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
    // marker and exit 36, so each appears twice.
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

// `cc.hll` is built into /bin/cc.elf, so it must host-compile. Fast guard for
// HLL errors without booting the kernel.
#[test]
fn cc_host_compiles() {
    let out = compile_hosted_program("cc");
    assert!(
        !out.to_flat_binary().is_empty(),
        "cc.hll produced an empty binary"
    );
}

// Separate compilation entirely in-VM: a.s calls an external `get_val` (CALL_PLT)
// and b.s loads its own `.data` via `la` (PCREL_HI20), so `[exit 42]` covers both.
#[test]
fn kernel_ld_links_objects_and_runs() {
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

    let (outcome, uart) = run_tool_session(
        &["as", "ld"],
        &[("/a.s", a_s), ("/b.s", b_s)],
        "as /a.s /a.o\nas /b.s /b.o\nld /a.o /b.o /prog\nrun /prog\nexit\n",
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

// The client inlines no I/O, so puts/putc/exit resolve at link time against a
// separately assembled stdlib.s. Covers CALL_PLT, a local call, and PCREL_HI20.
#[test]
fn kernel_ld_links_stdlib_and_runs() {
    let client_s = b"\
.globl _start
.text
_start:
  la a0, msg
  call puts
  li a0, 33
  call putc
  li a0, 10
  call putc
  li a0, 42
  call exit
.data
msg:
  .asciz \"hello from stdlib\"
";

    let (outcome, uart) = run_tool_session(
        &["as", "ld"],
        &[
            ("/stdlib.s", user::EXAMPLE_STDLIB_S.as_bytes()),
            ("/client.s", client_s),
        ],
        "as /stdlib.s /stdlib.o\nas /client.s /client.o\n\
ld /stdlib.o /client.o /hello\nrun /hello\nexit\n",
        300_000_000,
    );

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        !uart.contains("unhandled exception"),
        "the linked program faulted; uart={uart:?}"
    );
    assert!(
        uart.contains("as: wrote /stdlib.o") && uart.contains("as: wrote /client.o"),
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

// The whole in-VM toolchain in one run: cc, as, ld, then the loader. putc is no
// longer a cc intrinsic, so it resolves at link time against stdlib.s.
#[test]
fn kernel_cc_compiles_and_runs() {
    let (outcome, uart) = run_tool_session(
        &["as", "cc", "ld"],
        &[
            ("/prog.hll", user::CC_DEMO_HLL.as_bytes()),
            ("/stdlib.s", user::EXAMPLE_STDLIB_S.as_bytes()),
        ],
        "cc /prog.hll /prog.s\nas /prog.s /prog.o\nas /stdlib.s /stdlib.o\n\
ld /stdlib.o /prog.o /prog\nrun /prog\nexit\n",
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
        uart.contains("ld: wrote /prog"),
        "linker did not report success; uart={uart:?}"
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
    let (outcome, uart) = run_tool_session(
        &["as", "cc", "ld"],
        &[
            ("/home/src/hello.hll", user::CC_DEMO_HLL.as_bytes()),
            ("/home/src/stdlib.s", user::EXAMPLE_STDLIB_S.as_bytes()),
        ],
        "cd /home/src\ncc hello.hll hello.s\nas hello.s hello.o\nas stdlib.s stdlib.o\n\
ld stdlib.o hello.o hello\nhello\nexit\n",
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

// Regression: a trailing space on a `cc`/`as` output operand once became part of
// the filename. The extensionless output also covers cwd resolution.
#[test]
fn kernel_cc_tool_arg_trailing_space() {
    // Trailing spaces after the output operand on the cc/as/ld invocations.
    let (outcome, uart) = run_tool_session(
        &["as", "cc", "ld"],
        &[
            ("/home/src/hello.hll", user::CC_DEMO_HLL.as_bytes()),
            ("/home/src/stdlib.s", user::EXAMPLE_STDLIB_S.as_bytes()),
        ],
        "cd /home/src\ncc hello.hll hello.s \nas hello.s hello.o \nas stdlib.s stdlib.o\n\
ld stdlib.o hello.o hello \nhello\nexit\n",
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

// A finished program must return to the prompt, not halt the VM. The marker
// appears twice and the VM halts only on the shell's own `exit`.
#[test]
fn kernel_shell_survives_run_and_continues() {
    let shell = compile_hosted_program("shell");
    let child = compile_hosted(
        r#"
external console_writeln: (str: u8*)
main: () -> i32 {
    console_writeln("RAN_CHILD".ptr)
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

// The child loops forever, so the VM can only reach the trailing `exit` if the
// 0x03 byte tore the foreground child down and returned to the prompt.
#[test]
fn kernel_shell_ctrl_c_interrupts_foreground() {
    let shell = compile_hosted_program("shell");
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
    let shell = compile_hosted_program("shell");
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
    // neither real pid (2, 3) is ever reported "done": both spin forever.
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

// The spinner never exits on its own, so a later empty `jobs` is the only proof
// that the SIGKILL teardown (scheduler_kill_pid, syscall 129) really ran.
#[test]
fn kernel_kill_background_job() {
    let shell = compile_hosted_program("shell");
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

// `kill %<job>` maps a job id through the shell's job table onto the same SIGKILL
// path as `kill <pid>`.
#[test]
fn kernel_kill_by_jobid() {
    let shell = compile_hosted_program("shell");
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

// `prog > file` binds the child's fd 1 to an FS file. Ordering is the proof: a
// failed redirect would print the marker to the UART before `cat` reads it back.
#[test]
fn kernel_redirect_stdout_to_file() {
    let shell = compile_hosted_program("shell");
    let printer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("REDIRECT_OK".ptr)
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
// shows both lines, proving `>>` seeks to end instead of truncating.
#[test]
fn kernel_redirect_append() {
    let shell = compile_hosted_program("shell");
    let printer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("LINE".ptr)
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

// The child's fd 0 reads from an FS file and must see EOF (-1) at its end. echo
// copies stdin to stdout, so the file contents surface on the console.
#[test]
fn kernel_redirect_stdin_from_file() {
    let shell = compile_hosted_program("shell");
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

// `ls <dir>` must list a directory other than cwd. It once fell through to
// bare-name execution and failed with "unknown command".
#[test]
fn kernel_ls_dir_lists_target() {
    let shell = compile_hosted_program("shell");
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

// `echo` runs inside the shell, so its `>` redirect goes through the shell-side
// sink rather than the kernel fd table. `cat` reads the result back.
#[test]
fn kernel_echo_redirect_to_file() {
    let shell = compile_hosted_program("shell");
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

// A builtin `cat <file> >> <dst>` appends through the shell sink via sc_lseek,
// so two appends of one source must read back twice.
#[test]
fn kernel_cat_append_builtin() {
    let shell = compile_hosted_program("shell");
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

// The shell binds the producer's stdout to a temp file the filter reads back, so
// the payload only reaches the console if it travelled through the pipe.
#[test]
fn kernel_pipe_two_stage() {
    let shell = compile_hosted_program("shell");
    let producer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("PIPE_PAYLOAD".ptr)
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

// With `producer | filter > out.txt` the filter's stdout is captured to a file,
// so the payload may appear only after the read-back `cat`, never before.
#[test]
fn kernel_pipe_to_file() {
    let shell = compile_hosted_program("shell");
    let producer = compile_hosted(
        r#"
external console_writeln: (str: u8*)
external sc_exit: (code: i64)
main: () -> i32 {
    console_writeln("VIA_PIPE".ptr)
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

// A builtin producer: `cat` runs in the shell with its output bound to the pipe
// temp file, exercising the builtin branch of sh_run_stage.
#[test]
fn kernel_pipe_builtin_source() {
    let shell = compile_hosted_program("shell");
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

// `cat` with no file argument copies its input source to the sink, so as a pipe
// consumer it must print what the shell wired to its stdin.
#[test]
fn kernel_pipe_echo_into_cat() {
    let shell = compile_hosted_program("shell");
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

// The shell's pipe temp file starts with a dot and `ls` hides dotfiles, so it
// must not appear in the listing (regression for a leaked `.pipe0` entry).
#[test]
fn kernel_pipe_ls_hides_temp_file() {
    let shell = compile_hosted_program("shell");
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
    let shell = compile_hosted_program("shell");
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

// Input is injected mid-run, the way the GUI does it, so the shell really sleeps
// in readchar and an incoming byte must raise the RX interrupt to wake it.
#[test]
fn kernel_shell_interrupt_wake_lists_running_background_job() {
    let shell = compile_hosted_program("shell");
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
        let mut steps = 0u64;
        while steps < 30_000_000 && !out.contains(marker) {
            let r = vm.run(1_000_000);
            out.push_str(&r.uart_output);
            steps += 1_000_000;
        }
        out.contains(marker)
    };

    // Boot to the prompt (the shell is now blocked in its first readchar).
    let mut steps = 0u64;
    while steps < 10_000_000 && !out.contains("shell ready") {
        let r = vm.run(1_000_000);
        out.push_str(&r.uart_output);
        steps += 1_000_000;
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

    // The byte must wake the blocked shell and list the job as running. "no
    // background jobs" would mean the live job was wrongly reaped from the table.
    assert!(
        feed(&mut vm, &mut out, b"jobs\n", "running"),
        "interactive `jobs` did not list the running background job; out={out:?}"
    );
    assert!(
        !out.contains("no background jobs"),
        "shell reported no jobs while one was running; out={out:?}"
    );
}

// The child spins before printing, so "[1] pid N" must precede its "SLOW_DONE".
// `fg <n>` then reaps the exact exit code through waitpid (syscall 261).
#[test]
fn kernel_shell_background_job_runs_and_fg_reaps_it() {
    let shell = compile_hosted_program("shell");

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
    console_writeln("SLOW_DONE".ptr)
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
        console_writeln("CHILD_RAN".ptr)
        sc_exit(42)
    }

    console_writeln("PARENT_FORKED".ptr)
    code: i64 = sc_wait()
    if code == 42 {
        console_writeln("PARENT_REAPED_42".ptr)
    } else {
        console_writeln("PARENT_REAPED_WRONG".ptr)
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

// No syscalls run between marks, so only the timer interrupt can switch parent
// and child. Interleaved A/B proves preemption, sequential AAAA...BBBB does not.
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
    console_writeln("SENTINEL_START".ptr)
    pid: i64 = sc_fork()
    if pid == 0 {
        n: u64 = 0
        while n < 4 {
            spin()
            sc_write(1, "B".ptr, 1)
            n = n + 1
        }
        sc_exit(0)
    }
    m: u64 = 0
    while m < 4 {
        spin()
        sc_write(1, "A".ptr, 1)
        m = m + 1
    }
    sc_wait()
    console_writeln("SENTINEL_END".ptr)
    sc_exit(0)
    return 0
}
"#,
    );

    let image = build_fs_image(&[]);
    let mut vm = setup_kernel_vm(cached_kernel(), Some(&parent), Some(&image), "");
    let (_outcome, uart) = run_until(&mut vm, 200_000_000, |u| u.contains("SENTINEL_END"));

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
external sc_exec: (path: u8[]) -> i64
external sc_pidalive: (pid: u64) -> i64
external sc_yield: ()
external sc_exit: (code: i64)

main: () -> i32 {{
    p: u8* = {shared} as u8*
    @p = 0xAA as u8
    console_writeln("PARENT_WROTE".ptr)

    pid: i64 = sc_exec("/child.elf")
    if pid < 0 {{
        console_writeln("PARENT_EXEC_FAIL".ptr)
        sc_exit(1)
    }}
    while sc_pidalive(pid as u64) == 1 {{
        sc_yield()
    }}

    v: u8 = @p
    if v == 0xAA as u8 {{
        console_writeln("PARENT_FINAL_AA".ptr)
    }} else {{
        console_writeln("PARENT_FINAL_CORRUPT".ptr)
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
        console_writeln("CHILD_FINAL_BB".ptr)
    }} else {{
        console_writeln("CHILD_FINAL_BAD".ptr)
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
external sc_open:   (path: u8[], flags: u64) -> i64
external sc_close:  (fd: i64)
external sc_read:   (fd: i64, buf: u8*, offset: u64, len: u64) -> i64
external sc_write:  (fd: i64, buf: u8*, len: u64) -> i64
external sc_mkdir:  (path: u8[]) -> i64
external sc_rename: (old_path: u8[], new_path: u8[]) -> i64

fail: (label: u8*) -> i32 {
    console_write("FS_FAIL: ".ptr)
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
    buf: u8[64] = []

    ; 1. Read a pre-populated file.
    fd: i64 = sc_open("/hello.txt", 0)
    if fd < 2 { return fail("open hello".ptr) }
    n: i64 = sc_read(fd, &buf[0], 0, 13)
    if n != 13 { return fail("read hello count".ptr) }
    if bytes_eq(&buf[0], "hello from fs".ptr, 13) == 0 { return fail("read hello data".ptr) }
    sc_close(fd)

    ; 2. Create a directory.
    if sc_mkdir("/out") < 0 { return fail("mkdir out".ptr) }

    ; 3. Create a file in it and write to it.
    wfd: i64 = sc_open("/out/result.txt", 2)
    if wfd < 2 { return fail("create result".ptr) }
    w: i64 = sc_write(wfd, "data123".ptr, 7)
    if w != 7 { return fail("write count".ptr) }
    sc_close(wfd)

    ; 4. Re-open and read back the written contents.
    rfd: i64 = sc_open("/out/result.txt", 0)
    if rfd < 2 { return fail("reopen result".ptr) }
    rn: i64 = sc_read(rfd, &buf[0], 0, 7)
    if rn != 7 { return fail("readback count".ptr) }
    if bytes_eq(&buf[0], "data123".ptr, 7) == 0 { return fail("readback data".ptr) }
    sc_close(rfd)

    ; 5. Rename within the directory; old name must disappear.
    if sc_rename("/out/result.txt", "/out/final.txt") != 0 {
        return fail("rename".ptr)
    }
    ffd: i64 = sc_open("/out/final.txt", 0)
    if ffd < 2 { return fail("open final".ptr) }
    sc_close(ffd)
    gone: i64 = sc_open("/out/result.txt", 0)
    if gone >= 0 { return fail("old name still present".ptr) }

    console_writeln("FS_ALL_PASS".ptr)
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

#[test]
fn kernel_slice_paths_are_bounded_at_the_syscall_boundary() {
    let user = compile_hosted(
        r#"
sys := import("sys")

main: () -> i32 {
    backing: u8[] = "/hello.txt.extra"
    fd: i64 = sys.open(backing[0..10], 0)
    if fd < 2 { return 1 }
    sys.close(fd)

    embedded: u8[4] = []
    embedded[0] = '/' as u8
    embedded[1] = 'x' as u8
    embedded[2] = 0 as u8
    embedded[3] = 'y' as u8
    if sys.stat(embedded[..]) >= 0 { return 2 }

    long_path: u8[256] = []
    long_path[0] = '/' as u8
    for i in 1..256 {
        long_path[i] = 'a' as u8
    }
    if sys.stat(long_path[..]) >= 0 { return 3 }

    bad: u8[] = ""
    bad.ptr = 0x90000000 as u8*
    bad.len = 4
    if sys.stat(bad) >= 0 { return 4 }

    name: u8[10] = []
    name[9] = 0xAA as u8
    entry: i64 = sys.readdir("/", 0, name[0..9])
    if entry < 0 || ((entry as u64) >> 8) != 9 { return 5 }
    if name[9] != 0xAA as u8 { return 6 }
    return 0
}
"#,
    );
    let image = build_fs_image(&[FsEntry::File {
        path: "/hello.txt",
        data: b"bounded\n",
    }]);
    let (_, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&user), Some(&image), "", 50_000_000);
    assert_user_exit_ok(&uart, &outcome, "slice_path_boundary");
}

// Type is 1=file, 2=dir. On-disk state is checked directly because the shell
// echoes typed commands, so a UART substring would match the echo instead.
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
    let shell = compile_hosted_program("shell");

    // Start from an empty FS so the only inodes are the root plus what the
    // session creates.
    let image = build_fs_image(&[]);

    // Final state: /work and /work/renamed.txt exist, keep.txt and temp.txt are
    // gone.
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
    let shell = compile_hosted_program("shell");
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

// The inspector reads scheduler and PCB state straight from guest memory, so this
// guards the PCB field offsets and symbol resolution end to end.
#[test]
fn os_inspector_sees_shell_as_pid1() {
    use full_stack::inspect::os_view::{self, OsSymbols, Role};

    let kernel = cached_kernel();
    let shell = compile_hosted_program("shell");
    let image = build_fs_image(&[FsEntry::Dir { path: "/home" }]);

    let mut vm = setup_kernel_vm(kernel, Some(&shell), Some(&image), "");
    let _ = run_until(&mut vm, 80_000_000, |uart| uart.contains("shell ready"));
    // Settle past the prompt so the shell has entered its blocking readchar.
    let _ = vm.run(1_000_000);

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

// The stack view walks a process's user stack by software Sv39 translation, so a
// shell that has already made syscalls must yield at least one mapped word.
#[test]
fn os_inspector_walks_pid1_stack() {
    use full_stack::inspect::os_view::{self, OsSymbols, Role};

    let kernel = cached_kernel();
    let shell = compile_hosted_program("shell");
    let image = build_fs_image(&[FsEntry::Dir { path: "/home" }]);

    let mut vm = setup_kernel_vm(kernel, Some(&shell), Some(&image), "");
    let _ = run_until(&mut vm, 80_000_000, |uart| uart.contains("shell ready"));
    // Settle past the prompt so the shell has entered its blocking readchar.
    let _ = vm.run(1_000_000);

    let sym = OsSymbols::from_kernel(kernel).expect("kernel scheduler symbols resolve");
    let procs = os_view::capture(&vm, &sym);
    let shell_proc = procs
        .iter()
        .find(|p| p.role == Role::Running)
        .expect("a running process");

    assert!(
        shell_proc.sp != 0 && shell_proc.sp < 0x8000_0000,
        "saved sp {:#x} is not a user-stack address (trap-frame offset wrong?)",
        shell_proc.sp
    );
    assert!(
        shell_proc.page_root != 0,
        "pid 1 should have its own page-table root"
    );

    let stack = os_view::capture_stack(&vm, shell_proc);
    assert!(!stack.is_empty(), "stack walk produced no words");
    assert!(
        stack[0].va == shell_proc.sp && stack[0].mapped,
        "the word at sp {:#x} should translate and be mapped",
        shell_proc.sp
    );
}

// The syscall trace ring records every U-mode ecall; the strace panel drains it.
// After boot the shell has polled input (readchar), so the ring holds its calls.
#[test]
fn os_inspector_traces_shell_syscalls() {
    use full_stack::inspect::os_view::{self, OsSymbols};

    let kernel = cached_kernel();
    let shell = compile_hosted_program("shell");
    let image = build_fs_image(&[FsEntry::Dir { path: "/home" }]);

    let mut vm = setup_kernel_vm(kernel, Some(&shell), Some(&image), "");
    let _ = run_until(&mut vm, 80_000_000, |uart| uart.contains("shell ready"));
    // Settle past the prompt so the shell has entered its blocking readchar.
    let _ = vm.run(1_000_000);

    let sym = OsSymbols::from_kernel(kernel).expect("kernel scheduler symbols resolve");
    let trace = os_view::capture_trace(&vm, &sym);

    assert!(
        !trace.is_empty(),
        "expected the shell to have traced syscalls"
    );
    // Newest first: seq is strictly decreasing and every entry is the shell (pid 1).
    for pair in trace.windows(2) {
        assert!(pair[0].seq > pair[1].seq, "trace not ordered newest-first");
    }
    assert!(
        trace.iter().all(|e| e.pid == 1),
        "only the shell (pid 1) has run, so every traced call should be its"
    );
    // The shell idles in readchar (syscall 100); it must show up in the ring.
    assert!(
        trace.iter().any(|e| e.num == 100),
        "expected a readchar (100) syscall in the trace"
    );
}

#[test]
fn syscall_trace_wrap_preserves_kernel_state() {
    use full_stack::inspect::os_view::{self, OsSymbols};

    let kernel = cached_kernel();
    let user = compile_hosted(
        r#"
sys := import("sys")

main: () -> i32 {
    for i in 0..300 {
        sys.yield()
    }
    return 0
}
"#,
    );
    let (vm, outcome, uart) = boot_kernel(kernel, Some(&user), None, "", 50_000_000);
    assert_user_exit_ok(&uart, &outcome, "trace_wrap");

    let sym = OsSymbols::from_kernel(kernel).expect("kernel trace symbols resolve");
    let trace = os_view::capture_trace(&vm, &sym);
    assert_eq!(trace.len(), 64);
    assert!(trace.iter().all(|entry| entry.num == 2));
    assert!(trace.windows(2).all(|pair| pair[0].seq == pair[1].seq + 1));
}

// The FS inspector walks the in-memory image from guest memory; this guards the
// on-disk offsets, the tree reader, and the file-preview round-trip.
#[test]
fn fs_inspector_reads_tree_and_preview() {
    use full_stack::inspect::fs_view::{self, FsSymbols};

    let kernel = cached_kernel();
    let shell = compile_hosted_program("shell");
    let body = b"hello from the fs inspector\n";
    let image = build_fs_image(&[
        FsEntry::Dir { path: "/home" },
        FsEntry::File {
            path: "/readme.txt",
            data: body,
        },
    ]);

    let mut vm = setup_kernel_vm(kernel, Some(&shell), Some(&image), "");
    let _ = run_until(&mut vm, 80_000_000, |uart| uart.contains("shell ready"));
    // Settle past the prompt so the shell has entered its blocking readchar.
    let _ = vm.run(1_000_000);

    let sym = FsSymbols::from_kernel(kernel).expect("kernel FS symbols resolve");
    let root = fs_view::capture(&vm, &sym).expect("FS should be mounted after boot");

    assert_eq!(root.name, "/", "root node names the root directory");
    assert!(root.is_dir, "root is a directory");

    let home = root
        .children
        .iter()
        .find(|c| c.name == "home")
        .expect("/home should be in the tree");
    assert!(home.is_dir, "/home should be a directory");

    let readme = root
        .children
        .iter()
        .find(|c| c.name == "readme.txt")
        .expect("/readme.txt should be in the tree");
    assert!(!readme.is_dir, "/readme.txt should be a file");
    assert_eq!(
        readme.size as usize,
        body.len(),
        "file size should match the written bytes"
    );

    let preview = fs_view::file_preview(&vm, &sym, readme);
    assert_eq!(preview, body, "preview should round-trip the file contents");
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
