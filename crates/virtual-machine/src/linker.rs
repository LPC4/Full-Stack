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
    pub text_base: u64,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            text_base: RAM_BASE,
        }
    }
}

/// Flat linked image with relocated metadata.
pub struct LinkedProgram {
    pub bytes: Vec<u8>,
    pub load_addr: u64,
    pub entry_point: u64,
    pub symbols: HashMap<String, u64>,
    pub heap_base: u64,
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
