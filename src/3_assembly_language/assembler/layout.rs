use super::AssemblerError;
/// Pass 1: walk the typed token stream and compute every label's byte address.
///
/// The layout pass does **not** emit any bytes; it only answers the question
/// "at what byte offset within the output does this label live?"  That information
/// is recorded in a `SymbolTable` which the encode pass uses to fill in
/// branch/jump immediate fields.
use super::section::SectionKind;
use super::symbol_table::SymbolTable;
use super::token::AsmToken;

/// Result of the layout pass.
pub struct Layout {
    pub symbols: SymbolTable,
    /// Sections in the order they first appeared, used to keep the output stable.
    pub section_order: Vec<SectionKind>,
    /// For each section: the total byte size after all tokens are accounted for.
    pub section_sizes: std::collections::HashMap<SectionKind, u64>,
}

/// Walk `tokens`, assign addresses to every `Label`, and return the symbol table.
///
/// Section addresses start at 0 for each section (the encode pass packs them
/// consecutively, so the encode pass also sets each section's `base_address`).
/// We use section-relative addresses here; the encode pass converts them to
/// absolute when building `AssembledOutput`.
pub fn compute_layout(tokens: &[AsmToken]) -> Result<Layout, AssemblerError> {
    let mut symbols = SymbolTable::new();
    let mut section_order: Vec<SectionKind> = Vec::new();
    let mut section_sizes: std::collections::HashMap<SectionKind, u64> =
        std::collections::HashMap::new();

    let mut current = SectionKind::Text;
    let mut offset: u64 = 0; // byte offset within `current`

    for token in tokens {
        match token {
            AsmToken::Section(kind) => {
                // Commit final size for the outgoing section.
                *section_sizes.entry(current.clone()).or_insert(0) = offset;
                // Switch.
                if !section_order.contains(kind) {
                    section_order.push(kind.clone());
                }
                // Restore any previously accumulated offset for this section
                // (sections may be revisited, e.g. .text appears after .data).
                offset = *section_sizes.entry(kind.clone()).or_insert(0);
                current = kind.clone();
            }
            AsmToken::Label(name) => {
                if !section_order.contains(&current) {
                    section_order.push(current.clone());
                }
                if !symbols.define(format!("{}@{}", name, current.name()), offset) {
                    // Section-qualified duplicate — only error on the unqualified form below.
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

    // Commit the final section.
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
