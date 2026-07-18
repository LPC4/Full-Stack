//! ROM firmware: M-mode trap handler and syscall implementations.
//!
//! At VM startup, `generate_rom_image()` assembles `os_runtime::ROM_SOURCE`
//! and loads the resulting bytes into the ROM region.

use asm_to_binary::assembler::Assembler;
use asm_to_binary::parse_asm_source;

/// ROM base address. `_start` lives here.
pub const ROM_BASE: u64 = 0x0000_0000;

/// Physical address of the M-mode trap handler (`_m_trap`).
pub const M_TRAP_ADDR: u64 = ROM_BASE + 0x100;

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

/// Assemble the ROM firmware and return the raw bytes; panics if the
/// assembler rejects `os_runtime::ROM_SOURCE`.
pub fn generate_rom_image() -> Vec<u8> {
    let tokens = parse_asm_source(None, os_runtime::ROM_SOURCE);
    let output = Assembler::assemble(&tokens)
        .expect("ROM assembly failed - check crates/os-runtime/boot/rom.s");
    output.text_bytes().to_vec()
}
