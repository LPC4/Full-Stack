// Integration test for per-process address-space isolation (Phase 1).
//
// Each process now owns its own Sv39 page-table root, so two processes can map
// the same virtual address to different physical pages. This test boots a parent
// as pid 1 that writes a marker to a fixed user VA, then execs a child that
// writes a different marker to the *same* VA. If isolation works, the parent's
// value survives the child scribbling the shared VA: the addresses resolve to
// distinct physical pages.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{
    assembled_to_exec_file, build_fs_image, CompilationPipeline, FsEntry, TargetMode,
};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude,
};
use os_runtime::kernel;
use virtual_machine::virtual_machine::VirtualMachine;

const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

// A fixed user VA inside the lowest stack page (mapped R+W+U in every process,
// well below the live stack frames of a shallow program).
const SHARED_VA: u64 = 0x7FFF_C000;

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

fn build_kernel() -> AssembledOutput {
    let mut stdlib_pipeline = CompilationPipeline::new();
    stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    stdlib_pipeline.set_write_artifacts(false);
    let stdlib = stdlib_pipeline
        .compile(&get_kernel_stdlib_source())
        .expect("kernel stdlib compile");
    let (_, stdlib_tokens) = stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
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
}

#[test]
fn per_process_address_spaces_are_isolated() {
    let kernel_binary = build_kernel();

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

    pid: i64 = sc_exec("/child.bin".data)
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
        path: "/child.bin",
        data: &child_exec,
    }]);

    let mut flat = parent.to_flat_binary();
    let page = 4096usize;
    flat.resize(flat.len().div_ceil(page) * page, 0u8);
    let entry_off = parent.symbol_address("_start").expect("_start missing");
    let entry_va = USER_CODE_VA + entry_off;
    let user_size = flat.len() as u64;

    let mut vm = VirtualMachine::new_kernel(&kernel_binary);

    vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes()).unwrap();
    vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes())
        .unwrap();
    vm.write_ram(FS_IMAGE_PA, &image).unwrap();

    vm.write_ram(USER_META_PA, &entry_va.to_le_bytes()).unwrap();
    vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes())
        .unwrap();
    vm.write_ram(USER_BINARY_PA, &flat).unwrap();

    let run = vm.run(80_000_000);
    let uart = run.uart_output;

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
