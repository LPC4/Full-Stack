/// Pass 1: walk typed tokens and compute every label's byte address (section-relative).
/// No bytes are emitted; the symbol table is used by the encode pass.
use super::AssemblerError;
use super::section::SectionKind;
use super::symbol_table::SymbolTable;
use super::token::AsmToken;

/// Result of the layout pass.
pub struct Layout {
    pub symbols: SymbolTable,
    pub section_order: Vec<SectionKind>,
    pub section_sizes: std::collections::HashMap<SectionKind, u64>,
}

/// Walk tokens, assign addresses to labels, return the symbol table.
pub fn compute_layout(tokens: &[AsmToken]) -> Result<Layout, AssemblerError> {
    let mut symbols = SymbolTable::new();
    let mut section_order: Vec<SectionKind> = Vec::new();
    let mut section_sizes: std::collections::HashMap<SectionKind, u64> =
        std::collections::HashMap::new();

    let mut current = SectionKind::Text;
    let mut offset: u64 = 0;

    for token in tokens {
        match token {
            AsmToken::Section(kind) => {
                *section_sizes.entry(current.clone()).or_insert(0) = offset;
                if !section_order.contains(kind) {
                    section_order.push(kind.clone());
                }
                offset = *section_sizes.entry(kind.clone()).or_insert(0);
                current = kind.clone();
            }
            AsmToken::Label(name) => {
                if !section_order.contains(&current) {
                    section_order.push(current.clone());
                }
                if !symbols.define(format!("{}@{}", name, current.name()), offset) {
                    // Section-qualified duplicate -- only error on the unqualified form below.
                }
                if !symbols.define(name.clone(), offset) {
                    return Err(AssemblerError::new(format!(
                        "duplicate label `{name}` in section `{}`",
                        current.name()
                    )));
                }
            }
            AsmToken::Globl(name) => {
                symbols.mark_global(name.clone());
            }
            AsmToken::Align(n) => {
                offset = align_up(offset, 1u64 << n);
            }
            AsmToken::Balign(n) => {
                offset = align_up(offset, *n as u64);
            }
            other => {
                if let Some(size) = other.fixed_size() {
                    if !section_order.contains(&current) {
                        section_order.push(current.clone());
                    }
                    offset += size as u64;
                }
            }
        }
    }

    *section_sizes.entry(current).or_insert(0) = offset;

    Ok(Layout {
        symbols,
        section_order,
        section_sizes,
    })
}

pub fn align_up(offset: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    (offset + alignment - 1) & !(alignment - 1)
}
