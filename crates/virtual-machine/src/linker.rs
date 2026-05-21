//! Simple single-image linker.
//!
//! Takes an `AssembledOutput` (sections at address 0), relocates everything
//! to a configurable `text_base`, and returns a `LinkedProgram` ready for the
//! VM loader.  The design is intentionally minimal but structured so that
//! future features (multiple object files, weak symbols, section attributes)
//! can be added without a full rewrite.

use super::bus::RAM_BASE;
use asm_to_binary::AssembledOutput;
use std::collections::HashMap;

/// Linker configuration.
pub struct LinkerConfig {
    /// Where the first section (.text) is loaded in the target address space.
    /// Defaults to `RAM_BASE` (0x8000_0000).
    pub text_base: u64,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            text_base: RAM_BASE,
        }
    }
}

/// The output of the linker: a flat image with relocated metadata.
pub struct LinkedProgram {
    /// Flat byte image to be loaded at `load_addr`.
    pub bytes: Vec<u8>,
    /// Physical address where `bytes[0]` should be placed.
    pub load_addr: u64,
    /// Absolute entry point (PC to start execution at).
    pub entry_point: u64,
    /// All symbols with their final absolute addresses.
    pub symbols: HashMap<String, u64>,
    /// First address available for the heap allocator.
    pub heap_base: u64,
    /// Total size of the loaded image in bytes.
    pub image_size: u64,
}

const STANDARD_SECTIONS: &[&str] = &[".text", ".rodata", ".data", ".bss"];

/// Link an assembled output into a `LinkedProgram` at the given base address.
pub fn link(assembled: &AssembledOutput, config: &LinkerConfig) -> LinkedProgram {
    let base = config.text_base;
    let mut bytes: Vec<u8> = Vec::new();

    // Emit standard sections first in canonical order.
    for name in STANDARD_SECTIONS {
        bytes.extend_from_slice(match *name {
            ".text" => assembled.text_bytes(),
            ".rodata" => assembled.rodata_bytes(),
            ".data" => assembled.data_bytes(),
            ".bss" => assembled.bss_bytes(),
            _ => &[],
        });
    }

    // Append any custom (non-standard) sections.
    for info in assembled.sections_iter() {
        if !STANDARD_SECTIONS.contains(&info.name) {
            bytes.extend_from_slice(info.bytes);
        }
    }

    let image_size = bytes.len() as u64;

    let symbols: HashMap<String, u64> = assembled
        .symbols_iter()
        .map(|(name, offset)| (name.to_owned(), base + offset))
        .collect();

    let entry_point = symbols
        .get("_start")
        .or_else(|| symbols.get("main"))
        .copied()
        .unwrap_or(base);

    let heap_base = base + ((image_size + 0xFFF) & !0xFFF);

    LinkedProgram {
        bytes,
        load_addr: base,
        entry_point,
        symbols,
        heap_base,
        image_size,
    }
}
