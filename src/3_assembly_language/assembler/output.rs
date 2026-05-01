use std::collections::HashMap;
use super::section::{SectionData, SectionKind};

/// The final output produced by the assembler — one byte blob per section,
/// plus a complete symbol table ready to hand to a linker or ELF writer.
#[derive(Debug, Default)]
pub struct AssembledOutput {
    /// Sections in emission order, keyed by kind.
    pub sections: Vec<SectionData>,
    /// All resolved labels: name → absolute address within the output blob.
    pub symbol_table: HashMap<String, u64>,
    /// Names marked `.globl` (exported).
    pub global_symbols: Vec<String>,
}

impl AssembledOutput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a section by kind, returning its byte content (or an empty slice).
    pub fn section_bytes(&self, kind: &SectionKind) -> &[u8] {
        self.sections
            .iter()
            .find(|s| s.kind.as_ref() == Some(kind))
            .map(|s| s.bytes.as_slice())
            .unwrap_or(&[])
    }

    pub fn text_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::Text)
    }

    pub fn data_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::Data)
    }

    pub fn rodata_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::RoData)
    }

    /// Total encoded size across all sections.
    pub fn total_bytes(&self) -> usize {
        self.sections.iter().map(|s| s.bytes.len()).sum()
    }
}

impl std::fmt::Display for AssembledOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for section in &self.sections {
            if let Some(kind) = &section.kind {
                writeln!(f, "{} ({} bytes):", kind.name(), section.bytes.len())?;
                for chunk in section.bytes.chunks(16) {
                    write!(f, "  ")?;
                    for b in chunk {
                        write!(f, "{b:02x} ")?;
                    }
                    writeln!(f)?;
                }
            }
        }
        writeln!(f, "Symbols ({}):", self.symbol_table.len())?;
        let mut sorted: Vec<_> = self.symbol_table.iter().collect();
        sorted.sort_by_key(|&(_, addr)| addr);
        for (name, addr) in sorted {
            let marker = if self.global_symbols.contains(name) { " [global]" } else { "" };
            writeln!(f, "  {addr:#010x}  {name}{marker}")?;
        }
        Ok(())
    }
}
