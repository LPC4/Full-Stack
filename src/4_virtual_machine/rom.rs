//! ROM firmware: M-mode trap handler and syscall implementations.
//!
//! At VM startup, `generate_rom_image()` assembles `platform/rom/rom.s` (embedded
//! at compile time via `include_str!`) and loads the resulting bytes into the ROM
//! region (base 0x0000_0000) so the CPU can execute them normally.

use crate::assembly_language::assembler::Assembler;
use crate::assembly_language::rv_instruction::RvInstruction;

/// ROM base address — mtvec points here (direct mode, cause 0 → this address).
pub const ROM_BASE: u64 = 0x0000_0000;

/// Physical address of the trap handler entry point.
pub const TRAP_HANDLER_ENTRY: u64 = ROM_BASE;

/// Syscall numbers (Linux RISC-V ABI + our custom extensions).
pub mod syscall_numbers {
    pub const SYS_WRITE: u64 = 64;
    pub const SYS_EXIT: u64 = 93;
    pub const SYS_EXIT_GROUP: u64 = 94;
    pub const SYS_PUTCHAR: u64 = 1000;
    pub const SYS_PUTS: u64 = 1001;
    pub const SYS_PRINTF: u64 = 1002;
}

static ROM_SOURCE: &str = include_str!("../../platform/rom/rom.s");

/// Parse plain RISC-V assembly text into a `Vec<RvInstruction>`.
///
/// Lines are classified as:
/// - blank or starting with `#` → skipped
/// - ending with `:` → `Label`
/// - starting with `.` → `Directive` (assembler directive)
/// - everything else → `Directive` with a leading tab (instruction)
fn parse_asm_text(src: &str) -> Vec<RvInstruction> {
    src.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if let Some(name) = l.strip_suffix(':') {
                RvInstruction::Label(name.to_string())
            } else if l.starts_with('.') {
                RvInstruction::Directive(l.to_string())
            } else {
                RvInstruction::Directive(format!("\t{l}"))
            }
        })
        .collect()
}

/// Assemble the ROM firmware and return the raw bytes.
///
/// The returned `Vec<u8>` starts at physical address `ROM_BASE` (0x0000_0000).
///
/// # Panics
/// Panics if the assembler rejects the ROM source (indicates a bug in platform/rom/rom.s).
pub fn generate_rom_image() -> Vec<u8> {
    let tokens = parse_asm_text(ROM_SOURCE);
    let output = Assembler::assemble(&tokens)
        .expect("ROM assembly failed, check platform/rom/rom.s");
    output.text_bytes().to_vec()
}
