// Integration test for the file editor (Phase 3).
//
// Boots the shell as pid 1 with the line editor at /bin/edit and a pre-populated
// /notes.txt. A scripted UART session runs `edit /notes.txt`, clears the loaded
// contents, appends two fresh lines, writes, and quits; then `cat /notes.txt`
// echoes the result. This exercises:
//   - exec argument passing (the editor learns the path from USER_ARG_BASE),
//   - the user-space file syscalls (open/read/write/close),
//   - fs_truncate: /notes.txt starts larger than the new contents, so a correct
//     truncate leaves none of the old filler behind and shrinks the inode size.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{
    assembled_to_exec_file, build_fs_image, CompilationPipeline, FsEntry, TargetMode,
};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude,
};
use os_runtime::{kernel, user};
use virtual_machine::virtual_machine::VirtualMachine;

const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

// Old contents of /notes.txt: distinctive filler, longer than the new contents,
// so a missing truncate would leave some of it behind.
const OLD_CONTENTS: &[u8] = b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";

// New contents the editor writes (two lines, each terminated by '\n').
const NEW_LINE_1: &str = "hello world";
const NEW_LINE_2: &str = "second line";

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
    let kernel_binary = build_kernel();

    let shell = compile_hosted(user::SHELL);
    let editor = compile_hosted(user::EDIT);
    let editor_exec = assembled_to_exec_file(&editor);

    // /bin/edit holds the editor; /notes.txt starts with longer filler so the
    // editor's truncate-on-save is observable.
    let image = build_fs_image(&[
        FsEntry::Dir { path: "/bin" },
        FsEntry::File {
            path: "/bin/edit",
            data: &editor_exec,
        },
        FsEntry::File {
            path: "/notes.txt",
            data: OLD_CONTENTS,
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

    // Drive the session: open the editor on /notes.txt, clear the loaded filler,
    // append two lines, write (truncating), quit; then cat the file and exit.
    let session = format!(
        "edit /notes.txt\nc\na\n{NEW_LINE_1}\n{NEW_LINE_2}\n.\nw\nq\ncat /notes.txt\nexit\n"
    );
    for b in session.bytes() {
        vm.push_uart_rx(b);
    }

    let run = vm.run(200_000_000);
    let uart = run.uart_output;

    assert!(!uart.contains("PANIC!"), "kernel panicked; uart={uart:?}");
    assert!(
        uart.contains("edit: a=append"),
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
        after_write.contains(NEW_LINE_1) && after_write.contains(NEW_LINE_2),
        "cat did not show the new contents; uart={uart:?}"
    );
    assert!(
        !after_write.contains("ZZZ"),
        "old filler survived (truncate failed); uart={uart:?}"
    );

    // The on-disk inode size must equal the new contents exactly (truncate).
    let final_image = vm.peek_bytes_raw(FS_IMAGE_PA, image.len());
    let expected = (NEW_LINE_1.len() + 1 + NEW_LINE_2.len() + 1) as u32;
    let size = inode_size_of(&final_image, "notes.txt").expect("notes.txt inode");
    assert_eq!(
        size, expected,
        "inode size not truncated to new contents; got {size}, want {expected}"
    );
}
