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
    assembled_to_exec_file, build_fs_image, CompilationPipeline, FsEntry, TargetMode,
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

// ---------------------------------------------------------------------------
// Cached kernel binaries
// ---------------------------------------------------------------------------

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
                &[("kernel_stdlib", &stdlib_obj), ("my_kernel", &kernel_objs[0])],
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

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

// Compile a hosted user program (links the hosted stdlib).
fn compile_hosted(src: &str) -> AssembledOutput {
    let full = format!("{}\n{}", get_stdlib_source_for_mode(TargetMode::Hosted), src);
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
fn boot_kernel(
    kernel: &AssembledOutput,
    user: Option<&AssembledOutput>,
    fs_image: Option<&[u8]>,
    input: &str,
    max_steps: u64,
) -> (VirtualMachine, StepOutcome, String) {
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
        vm.write_ram(USER_BINARY_PA, &flat).expect("write user binary");
    }

    for b in input.bytes() {
        vm.push_uart_rx(b);
    }

    let run = vm.run(max_steps);
    (vm, run.outcome, run.uart_output)
}

// A hosted user program injected as pid 1 must spawn and exit with code 0.
fn assert_user_exit_ok(uart: &str, outcome: &StepOutcome, label: &str) {
    if let StepOutcome::Halted(c) = outcome
        && *c != 0
    {
        panic!("{label}: unexpected VM halt with code {c}; uart={uart:?}");
    }
    assert!(!uart.contains("PANIC!"), "{label}: kernel panicked; uart={uart:?}");
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

// Compile a hosted program and inject it as pid 1 (no FS image), then assert a
// clean spawn-and-exit.
fn run_example_in_kernel(user_src: &str, label: &str) {
    let user = compile_hosted(user_src);
    let (_, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 10_000_000);
    assert_user_exit_ok(&uart, &outcome, label);
}

// ===========================================================================
// Boot sequence (merged from kernel_boot_device_tree.rs + kernel_user_injection.rs)
//
// All of these formerly recompiled the kernel and asserted on different parts of
// the same boot. One boot now covers the whole sequence.
// ===========================================================================

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
    assert!(uart.contains("[  OK  ] heap ready\n"), "expected heap smoke-test; uart={uart:?}");
    assert!(uart.contains("[  OK  ] timer armed\n"), "expected timer armed; uart={uart:?}");
    assert!(uart.contains("[  OK  ] boot complete\n"), "expected boot complete; uart={uart:?}");

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
    let (_, outcome, uart) =
        boot_kernel(cached_kernel_multi_module(), Some(&user), None, "", 10_000_000);
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
    let (_, outcome, uart) =
        boot_kernel(cached_kernel_multi_module(), None, None, "", 10_000_000);
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

// ===========================================================================
// Example programs in kernel userspace (merged from kernel_example_injection.rs)
// ===========================================================================

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

// ===========================================================================
// Framebuffer device (map_fb syscall + fbdemo program)
// ===========================================================================

// Boot fbdemo as pid 1, read the framebuffer back, and check the rendered image.
// Covers the whole path: map_fb -> MMU translation -> bus routing -> device store.
#[test]
fn fbdemo_renders_mandelbrot_set() {
    const FB_W: usize = 320;
    const FB_H: usize = 240;
    let user = compile_hosted(user::FBDEMO);
    let (vm, outcome, uart) = boot_kernel(cached_kernel(), Some(&user), None, "", 2_000_000_000);
    assert_user_exit_ok(&uart, &outcome, "fbdemo");
    assert!(
        uart.contains("fbdemo: mandelbrot rendered"),
        "fbdemo did not report success; uart={uart:?}"
    );

    let px = vm.peek_framebuffer();
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let o = (y * FB_W + x) * 4;
        [px[o], px[o + 1], px[o + 2], px[o + 3]]
    };

    // Every pixel must be fully opaque (the alpha byte is always written).
    for (i, p) in px.chunks_exact(4).enumerate() {
        assert_eq!(p[3], 255, "pixel {i} not opaque: {p:?}");
    }

    // Image centre maps to c ~ (-0.75, 0), deep inside the set -> black.
    assert_eq!(pixel(FB_W / 2, FB_H / 2), [0, 0, 0, 255], "set interior should be black");

    // Far left on the real axis escapes immediately, so it is coloured.
    let left = pixel(40, FB_H / 2);
    assert!(
        left[0] != 0 || left[1] != 0 || left[2] != 0,
        "escaping pixel should be coloured, got {left:?}"
    );

    // A large coloured region confirms the fractal rendered, not a stray pixel.
    let coloured = px
        .chunks_exact(4)
        .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
        .count();
    assert!(
        coloured > 10_000,
        "expected a substantial coloured region, got {coloured} pixels"
    );
}

// ===========================================================================
// Interactive shell (merged from kernel_shell.rs)
// ===========================================================================

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
    let exec_file = assembled_to_exec_file(&child);

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/home" },
        FsEntry::File {
            path: "/home/hello.fexe",
            data: &exec_file,
        },
    ]);

    let session = "ls\ncd /home\nls\nrun hello.fexe\nexit\n";
    let (_, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&shell), Some(&image), session, 80_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(uart.contains("HLL shell ready"), "shell did not start; uart={uart:?}");
    assert!(uart.contains("home"), "ls of root did not list home; uart={uart:?}");
    assert!(
        uart.contains("hello.fexe"),
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

// `run` on a non-FEXE file must report "not an executable" up front rather than
// failing opaquely inside sys_exec.
#[test]
fn kernel_shell_run_rejects_non_executable() {
    let shell = compile_hosted(user::SHELL);
    let image = build_fs_image(&[FsEntry::File {
        path: "/notes.txt",
        data: b"just some text, not a program\n",
    }]);

    let session = "run /notes.txt\nrun /missing.fexe\nexit\n";
    let (_, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&shell), Some(&image), session, 80_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("run: not an executable: /notes.txt"),
        "run did not reject a non-FEXE file; uart={uart:?}"
    );
    assert!(
        uart.contains("run: no such file: /missing.fexe"),
        "run did not report a missing file; uart={uart:?}"
    );
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt cleanly; outcome={outcome:?} uart={uart:?}"
    );
}

// ===========================================================================
// File editor (merged from kernel_editor.rs)
// ===========================================================================

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
    let editor_exec = assembled_to_exec_file(&editor);

    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/edit.fexe",
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
    let (vm, _outcome, uart) =
        boot_kernel(cached_kernel(), Some(&shell), Some(&image), &session, 200_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(uart.contains("edit: a=append"), "editor did not start; uart={uart:?}");
    assert!(uart.contains("edit: written"), "editor did not write the file; uart={uart:?}");

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

// ===========================================================================
// fork / wait (merged from kernel_fork.rs)
// ===========================================================================

#[test]
fn fork_child_runs_and_parent_reaps_exit_code() {
    // pid 1: fork. The child (a0==0) writes a marker and exits with 42; the
    // parent waits and confirms the reaped code is exactly 42.
    let parent = compile_hosted(
        r#"
external console_writeln: (str: u8*)

_a0:  u64 = 0
_ret: i64 = 0

sc_fork: () -> i64 {
    asm {
        li a7, 220
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_wait: () -> i64 {
    asm {
        li a7, 260
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_exit: (code: i64) {
    _a0 = code as u64
    asm {
        la t0, _a0
        ld a0, 0(t0)
        li a7, 93
        ecall
    }
}

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
    assert!(uart.contains("CHILD_RAN"), "child did not run; uart={uart:?}");
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
    assert!(child_at < reaped_at, "parent reaped before the child ran; uart={uart:?}");
}

// ===========================================================================
// Per-process address-space isolation (merged from kernel_isolation.rs)
// ===========================================================================

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

_a0:  u64 = 0
_ret: i64 = 0

sc_exec: (path: u8*) -> i64 {{
    _a0 = path as u64
    asm {{
        la t0, _a0
        ld a0, 0(t0)
        li a1, 0
        li a7, 103
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }}
    return _ret
}}

sc_pidalive: (pid: u64) -> i64 {{
    _a0 = pid
    asm {{
        la t0, _a0
        ld a0, 0(t0)
        li a7, 104
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }}
    return _ret
}}

sc_yield: () {{
    asm {{ li a7, 2  ecall }}
}}

sc_exit: (code: i64) {{
    _a0 = code as u64
    asm {{
        la t0, _a0
        ld a0, 0(t0)
        li a7, 93
        ecall
    }}
}}

main: () -> i32 {{
    p: u8* = {shared} as u8*
    @p = 0xAA as u8
    console_writeln("PARENT_WROTE".data)

    pid: i64 = sc_exec("/child.fexe".data)
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
    let child_exec = assembled_to_exec_file(&child);

    let image = build_fs_image(&[FsEntry::File {
        path: "/child.fexe",
        data: &child_exec,
    }]);

    let (_, _outcome, uart) =
        boot_kernel(cached_kernel(), Some(&parent), Some(&image), "", 80_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(uart.contains("PARENT_WROTE"), "parent did not start; uart={uart:?}");
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

// ===========================================================================
// Filesystem (merged from kernel_fs.rs)
// ===========================================================================

// User program that exercises the full FS API and prints FS_ALL_PASS on success.
const FS_EXERCISER: &str = r#"
_a0: u64 = 0
_a1: u64 = 0
_a2: u64 = 0
_a3: u64 = 0
_ret: i64 = 0

sc_open: (path: u64, flags: u64) -> i64 {
    _a0 = path
    _a1 = flags
    asm {
        la t0, _a0
        ld a0, 0(t0)
        la t0, _a1
        ld a1, 0(t0)
        li a7, 56
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_close: (fd: i64) {
    _a0 = fd as u64
    asm {
        la t0, _a0
        ld a0, 0(t0)
        li a7, 57
        ecall
    }
}

sc_read: (fd: i64, buf: u64, offset: u64, len: u64) -> i64 {
    _a0 = fd as u64
    _a1 = buf
    _a2 = offset
    _a3 = len
    asm {
        la t0, _a0
        ld a0, 0(t0)
        la t0, _a1
        ld a1, 0(t0)
        la t0, _a2
        ld a2, 0(t0)
        la t0, _a3
        ld a3, 0(t0)
        li a7, 63
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_write: (fd: i64, buf: u64, len: u64) -> i64 {
    _a0 = fd as u64
    _a1 = buf
    _a2 = len
    asm {
        la t0, _a0
        ld a0, 0(t0)
        la t0, _a1
        ld a1, 0(t0)
        la t0, _a2
        ld a2, 0(t0)
        li a7, 64
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_mkdir: (path: u64) -> i64 {
    _a0 = path
    asm {
        la t0, _a0
        ld a0, 0(t0)
        li a7, 83
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

sc_rename: (oldp: u64, newp: u64) -> i64 {
    _a0 = oldp
    _a1 = newp
    asm {
        la t0, _a0
        ld a0, 0(t0)
        la t0, _a1
        ld a1, 0(t0)
        li a7, 82
        ecall
        la t0, _ret
        sd a0, 0(t0)
    }
    return _ret
}

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
    buf_addr: u64 = u64(buf[0])

    ; 1. Read a pre-populated file.
    fd: i64 = sc_open(u64("/hello.txt".data), 0)
    if fd < 2 { return fail("open hello".data) }
    n: i64 = sc_read(fd, buf_addr, 0, 13)
    if n != 13 { return fail("read hello count".data) }
    if bytes_eq(buf[0], "hello from fs".data, 13) == 0 { return fail("read hello data".data) }
    sc_close(fd)

    ; 2. Create a directory.
    if sc_mkdir(u64("/out".data)) < 0 { return fail("mkdir out".data) }

    ; 3. Create a file in it and write to it.
    wfd: i64 = sc_open(u64("/out/result.txt".data), 2)
    if wfd < 2 { return fail("create result".data) }
    w: i64 = sc_write(wfd, u64("data123".data), 7)
    if w != 7 { return fail("write count".data) }
    sc_close(wfd)

    ; 4. Re-open and read back the written contents.
    rfd: i64 = sc_open(u64("/out/result.txt".data), 0)
    if rfd < 2 { return fail("reopen result".data) }
    rn: i64 = sc_read(rfd, buf_addr, 0, 7)
    if rn != 7 { return fail("readback count".data) }
    if bytes_eq(buf[0], "data123".data, 7) == 0 { return fail("readback data".data) }
    sc_close(rfd)

    ; 5. Rename within the directory; old name must disappear.
    if sc_rename(u64("/out/result.txt".data), u64("/out/final.txt".data)) != 0 {
        return fail("rename".data)
    }
    ffd: i64 = sc_open(u64("/out/final.txt".data), 0)
    if ffd < 2 { return fail("open final".data) }
    sc_close(ffd)
    gone: i64 = sc_open(u64("/out/result.txt".data), 0)
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
    assert!(uart.contains("boot complete"), "expected boot to complete; uart={uart:?}");
    assert!(uart.contains("[  FS  ] mounted"), "expected FS mount message; uart={uart:?}");
    assert!(!uart.contains("FS_FAIL"), "an FS operation failed; uart={uart:?}");
    assert!(uart.contains("FS_ALL_PASS"), "FS exerciser did not report success; uart={uart:?}");
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

    let (vm, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&shell), Some(&image), session, 120_000_000);

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(uart.contains("HLL shell ready"), "shell did not start; uart={uart:?}");
    assert!(
        matches!(outcome, StepOutcome::Halted(0)),
        "exit did not halt the VM cleanly; outcome={outcome:?} uart={uart:?}"
    );

    // No command should have reported a failure.
    assert!(!uart.contains("cannot create"), "a create command failed; uart={uart:?}");
    assert!(!uart.contains("cannot remove"), "rm failed; uart={uart:?}");
    assert!(!uart.contains("cannot move"), "mv failed; uart={uart:?}");

    let final_image = vm.peek_bytes_raw(FS_IMAGE_PA, image.len());
    assert!(inode_present(&final_image, "work", 2), "mkdir did not create /work");
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

    let (vm, outcome, uart) =
        boot_kernel(cached_kernel(), Some(&shell), Some(&image), session, 120_000_000);

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
    assert!(inode_present(&final_image, "full", 2), "non-empty dir was wrongly removed");
    assert!(!inode_present(&final_image, "empty", 2), "empty dir was not removed");
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
