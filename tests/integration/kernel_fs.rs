// Integration tests for the kernel filesystem (fs.hll).
//
// The filesystem runs entirely in the kernel (HLL). Exercising it requires a
// booted VM, so to keep the suite fast we boot the VM ONCE and drive every FS
// operation (open/read/write/create/mkdir/rename/close, nested directories)
// from a single user program that prints `FS_ALL_PASS` on success. A second,
// minimal boot verifies the kernel comes up cleanly with no FS image. The
// remaining tests are pure layout/symbol checks that need no VM.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{build_fs_image, CompilationPipeline, FsEntry, TargetMode};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude,
};
use os_runtime::kernel;
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Physical layout constants matching the spec.
const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

// --- Build helpers ---

fn compile_user_binary(user_src: &str) -> AssembledOutput {
    let full_source = format!(
        "{}\n{}",
        get_stdlib_source_for_mode(TargetMode::Hosted),
        user_src
    );
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_target_mode(TargetMode::Hosted);
    pipeline.set_write_artifacts(false);
    pipeline.set_type_prelude(get_stdlib_type_prelude());
    let result = pipeline.compile(&full_source).expect("user compile");
    let (_, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    pipeline.assemble(&tokens).expect("user assemble")
}

// Kernel binary, compiled once per test run.
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

// Boot a VM with an optional FS image and a user program; return UART + outcome.
fn run_with_fs(fs_image: Option<&[u8]>, user_src: &str) -> (String, StepOutcome) {
    let kernel_binary = get_kernel_binary();

    let user_assembled = compile_user_binary(user_src);
    let mut flat = user_assembled.to_flat_binary();
    let page_size = 4096usize;
    let padded = flat.len().div_ceil(page_size) * page_size;
    flat.resize(padded, 0u8);

    let entry_off = user_assembled
        .symbol_address("_start")
        .expect("_start symbol missing");
    let entry_va = USER_CODE_VA + entry_off;
    let user_size = flat.len() as u64;

    let mut vm = VirtualMachine::new_kernel(kernel_binary);

    if let Some(image) = fs_image {
        vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes())
            .expect("write fs image PA");
        vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes())
            .expect("write fs image size");
        vm.write_ram(FS_IMAGE_PA, image).expect("write fs image");
    }

    vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
        .expect("write user entry VA");
    vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
        .expect("write user size");
    vm.write_ram(USER_BINARY_PA, &flat).expect("write user binary");

    let run = vm.run(50_000_000);
    (run.uart_output, run.outcome)
}

fn assert_clean_boot(uart: &str, outcome: &StepOutcome, label: &str) {
    // The last user process exiting now halts the VM via SYSCON; a zero exit
    // code is the expected clean shutdown. A non-zero halt is a real failure.
    if let StepOutcome::Halted(c) = outcome
        && *c != 0
    {
        panic!("{label}: unexpected VM halt with code {c}; uart={uart:?}");
    }
    assert!(!uart.contains("PANIC!"), "{label}: kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("[ PROC ] pid 1 ready"),
        "{label}: user process not spawned; uart={uart:?}"
    );
    assert!(
        uart.contains("sys_exit code: 0"),
        "{label}: user did not exit cleanly; uart={uart:?}"
    );
}

// User program that exercises the full FS API and prints FS_ALL_PASS on success.
// Each syscall is wrapped in a tiny helper that marshals arguments through
// globals (inline asm can only reach globals via `la`).
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

// --- Tests ---

// One boot exercises the full FS API: mount, read, mkdir, create, write,
// read-back, rename, and close, plus a clean kernel boot.
#[test]
fn kernel_fs_full_lifecycle() {
    let image = build_fs_image(&[FsEntry::File {
        path: "/hello.txt",
        data: b"hello from fs\n",
    }]);

    let (uart, outcome) = run_with_fs(Some(&image), FS_EXERCISER);
    assert_clean_boot(&uart, &outcome, "fs_full_lifecycle");
    assert!(
        uart.contains("boot complete"),
        "expected boot to complete; uart={uart:?}"
    );
    assert!(
        uart.contains("[ FS ] mounted"),
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

// The kernel boots cleanly with no FS image present (FS_META_PA stays zero) and
// does not attempt to mount a filesystem.
#[test]
fn kernel_boots_without_fs_image() {
    let src = r#"
main: () -> i32 {
    return 0
}
"#;
    let (uart, outcome) = run_with_fs(None, src);
    assert_clean_boot(&uart, &outcome, "no_fs_image");
    assert!(
        !uart.contains("[ FS ] mounted"),
        "FS should not mount when no image; uart={uart:?}"
    );
}

// --- Layout / symbol address checks (no VM needed) ---

// Every user global referenced via inline asm must live inside the flat binary
// that gets mapped into the VM; a global past the last mapped page would fault.
#[test]
fn kernel_fs_user_globals_in_mapped_range() {
    let assembled = compile_user_binary(FS_EXERCISER);
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

// The path string literals the exerciser passes to the kernel must be within
// the mapped user range.
#[test]
fn kernel_fs_path_literals_in_mapped_range() {
    let assembled = compile_user_binary(FS_EXERCISER);
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
