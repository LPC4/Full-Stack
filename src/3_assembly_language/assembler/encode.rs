use super::AssemblerError;
use super::layout::Layout;
use super::output::AssembledOutput;
use super::section::{SectionData, SectionKind};
use super::token::{AsmToken, BranchKind};
/// Pass 2: encode the typed token stream to bytes, resolving all label references.
///
/// At this point the `SymbolTable` from the layout pass contains every label's
/// section-relative address.  We convert those to absolute addresses by adding
/// the running section base, then compute PC-relative branch/jump offsets and
/// encode the final machine words.
use crate::assembly_language::real::RealInstruction;
use crate::assembly_language::riscv::rv64i::{
    Addi, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne, Jal as JalInst, Jalr,
};
use crate::assembly_language::traits::Instruction;

/// Encode all tokens into an `AssembledOutput` using layout information for
/// label resolution.
pub fn encode(tokens: &[AsmToken], layout: &Layout) -> Result<AssembledOutput, AssemblerError> {
    let mut out = AssembledOutput::new();

    // Build one `SectionData` per section in discovery order.
    let mut sections: std::collections::HashMap<SectionKind, SectionData> = layout
        .section_order
        .iter()
        .map(|k| (k.clone(), SectionData::new(k.clone())))
        .collect();

    // Compute the absolute base address of each section by packing them
    // consecutively (base of section N = sum of sizes of sections 0..N-1).
    let mut section_bases: std::collections::HashMap<SectionKind, u64> =
        std::collections::HashMap::new();
    let mut running_base: u64 = 0;
    for kind in &layout.section_order {
        section_bases.insert(kind.clone(), running_base);
        running_base += layout.section_sizes.get(kind).copied().unwrap_or(0);
    }

    // Walk tokens and emit bytes.
    let mut current_kind = SectionKind::Text;
    // Current absolute address of the next byte to be emitted.
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
                let word = encode_jal(*rd, target, current_addr, &layout.symbols)?;
                push_u32(
                    sections
                        .entry(current_kind.clone())
                        .or_insert_with(|| SectionData::new(current_kind.clone())),
                    word,
                    &mut current_addr,
                );
            }

            AsmToken::Call { symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_call(sec, symbol, current_addr, &layout.symbols)?;
                current_addr += 8; // 2 instructions
            }

            AsmToken::Tail { symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_tail(sec, symbol, current_addr, &layout.symbols)?;
                current_addr += 8; // 2 instructions
            }

            AsmToken::La { rd, symbol } => {
                let sec = sections
                    .entry(current_kind.clone())
                    .or_insert_with(|| SectionData::new(current_kind.clone()));
                encode_la(sec, *rd, symbol, current_addr, &layout.symbols)?;
                current_addr += 8; // 2 instructions
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
                sec.bytes.extend(std::iter::repeat(0u8).take(*n as usize));
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
                sec.push_u8(0); // null terminator
                current_addr += s.len() as u64 + 1;
            }

            AsmToken::Comment(_) => {}
        }
    }

    // Flatten sections into output, assigning absolute addresses to symbols.
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

// ---------------------------------------------------------------------------
// Branch and jump encoding helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Pseudo-instruction encoding with symbol relocation
// ---------------------------------------------------------------------------

/// Encode `call symbol` → `auipc ra, %pcrel_hi(symbol); jalr ra, ra, %pcrel_lo(symbol)`
fn encode_call(
    sec: &mut SectionData,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
) -> Result<(), AssemblerError> {
    let target_addr = symbols
        .resolve(symbol)
        .ok_or_else(|| AssemblerError::new(format!("undefined symbol `{symbol}`")))?;

    // auipc uses its own PC (current_addr) as the base, not PC+4
    let offset = (target_addr as i64) - (current_addr as i64);

    // Split into hi20 and lo12 (same as li but PC-relative)
    // The lo12 is sign-extended, so we need to adjust hi20 if bit 11 of lo12 is set
    let lo12 = ((offset & 0xFFF) as i32).wrapping_sub(if offset & 0x800 != 0 { 0x1000 } else { 0 });
    let hi20 = ((offset - lo12 as i64) >> 12) as i32;

    // Emit: auipc ra, hi20
    let auipc_word = Auipc::new(1, hi20).encode(); // x1 = ra
    sec.push_u32_le(auipc_word);

    // Emit: jalr ra, ra, lo12
    let jalr_word = Jalr::new(1, 1, lo12).encode(); // rd=ra, rs1=ra, imm=lo12
    sec.push_u32_le(jalr_word);

    Ok(())
}

/// Encode `tail symbol` → `auipc t1, %pcrel_hi(symbol); jalr x0, t1, %pcrel_lo(symbol)`
fn encode_tail(
    sec: &mut SectionData,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
) -> Result<(), AssemblerError> {
    let target_addr = symbols
        .resolve(symbol)
        .ok_or_else(|| AssemblerError::new(format!("undefined symbol `{symbol}`")))?;

    // auipc uses its own PC (current_addr) as the base, not PC+4
    let offset = (target_addr as i64) - (current_addr as i64);

    let lo12 = ((offset & 0xFFF) as i32).wrapping_sub(if offset & 0x800 != 0 { 0x1000 } else { 0 });
    let hi20 = ((offset - lo12 as i64) >> 12) as i32;

    // Emit: auipc t1, hi20
    let auipc_word = Auipc::new(6, hi20).encode(); // x6 = t1
    sec.push_u32_le(auipc_word);

    // Emit: jalr x0, t1, lo12 (tail call, no return)
    let jalr_word = Jalr::new(0, 6, lo12).encode(); // rd=x0, rs1=t1, imm=lo12
    sec.push_u32_le(jalr_word);

    Ok(())
}

/// Encode `la rd, symbol` → `auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)`
fn encode_la(
    sec: &mut SectionData,
    rd: u8,
    symbol: &str,
    current_addr: u64,
    symbols: &super::symbol_table::SymbolTable,
) -> Result<(), AssemblerError> {
    let target_addr = symbols
        .resolve(symbol)
        .ok_or_else(|| AssemblerError::new(format!("undefined symbol `{symbol}`")))?;

    // auipc uses its own PC (current_addr) as the base, not PC+4
    let offset = (target_addr as i64) - (current_addr as i64);

    let lo12 = ((offset & 0xFFF) as i32).wrapping_sub(if offset & 0x800 != 0 { 0x1000 } else { 0 });
    let hi20 = ((offset - lo12 as i64) >> 12) as i32;

    // Emit: auipc rd, hi20
    let auipc_word = Auipc::new(rd, hi20).encode();
    sec.push_u32_le(auipc_word);

    // Emit: addi rd, rd, lo12
    let addi_word = Addi::new(rd, rd, lo12).encode();
    sec.push_u32_le(addi_word);

    Ok(())
}

