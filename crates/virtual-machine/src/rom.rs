//! ROM firmware: M-mode trap handler and syscall implementations.
//!
//! At VM startup, `generate_rom_image()` assembles `firmware::ROM_SOURCE`
//! (the M-mode boot stub + trap handler from `crates/firmware/boot/rom.s`) and
//! loads the resulting bytes into the ROM region (base 0x0000_0000).

use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;

/// ROM base address.  `_start` (kernel boot stub) lives here.
pub const ROM_BASE: u64 = 0x0000_0000;

/// Physical address of the M-mode trap handler (`_m_trap`).
///
/// `mtvec` is initialised to this address by `Pipeline::new` so that both
/// hosted programs and the kernel's SBI ecalls reach the right handler.
/// `_start` re-writes the same value before `mret` into S-mode.
pub const M_TRAP_ADDR: u64 = ROM_BASE + 0x100;

/// Physical address of the trap handler entry point (kept for compatibility).
pub const TRAP_HANDLER_ENTRY: u64 = M_TRAP_ADDR;

/// Syscall numbers (Linux RISC-V ABI + our custom extensions).
pub mod syscall_numbers {
    pub const SYS_WRITE: u64 = 64;
    pub const SYS_EXIT: u64 = 93;
    pub const SYS_EXIT_GROUP: u64 = 94;
    pub const SYS_PUTCHAR: u64 = 1000;
    pub const SYS_PUTS: u64 = 1001;
    pub const SYS_PRINTF: u64 = 1002;
}

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
/// Panics if the assembler rejects the ROM source (indicates a bug in `crates/firmware/boot/rom.s`).
pub fn generate_rom_image() -> Vec<u8> {
    let tokens = parse_asm_text(firmware::ROM_SOURCE);
    let output = Assembler::assemble(&tokens).expect("ROM assembly failed — check crates/firmware/boot/rom.s");
    output.text_bytes().to_vec()
}
