use super::AssembledOutput;

// --- Executable file format (static ELF64) ---

/// Virtual base the kernel maps user code at; ELF binaries link `e_entry`/`p_vaddr`
/// here. Matches `USER_CODE_BASE`/`USER_CODE_VA` in the kernel sources.
pub const USER_CODE_BASE: u64 = 0x4000_0000;

/// Build a static ELF64 executable, linked at [`USER_CODE_BASE`] with one `PT_LOAD`
/// segment carrying the flat image, so the kernel's `sys_exec` can load it.
pub fn assembled_to_elf_file(assembled: &AssembledOutput) -> Vec<u8> {
    assembled.to_elf(USER_CODE_BASE)
}
