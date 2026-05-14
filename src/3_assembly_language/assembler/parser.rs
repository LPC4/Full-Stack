use super::directive::Directive;
use super::reg_parse::parse_int_reg;
use super::section::SectionKind;
use super::token::{AsmToken, BranchKind};
use crate::assembly_language::real::RealInstruction;
use crate::assembly_language::riscv::rv64i::{Addi, Ecall, Jalr, Lbu, Ld, Sb, Sd};
/// Pass 0: parse `Vec<RvInstruction>` into `Vec<AsmToken>`.
///
/// `RvInstruction` carries some untyped raw strings (branches emitted via
/// `emit_raw`, data-section directives, etc.).  This pass converts every token
/// into a fully-typed `AsmToken` so subsequent passes never touch raw strings.
use crate::assembly_language::rv_instruction::RvInstruction;

/// Convert a `RvInstruction` stream to a typed `AsmToken` stream.
///
/// Tokens that cannot be parsed are emitted as `AsmToken::Comment` with the raw
/// text so nothing is silently lost -- the caller can choose to error on those.
pub fn parse(tokens: &[RvInstruction]) -> Vec<AsmToken> {
    let mut out = Vec::with_capacity(tokens.len());
    for tok in tokens {
        match tok {
            RvInstruction::Real(inst) => out.push(AsmToken::Real(inst.clone())),
            RvInstruction::Pseudo(p) => {
                // Expand pseudos that have no unresolved symbol references.
                for real in p.expand() {
                    out.push(AsmToken::Real(real));
                }
            }
            RvInstruction::Label(name) => out.push(AsmToken::Label(name.clone())),
            RvInstruction::Comment(text) => out.push(AsmToken::Comment(text.clone())),
            RvInstruction::Directive(raw) => {
                parse_directive_or_instruction(raw, &mut out);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Parsing `Directive` variants
// ---------------------------------------------------------------------------

fn parse_directive_or_instruction(raw: &str, out: &mut Vec<AsmToken>) {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return;
    }

    // Label lines emitted by data section: `name:` (no spaces in name)
    if let Some(label) = trimmed.strip_suffix(':') {
        if !label.is_empty() && !label.contains(|c: char| c.is_whitespace()) {
            out.push(AsmToken::Label(label.to_owned()));
            return;
        }
    }

    // Assembler directives start with `.`
    if trimmed.starts_with('.') {
        if let Some(dir) = Directive::parse(trimmed) {
            push_directive(dir, out);
        } else {
            out.push(AsmToken::Comment(format!(
                "unrecognised directive: {trimmed}"
            )));
        }
        return;
    }

    // Instruction lines start with whitespace (tab or spaces) in our emitter.
    // Strip leading whitespace then try to parse as a mnemonic.
    if raw.starts_with('\t') || raw.starts_with(' ') {
        parse_instruction_line(trimmed, out);
        return;
    }

    out.push(AsmToken::Comment(format!("unparsed line: {raw}")));
}

fn push_directive(dir: Directive, out: &mut Vec<AsmToken>) {
    match dir {
        Directive::Section(name) => {
            out.push(AsmToken::Section(SectionKind::from_str(&name)));
        }
        Directive::Globl(name) => out.push(AsmToken::Globl(name)),
        Directive::Align(n) => out.push(AsmToken::Align(n as usize)),
        Directive::Balign(n) => out.push(AsmToken::Balign(n as usize)),
        Directive::Byte(b) => out.push(AsmToken::DataU8(b)),
        Directive::Half(h) => out.push(AsmToken::DataU16(h)),
        Directive::Word(w) => out.push(AsmToken::DataU32(w)),
        Directive::Dword(d) => out.push(AsmToken::DataU64(d)),
        Directive::Asciz(s) => out.push(AsmToken::DataAsciz(s)),
        Directive::Space(n) => out.push(AsmToken::Space(n)),
        Directive::Equ(_, _) | Directive::Unknown(_) => {
            // Nothing to emit for equates/unknown directives at this stage.
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing raw instruction lines
// ---------------------------------------------------------------------------

/// Parse a trimmed instruction line and push typed `AsmToken`s into `out`.
///
/// May push more than one token (e.g. `li` expands to up to two real instructions).
/// Unrecognised mnemonics are emitted as `AsmToken::Comment` so nothing is silently lost.
fn parse_instruction_line(line: &str, out: &mut Vec<AsmToken>) {
    let (mnemonic, rest) = split_mnemonic(line);

    // Branch instructions: `bne rs1, rs2, label`
    if let Some(kind) = BranchKind::from_mnemonic(mnemonic) {
        if let Some(tok) = parse_branch(kind, rest) {
            out.push(tok);
        }
        return;
    }

    // Unconditional jump: `j label`
    if mnemonic == "j" {
        let target = rest.trim().to_owned();
        if !target.is_empty() {
            out.push(AsmToken::Jal { rd: 0, target });
        }
        return;
    }

    // `jal rd, label`
    if mnemonic == "jal" {
        if let Some(tok) = parse_jal(rest) {
            out.push(tok);
        }
        return;
    }

    // `call symbol`
    if mnemonic == "call" {
        let symbol = rest.trim().to_owned();
        if !symbol.is_empty() {
            out.push(AsmToken::Call { symbol });
        }
        return;
    }

    // `tail symbol`
    if mnemonic == "tail" {
        let symbol = rest.trim().to_owned();
        if !symbol.is_empty() {
            out.push(AsmToken::Tail { symbol });
        }
        return;
    }

    // `la rd, symbol`
    if mnemonic == "la" {
        if let Some(tok) = parse_la(rest) {
            out.push(tok);
        }
        return;
    }

    // `ecall`
    if mnemonic == "ecall" {
        out.push(AsmToken::Real(RealInstruction::Ecall(Ecall)));
        return;
    }

    // `ret`  →  jalr x0, 0(ra)
    if mnemonic == "ret" {
        out.push(AsmToken::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))));
        return;
    }

    // `li rd, imm`  (pseudo: expands to addi or lui+addi)
    if mnemonic == "li" {
        if let Some((rd, imm)) = parse_two_fields(rest) {
            if let Some(rd) = parse_int_reg(rd) {
                if let Ok(imm) = imm.parse::<i64>() {
                    expand_li(rd, imm, out);
                    return;
                }
            }
        }
        out.push(AsmToken::Comment(format!("unrecognised li: {line}")));
        return;
    }

    // `mv rd, rs`  →  addi rd, rs, 0
    if mnemonic == "mv" {
        if let Some((rd_str, rs_str)) = parse_two_fields(rest) {
            if let (Some(rd), Some(rs)) = (parse_int_reg(rd_str), parse_int_reg(rs_str)) {
                out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(rd, rs, 0))));
                return;
            }
        }
        out.push(AsmToken::Comment(format!("unrecognised mv: {line}")));
        return;
    }

    // `addi rd, rs1, imm`
    if mnemonic == "addi" {
        if let Some((rd, rs1, imm)) = parse_r_i_imm(rest) {
            out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
                rd, rs1, imm,
            ))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised addi: {line}")));
        return;
    }

    // Store: `sd rs2, imm(rs1)`  →  Sd::new(base=rs1, src=rs2, offset=imm)
    if mnemonic == "sd" {
        if let Some((rs2, rs1, imm)) = parse_store_mem(rest) {
            out.push(AsmToken::Real(RealInstruction::Sd(Sd::new(rs1, rs2, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised sd: {line}")));
        return;
    }

    // Store: `sb rs2, imm(rs1)`
    if mnemonic == "sb" {
        if let Some((rs2, rs1, imm)) = parse_store_mem(rest) {
            out.push(AsmToken::Real(RealInstruction::Sb(Sb::new(rs1, rs2, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised sb: {line}")));
        return;
    }

    // Load: `ld rd, imm(rs1)`  →  Ld::new(rd, base=rs1, offset=imm)
    if mnemonic == "ld" {
        if let Some((rd, rs1, imm)) = parse_load_mem(rest) {
            out.push(AsmToken::Real(RealInstruction::Ld(Ld::new(rd, rs1, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised ld: {line}")));
        return;
    }

    // Load: `lbu rd, imm(rs1)`
    if mnemonic == "lbu" {
        if let Some((rd, rs1, imm)) = parse_load_mem(rest) {
            out.push(AsmToken::Real(RealInstruction::Lbu(Lbu::new(rd, rs1, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised lbu: {line}")));
        return;
    }

    out.push(AsmToken::Comment(format!(
        "unrecognised instruction: {line}"
    )));
}

/// Expand `li rd, imm` into addi (1-instr) or lui+addi (2-instr).
fn expand_li(rd: u8, imm: i64, out: &mut Vec<AsmToken>) {
    use crate::assembly_language::riscv::rv64i::Lui;
    if (-2048..=2047).contains(&imm) {
        out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
            rd, 0, imm as i32,
        ))));
    } else {
        let hi = ((imm >> 12) & 0xFFFFF) as i32;
        let lo = (imm & 0xFFF) as i32;
        let lo_signed = if lo >= 0x800 { lo - 0x1000 } else { lo };
        let hi_adj = if lo_signed < 0 { hi + 1 } else { hi };
        out.push(AsmToken::Real(RealInstruction::Lui(Lui::new(
            rd,
            hi_adj << 12,
        ))));
        if lo_signed != 0 {
            out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
                rd, rd, lo_signed,
            ))));
        }
    }
}

/// Split `"rd, rest"` into `(rd_str, rest_str)`.
fn parse_two_fields(operands: &str) -> Option<(&str, &str)> {
    let comma = operands.find(',')?;
    Some((operands[..comma].trim(), operands[comma + 1..].trim()))
}

/// Parse `"rd, rs1, imm"` → (rd_reg, rs1_reg, imm_i32).
fn parse_r_i_imm(operands: &str) -> Option<(u8, u8, i32)> {
    let mut parts = operands.splitn(3, ',');
    let rd = parse_int_reg(parts.next()?.trim())?;
    let rs1 = parse_int_reg(parts.next()?.trim())?;
    let imm: i32 = parts.next()?.trim().parse().ok()?;
    Some((rd, rs1, imm))
}

/// Parse `"rs2, imm(rs1)"` → (rs2_reg, rs1_reg, imm_i32).  Used by stores.
fn parse_store_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rs2 = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rs2, rs1, imm))
}

/// Parse `"rd, imm(rs1)"` → (rd_reg, rs1_reg, imm_i32).  Used by loads.
fn parse_load_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rd = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rd, rs1, imm))
}

/// Parse `"imm(rs1)"` → (rs1_reg, imm_i32).
fn parse_mem_ref(s: &str) -> Option<(u8, i32)> {
    let lparen = s.find('(')?;
    let rparen = s.find(')')?;
    let imm: i32 = s[..lparen].trim().parse().ok()?;
    let rs1 = parse_int_reg(s[lparen + 1..rparen].trim())?;
    Some((rs1, imm))
}

fn split_mnemonic(line: &str) -> (&str, &str) {
    match line.find(|c: char| c.is_whitespace()) {
        Some(idx) => (&line[..idx], line[idx..].trim_start()),
        None => (line, ""),
    }
}

/// Parse `rs1, rs2, label` for a branch instruction.
fn parse_branch(kind: BranchKind, operands: &str) -> Option<AsmToken> {
    let parts: Vec<&str> = operands.splitn(3, ',').collect();
    if parts.len() != 3 {
        return None;
    }
    let rs1 = parse_int_reg(parts[0].trim())?;
    let rs2 = parse_int_reg(parts[1].trim())?;
    let target = parts[2].trim().to_owned();
    Some(AsmToken::Branch {
        kind,
        rs1,
        rs2,
        target,
    })
}

/// Parse `rd, label` (or just `label` for the `j` form handled above).
fn parse_jal(operands: &str) -> Option<AsmToken> {
    let parts: Vec<&str> = operands.splitn(2, ',').collect();
    match parts.len() {
        2 => {
            let rd = parse_int_reg(parts[0].trim())?;
            let target = parts[1].trim().to_owned();
            Some(AsmToken::Jal { rd, target })
        }
        1 => {
            // `jal label` -- treat as `jal ra, label`
            let target = parts[0].trim().to_owned();
            if target.is_empty() {
                None
            } else {
                Some(AsmToken::Jal { rd: 1, target })
            }
        }
        _ => None,
    }
}

/// Parse `rd, symbol` for the `la` pseudo-instruction.
fn parse_la(operands: &str) -> Option<AsmToken> {
    let parts: Vec<&str> = operands.splitn(2, ',').collect();
    if parts.len() != 2 {
        return None;
    }
    let rd = parse_int_reg(parts[0].trim())?;
    let symbol = parts[1].trim().to_owned();
    if symbol.is_empty() {
        None
    } else {
        Some(AsmToken::La { rd, symbol })
    }
}
