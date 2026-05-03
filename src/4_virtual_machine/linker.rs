//! Simple single-image linker.
//!
//! Takes an `AssembledOutput` (sections at address 0), relocates everything
//! to a configurable `text_base`, and returns a `LinkedProgram` ready for the
//! VM loader.  The design is intentionally minimal but structured so that
//! future features (multiple object files, weak symbols, section attributes)
//! can be added without a full rewrite.

use super::bus::RAM_BASE;
use crate::assembly_language::assembler::output::AssembledOutput;
use crate::assembly_language::assembler::section::SectionKind;
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

/// Canonical section load order: text -> rodata -> data -> bss -> custom.
fn section_load_order() -> &'static [SectionKind] {
    &[
        SectionKind::Text,
        SectionKind::RoData,
        SectionKind::Data,
        SectionKind::Bss,
    ]
}

/// Link an assembled output into a `LinkedProgram` at the given base address.
pub fn link(assembled: &AssembledOutput, config: &LinkerConfig) -> LinkedProgram {
    let base = config.text_base;
    let mut bytes: Vec<u8> = Vec::new();

    for kind in section_load_order() {
        bytes.extend_from_slice(assembled.section_bytes(kind));
    }

    for section in &assembled.sections {
        if let Some(kind) = &section.kind {
            if !section_load_order().contains(kind) {
                bytes.extend_from_slice(&section.bytes);
            }
        }
    }

    let image_size = bytes.len() as u64;

    let symbols: HashMap<String, u64> = assembled
        .symbol_table
        .iter()
        .map(|(name, &offset)| (name.clone(), base + offset))
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
