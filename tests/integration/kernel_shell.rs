// Integration test for the interactive shell (user/shell.hll).
//
// Boots the kernel with the shell as pid 1 and a filesystem image that contains
// a /home directory holding an executable file. UART input is pre-loaded to
// drive a session: list the root, cd into /home, list it, run the program, and
// exit. The test asserts the shell echoes the expected output and that exiting
// halts the VM cleanly.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{
    assembled_to_exec_file, build_fs_image, CompilationPipeline, FsEntry, TargetMode,
};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude,
};
use os_runtime::{kernel, user};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

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

// Build the kernel image (stdlib + my_kernel) linked at object level.
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
fn kernel_shell_ls_cd_run_exit() {
    let kernel_binary = build_kernel();

    // The shell is the pid-1 user process.
    let shell = compile_hosted(user::SHELL);

    // A child program the shell can run via the filesystem.
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

    // Filesystem: /home directory with the executable inside.
    let image = build_fs_image(&[
        FsEntry::Dir { path: "/home" },
        FsEntry::File {
            path: "/home/hello.bin",
            data: &exec_file,
        },
    ]);

    let mut flat = shell.to_flat_binary();
    let page = 4096usize;
    flat.resize(flat.len().div_ceil(page) * page, 0u8);
    let entry_off = shell.symbol_address("_start").expect("_start missing");
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

    // Pre-load the interactive session into the UART receive buffer.
    let session = "ls\ncd /home\nls\nrun hello.bin\nexit\n";
    for b in session.bytes() {
        vm.push_uart_rx(b);
    }

    let run = vm.run(80_000_000);
    let uart = run.uart_output;

    assert!(
        !uart.contains("PANIC!"),
        "kernel panicked; uart={uart:?}"
    );
    assert!(
        uart.contains("HLL shell ready"),
        "shell did not start; uart={uart:?}"
    );
    assert!(
        uart.contains("home"),
        "ls of root did not list home; uart={uart:?}"
    );
    assert!(
        uart.contains("hello.bin"),
        "ls of /home did not list the program; uart={uart:?}"
    );
    assert!(
        uart.contains("HELLO_FROM_CHILD"),
        "run did not execute the program; uart={uart:?}"
    );
    assert!(
        matches!(run.outcome, StepOutcome::Halted(0)),
        "exit did not halt the VM cleanly; outcome={:?} uart={uart:?}",
        run.outcome
    );
}
