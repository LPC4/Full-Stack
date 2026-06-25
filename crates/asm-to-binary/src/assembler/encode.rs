/// Pass 2: encode typed tokens to bytes, resolving label references via the symbol table.
/// See `_RISCV_SPECIFICATIONS.md` for pseudo-instruction expansion rules.
use super::AssemblerError;
use super::layout::Layout;
use super::output::{AssembledOutput, RelocationKind, RelocationRecord};
use super::section::{SectionData, SectionKind};
use super::token::{AsmToken, BranchKind};
use crate::real::RealInstruction;
use crate::riscv::rv64i::{Addi, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne, Jal as JalInst, Jalr};
use crate::traits::Instruction as _;

/// Encode all tokens into an `AssembledOutput` using the layout symbol table.
pub fn encode(tokens: &[AsmToken], layout: &Layout) -> Result<AssembledOutput, AssemblerError> {
    let mut out = AssembledOutput::new();
    let mut sections: std::collections::HashMap<SectionKind, SectionData> = layout
        .section_order
        .iter()
        .map(|k| (k.clone(), SectionData::new(k.clone())))
        .collect();

    // Non-BSS sections first, BSS last to keep ELF virtual addresses contiguous.
    let mut section_bases: std::collections::HashMap<SectionKind, u64> =
        std::collections::HashMap::new();
    let mut running_base: u64 = 0;
    for kind in layout
        .section_order
        .iter()
        .filter(|k| !matches!(k, SectionKind::Bss))
    {
        section_bases.insert(kind.clone(), running_base);
        running_base += layout.section_sizes.get(kind).copied().unwrap_or(0);
    }
    for kind in layout
        .section_order
        .iter()
        .filter(|k| matches!(k, SectionKind::Bss))
    {
        section_bases.insert(kind.clone(), running_base);
        running_base += layout.section_sizes.get(kind).copied().unwrap_or(0);
    }

    let mut current_kind = SectionKind::Text;
    let mut current_addr: u64 = section_bases.get(&current_kind).copied().unwrap_or(0);

    for token in tokens {
        match token {
            AsmToken::Section(kind) => {
                current_kind = kind.clone();
                let base = section_bases.get(kind).copied().unwrap_or(0);
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                current_addr = base + sec.current_offset();
            }
            AsmToken::Label(name) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.define_label(name.clone());
            }
            AsmToken::Globl(name) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.export_global(name.clone());
                if !out.global_symbols.contains(name) {
                    out.global_symbols.push(name.clone());
                }
            }
            AsmToken::Real(inst) => {
                push_u32(
                    sections
                        .entry(current_kind.clone())
                        .or_insert_with(|| SectionData::new(current_kind.clone())),
                    inst.encode(),
                    &mut current_addr,
                );
            }
            AsmToken::Branch {
                kind,
                rs1,
                rs2,
                target,
            } => {
                let word = encode_branch(kind, *rs1, *rs2, target, current_addr, &layout.symbols)?;
                push_u32(
                    sections
                        .entry(current_kind.clone())
                        .or_insert_with(|| SectionData::new(current_kind.clone())),
                    word,
                    &mut current_addr,
                );
            }
            AsmToken::Jal { rd, target } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                if let Ok(word) = encode_jal(*rd, target, current_addr, &layout.symbols) {
                    push_u32(sec, word, &mut current_addr);
                } else {
                    let reloc_offset = sec.current_offset();
                    sec.push_u32_le(RealInstruction::Jal(JalInst::new(*rd, 0)).encode());
                    out.relocations.push(RelocationRecord {
                        section: current_kind.name().to_owned(),
                        offset: reloc_offset,
                        symbol: target.clone(),
                        kind: RelocationKind::Jal,
                        addend: 0,
                    });
                    current_addr += 4;
                }
            }
            AsmToken::Call { symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_call(
                    sec,
                    symbol,
                    current_addr,
                    &layout.symbols,
                    &section_bases,
                    &mut out.relocations,
                    current_kind.name(),
                );
                current_addr += 8;
            }
            AsmToken::Tail { symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_tail(
                    sec,
                    symbol,
                    current_addr,
                    &layout.symbols,
                    &section_bases,
                    &mut out.relocations,
                    current_kind.name(),
                );
                current_addr += 8;
            }
            AsmToken::La { rd, symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_la(
                    sec,
                    *rd,
                    symbol,
                    current_addr,
                    &layout.symbols,
                    &section_bases,
                    &mut out.relocations,
                    current_kind.name(),
                )?;
                current_addr += 8;
            }
            AsmToken::Align(n) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                let alignment = 1usize << n;
                sec.align_to(alignment);
                current_addr =
                    section_bases.get(&current_kind).copied().unwrap_or(0) + sec.current_offset();
            }
            AsmToken::Balign(n) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.align_to(*n);
                current_addr =
                    section_bases.get(&current_kind).copied().unwrap_or(0) + sec.current_offset();
            }
            AsmToken::Space(n) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.bytes.extend(std::iter::repeat_n(0u8, *n as usize));
                current_addr += n;
            }
            AsmToken::DataU8(b) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.push_u8(*b);
                current_addr += 1;
            }
            AsmToken::DataU16(h) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.bytes.extend_from_slice(&h.to_le_bytes());
                current_addr += 2;
            }
            AsmToken::DataU32(w) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.push_u32_le(*w);
                current_addr += 4;
            }
            AsmToken::DataU64(d) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.push_u64_le(*d);
                current_addr += 8;
            }
            AsmToken::DataAsciz(s) => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                sec.bytes.extend_from_slice(s.as_bytes());
                sec.push_u8(0);
                current_addr += s.len() as u64 + 1;
            }
            AsmToken::Comment => {}
        }
    }

    // Flatten sections, assign absolute addresses to symbols.
    for kind in &layout.section_order {
        let base = section_bases.get(kind).copied().unwrap_or(0);
        if let Some(sec) = sections.remove(kind) {
            for (name, sec_offset) in &sec.symbols {
                out.symbol_table.insert(name.clone(), base + sec_offset);
            }
            out.sections.push(sec);
        }
    }

    for g in layout.symbols.globals() {
        if !out.global_symbols.contains(g) {
            out.global_symbols.push(g.clone());
        }
    }

    Ok(out)
}

// --- Branch and jump encoding helpers ---

fn encode_branch(
    kind: &BranchKind,
    rs1: u8,
    rs2: u8,
    target: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
) -> Result<u32, AssemblerError> {
    let target_addr = symbols
        .resolve(target)
        .ok_or_else(|| AssemblerError::new(format!("undefined label `{target}`")))?;

    let offset = (target_addr as i64) - (current_addr as i64);
    if offset & 1 != 0 {
        return Err(AssemblerError::new(format!(
            "branch to `{target}` has odd offset {offset}"
        )));
    }
    if !(-4096..=4094).contains(&offset) {
        return Err(AssemblerError::new(format!(
            "branch to `{target}` offset {offset} out of B-type range [-4096, 4094]"
        )));
    }
    let off = offset as i32;

    let inst: RealInstruction = match kind {
        BranchKind::Beq => RealInstruction::Beq(Beq::new(rs1, rs2, off)),
        BranchKind::Bne => RealInstruction::Bne(Bne::new(rs1, rs2, off)),
        BranchKind::Blt => RealInstruction::Blt(Blt::new(rs1, rs2, off)),
        BranchKind::Bge => RealInstruction::Bge(Bge::new(rs1, rs2, off)),
        BranchKind::Bltu => RealInstruction::Bltu(Bltu::new(rs1, rs2, off)),
        BranchKind::Bgeu => RealInstruction::Bgeu(Bgeu::new(rs1, rs2, off)),
    };
    Ok(inst.encode())
}

fn encode_jal(
    rd: u8,
    target: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
) -> Result<u32, AssemblerError> {
    let target_addr = symbols
        .resolve(target)
        .ok_or_else(|| AssemblerError::new(format!("undefined label `{target}`")))?;

    let offset = (target_addr as i64) - (current_addr as i64);
    if offset & 1 != 0 {
        return Err(AssemblerError::new(format!(
            "jump to `{target}` has odd offset {offset}"
        )));
    }
    if !(-1_048_576..=1_048_574).contains(&offset) {
        return Err(AssemblerError::new(format!(
            "jump to `{target}` offset {offset} out of J-type range"
        )));
    }

    Ok(RealInstruction::Jal(JalInst::new(rd, offset as i32)).encode())
}

fn push_u32(sec: &mut SectionData, word: u32, current_addr: &mut u64) {
    sec.push_u32_le(word);
    *current_addr += 4;
}

// --- Pseudo-instruction encoding with symbol relocation ---

/// Split a PC-relative byte offset into (hi20, lo12) for AUIPC+lo pairs.
fn pcrel_split(offset: i64) -> (i32, i32) {
    let lo12 = ((offset & 0xFFF) as i32).wrapping_sub(if offset & 0x800 != 0 { 0x1000 } else { 0 });
    let hi20 = ((offset - lo12 as i64) >> 12) as i32;
    (hi20, lo12)
}

fn resolve_absolute_symbol(
    symbol: &str,
    symbols: &super::symbol_table::SymbolTable,
    section_bases: &std::collections::HashMap<SectionKind, u64>,
) -> Option<u64> {
    if let Some(&addr) = symbols.all().get(symbol) {
        for (section_kind, base) in section_bases {
            let qualified_name = format!("{}@{}", symbol, section_kind.name());
            if let Some(&offset) = symbols.all().get(&qualified_name) {
                return Some(*base + offset);
            }
        }
        return Some(addr);
    }
    None
}

/// Compute PC-relative (hi20, lo12) for a target address relative to `current_addr`.
fn pcrel_offsets(target_addr: u64, current_addr: u64) -> (i32, i32) {
    pcrel_split((target_addr as i64) - (current_addr as i64))
}

/// Encode `call symbol` -> `auipc ra, %pcrel_hi(symbol); jalr ra, ra, %pcrel_lo(symbol)`.
fn encode_call(
    sec: &mut SectionData,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
    section_bases: &std::collections::HashMap<SectionKind, u64>,
    relocations: &mut Vec<RelocationRecord>,
    section_name: &str,
) {
    if let Some(target_addr) = resolve_absolute_symbol(symbol, symbols, section_bases) {
        let (hi20, lo12) = pcrel_offsets(target_addr, current_addr);
        sec.push_u32_le(Auipc::new(1, hi20 << 12).encode());
        sec.push_u32_le(Jalr::new(1, 1, lo12).encode());
        return;
    }
    // Emit relocation when the target is in a different object.
    let reloc_offset = sec.current_offset();
    sec.push_u32_le(Auipc::new(1, 0).encode());
    sec.push_u32_le(Jalr::new(1, 1, 0).encode());
    relocations.push(RelocationRecord {
        section: section_name.to_owned(),
        offset: reloc_offset,
        symbol: symbol.to_owned(),
        kind: RelocationKind::CallPlt,
        addend: 0,
    });
}

/// Encode `tail symbol` -> `auipc t1, %pcrel_hi(symbol); jalr x0, t1, %pcrel_lo(symbol)`.
fn encode_tail(
    sec: &mut SectionData,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
    section_bases: &std::collections::HashMap<SectionKind, u64>,
    relocations: &mut Vec<RelocationRecord>,
    section_name: &str,
) {
    if let Some(target_addr) = resolve_absolute_symbol(symbol, symbols, section_bases) {
        let (hi20, lo12) = pcrel_offsets(target_addr, current_addr);
        sec.push_u32_le(Auipc::new(6, hi20 << 12).encode());
        sec.push_u32_le(Jalr::new(0, 6, lo12).encode());
        return;
    }
    let reloc_offset = sec.current_offset();
    sec.push_u32_le(Auipc::new(6, 0).encode());
    sec.push_u32_le(Jalr::new(0, 6, 0).encode());
    relocations.push(RelocationRecord {
        section: section_name.to_owned(),
        offset: reloc_offset,
        symbol: symbol.to_owned(),
        kind: RelocationKind::CallPlt,
        addend: 0,
    });
}

/// Encode `la rd, symbol` -> `auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)`.
/// Always emits a relocation since section merging may change cross-section distances.
fn encode_la(
    sec: &mut SectionData,
    rd: u8,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
    section_bases: &std::collections::HashMap<SectionKind, u64>,
    relocations: &mut Vec<RelocationRecord>,
    section_name: &str,
) -> Result<(), AssemblerError> {
    let target_abs_addr = resolve_absolute_symbol(symbol, symbols, section_bases);
    let (hi20, lo12) = pcrel_offsets(target_abs_addr.unwrap_or(0), current_addr);
    let reloc_offset = sec.current_offset();
    sec.push_u32_le(Auipc::new(rd, hi20 << 12).encode());
    sec.push_u32_le(Addi::new(rd, rd, lo12).encode());
    relocations.push(RelocationRecord {
        section: section_name.to_owned(),
        offset: reloc_offset,
        symbol: symbol.to_owned(),
        kind: RelocationKind::La,
        addend: 0,
    });
    Ok(())
}
