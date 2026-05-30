// Integration test for fork/wait (Phase 2).
//
// A single program is injected as pid 1. It calls fork (syscall 220): the child
// returns 0, writes a UART marker, and exits with a known code (42). The parent
// receives the child pid, calls wait (syscall 260), and confirms it reaps that
// exact exit code. fork copies the parent's address space, so the child runs the
// same binary without needing a separate executable in the filesystem.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{build_fs_image, CompilationPipeline, TargetMode};
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
fn fork_child_runs_and_parent_reaps_exit_code() {
    let kernel_binary = build_kernel();

    // pid 1: fork. The child (a0==0) writes a marker and exits with 42; the
    // parent waits and confirms the reaped code is exactly 42.
    let src = r#"
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
"#;

    let parent = compile_hosted(src);

    // fork needs no child binary in the FS, but the kernel still expects a valid
    // (possibly empty) FS image at the agreed physical address.
    let image = build_fs_image(&[]);

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
