use super::directive::Directive;
use super::reg_parse::parse_int_reg;
use super::section::SectionKind;
use super::token::{AsmToken, BranchKind};
use crate::real::RealInstruction;
use crate::riscv::rv64i::{
    Add, Addi, And, Andi, Ecall, Jalr, Lb, Lbu, Ld, Mret, Or, Ori, Sb, Sd, Slli, Srl, Sret, Sub,
};
use crate::riscv::rv64m::{Divu, Remu};
use crate::riscv::rv64zicsr::{Csrrs, Csrrw};
/// Pass 0: parse `Vec<RvInstruction>` into `Vec<AsmToken>`.
///
/// `RvInstruction` carries some untyped raw strings (branches emitted via
/// `emit_raw`, data-section directives, etc.).  This pass converts every token
/// into a fully-typed `AsmToken` so subsequent passes never touch raw strings.
use crate::rv_instruction::RvInstruction;

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
    // Strip trailing inline `;` comments before any classification.
    // Exception: `.asciz` lines may contain `;` inside the string literal and
    // are handled by the directive parser which already understands quoting.
    let raw = if raw.trim_start().starts_with(".asciz") {
        raw
    } else {
        match raw.find(';') {
            Some(i) => raw[..i].trim_end(),
            None => raw,
        }
    };

    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return;
    }

    // Label lines emitted by data section: `name:` (no spaces in name)
    if let Some(label) = trimmed.strip_suffix(':')
        && !label.is_empty()
        && !label.contains(|c: char| c.is_whitespace())
    {
        out.push(AsmToken::Label(label.to_owned()));
        return;
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
    // Strip inline `;` comments before any operand parsing.
    let line = match line.find(';') {
        Some(i) => line[..i].trim_end(),
        None => line,
    };
    if line.is_empty() {
        return;
    }
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
        if let Some((rd, imm)) = parse_two_fields(rest)
            && let Some(rd) = parse_int_reg(rd)
            && let Some(imm) = parse_imm_i64(imm)
        {
            expand_li(rd, imm, out);
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised li: {line}")));
        return;
    }

    // `mv rd, rs`  →  addi rd, rs, 0
    if mnemonic == "mv" {
        if let Some((rd_str, rs_str)) = parse_two_fields(rest)
            && let (Some(rd), Some(rs)) = (parse_int_reg(rd_str), parse_int_reg(rs_str))
        {
            out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(rd, rs, 0))));
            return;
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

    // Load: `lb rd, imm(rs1)`
    if mnemonic == "lb" {
        if let Some((rd, rs1, imm)) = parse_load_mem(rest) {
            out.push(AsmToken::Real(RealInstruction::Lb(Lb::new(rd, rs1, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised lb: {line}")));
        return;
    }

    // `andi rd, rs1, imm`
    if mnemonic == "andi" {
        if let Some((rd, rs1, imm)) = parse_r_i_imm(rest) {
            out.push(AsmToken::Real(RealInstruction::Andi(Andi::new(
                rd, rs1, imm,
            ))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised andi: {line}")));
        return;
    }

    // `mret`
    if mnemonic == "mret" {
        out.push(AsmToken::Real(RealInstruction::Mret(Mret::new())));
        return;
    }

    // `sret`
    if mnemonic == "sret" {
        out.push(AsmToken::Real(RealInstruction::Sret(Sret::new())));
        return;
    }

    // `ori rd, rs1, imm`
    if mnemonic == "ori" {
        if let Some((rd, rs1, imm)) = parse_r_i_imm(rest) {
            out.push(AsmToken::Real(RealInstruction::Ori(Ori::new(rd, rs1, imm))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised ori: {line}")));
        return;
    }

    // `slli rd, rs1, shamt`
    if mnemonic == "slli" {
        if let Some((rd, rs1, shamt)) = parse_r_i_imm(rest)
            && (0..=63).contains(&shamt)
        {
            out.push(AsmToken::Real(RealInstruction::Slli(Slli::new(
                rd,
                rs1,
                shamt as u8,
            ))));
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised slli: {line}")));
        return;
    }

    // `csrr rd, csr`  ->  csrrs rd, csr, x0
    if mnemonic == "csrr" {
        if let Some(tok) = parse_csrr(rest) {
            out.push(tok);
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised csrr: {line}")));
        return;
    }

    // `csrw csr, rs`  ->  csrrw x0, csr, rs
    if mnemonic == "csrw" {
        if let Some(tok) = parse_csrw(rest) {
            out.push(tok);
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised csrw: {line}")));
        return;
    }

    // `beqz rs, label`  ->  beq rs, x0, label
    if mnemonic == "beqz" {
        if let Some(tok) = parse_branch_zero(BranchKind::Beq, rest) {
            out.push(tok);
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised beqz: {line}")));
        return;
    }

    // `bnez rs, label`  ->  bne rs, x0, label
    if mnemonic == "bnez" {
        if let Some(tok) = parse_branch_zero(BranchKind::Bne, rest) {
            out.push(tok);
            return;
        }
        out.push(AsmToken::Comment(format!("unrecognised bnez: {line}")));
        return;
    }

    // R-type instructions: `add`, `sub`, `srl`, `or`, `divu`, `remu`
    if let Some(tok) = parse_r_type(mnemonic, rest) {
        out.push(tok);
        return;
    }

    out.push(AsmToken::Comment(format!(
        "unrecognised instruction: {line}"
    )));
}

/// Expand `li rd, imm` into addi (1-instr) or lui+addi (2-instr).
fn expand_li(rd: u8, imm: i64, out: &mut Vec<AsmToken>) {
    use crate::riscv::rv64i::Lui;
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

/// Parse an integer immediate: decimal, or `0x`/`0X` hex prefix.
fn parse_imm_i64(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

/// Split `"rd, rest"` into `(rd_str, rest_str)`.
fn parse_two_fields(operands: &str) -> Option<(&str, &str)> {
    let comma = operands.find(',')?;
    Some((operands[..comma].trim(), operands[comma + 1..].trim()))
}

/// Parse `"rd, rs1, imm"` → (`rd_reg`, `rs1_reg`, `imm_i32`).
fn parse_r_i_imm(operands: &str) -> Option<(u8, u8, i32)> {
    let mut parts = operands.splitn(3, ',');
    let rd = parse_int_reg(parts.next()?.trim())?;
    let rs1 = parse_int_reg(parts.next()?.trim())?;
    let imm = parse_imm_i64(parts.next()?.trim())? as i32;
    Some((rd, rs1, imm))
}

/// Parse `"rs2, imm(rs1)"` → (`rs2_reg`, `rs1_reg`, `imm_i32`).  Used by stores.
fn parse_store_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rs2 = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rs2, rs1, imm))
}

/// Parse `"rd, imm(rs1)"` → (`rd_reg`, `rs1_reg`, `imm_i32`).  Used by loads.
fn parse_load_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rd = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rd, rs1, imm))
}

/// Parse `"imm(rs1)"` → (`rs1_reg`, `imm_i32`).
fn parse_mem_ref(s: &str) -> Option<(u8, i32)> {
    let lparen = s.find('(')?;
    let rparen = s.find(')')?;
    let imm = parse_imm_i64(s[..lparen].trim())? as i32;
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

// ---------------------------------------------------------------------------
// New instruction parsers
// ---------------------------------------------------------------------------

/// Parse `rs, label` for a zero-register branch (`beqz`/`bnez`).
/// Expands to `beq/bne rs, x0, label`.
fn parse_branch_zero(kind: BranchKind, operands: &str) -> Option<AsmToken> {
    let comma = operands.find(',')?;
    let rs1 = parse_int_reg(operands[..comma].trim())?;
    let target = operands[comma + 1..].trim().to_owned();
    if target.is_empty() {
        return None;
    }
    Some(AsmToken::Branch {
        kind,
        rs1,
        rs2: 0,
        target,
    })
}

/// Map a CSR name or numeric literal to its 12-bit address.
fn parse_csr_name(name: &str) -> Option<u16> {
    match name.trim() {
        // Machine-mode CSRs
        "mstatus" => Some(0x300),
        "misa" => Some(0x301),
        "medeleg" => Some(0x302),
        "mideleg" => Some(0x303),
        "mie" => Some(0x304),
        "mtvec" => Some(0x305),
        "mscratch" => Some(0x340),
        "mepc" => Some(0x341),
        "mcause" => Some(0x342),
        "mtval" => Some(0x343),
        "mip" => Some(0x344),
        "pmpcfg0" => Some(0x3A0),
        "pmpaddr0" => Some(0x3B0),
        // Supervisor-mode CSRs
        "sstatus" => Some(0x100),
        "sie" => Some(0x104),
        "stvec" => Some(0x105),
        "sscratch" => Some(0x140),
        "sepc" => Some(0x141),
        "scause" => Some(0x142),
        "stval" => Some(0x143),
        "sip" => Some(0x144),
        "satp" => Some(0x180),
        // Hex / decimal fallback
        s if s.starts_with("0x") || s.starts_with("0X") => u16::from_str_radix(&s[2..], 16).ok(),
        s => s.parse::<u16>().ok(),
    }
}

/// Parse `rd, csr` for `csrr` (pseudo: csrrs rd, csr, x0).
fn parse_csrr(operands: &str) -> Option<AsmToken> {
    let comma = operands.find(',')?;
    let rd = parse_int_reg(operands[..comma].trim())?;
    let csr = parse_csr_name(operands[comma + 1..].trim())?;
    Some(AsmToken::Real(RealInstruction::Csrrs(Csrrs::new(
        rd, csr, 0,
    ))))
}

/// Parse `csr, rs` for `csrw` (pseudo: csrrw x0, csr, rs).
fn parse_csrw(operands: &str) -> Option<AsmToken> {
    let comma = operands.find(',')?;
    let csr = parse_csr_name(operands[..comma].trim())?;
    let rs = parse_int_reg(operands[comma + 1..].trim())?;
    Some(AsmToken::Real(RealInstruction::Csrrw(Csrrw::new(
        0, csr, rs,
    ))))
}

/// Parse `rd, rs1, rs2` for R-type instructions.
fn parse_r_type(mnemonic: &str, operands: &str) -> Option<AsmToken> {
    let parts: Vec<&str> = operands.splitn(3, ',').collect();
    if parts.len() != 3 {
        return None;
    }
    let rd = parse_int_reg(parts[0].trim())?;
    let rs1 = parse_int_reg(parts[1].trim())?;
    let rs2 = parse_int_reg(parts[2].trim())?;
    let real = match mnemonic {
        "add" => RealInstruction::Add(Add::new(rd, rs1, rs2)),
        "sub" => RealInstruction::Sub(Sub::new(rd, rs1, rs2)),
        "and" => RealInstruction::And(And::new(rd, rs1, rs2)),
        "srl" => RealInstruction::Srl(Srl::new(rd, rs1, rs2)),
        "or" => RealInstruction::Or(Or::new(rd, rs1, rs2)),
        "divu" => RealInstruction::Divu(Divu::new(rd, rs1, rs2)),
        "remu" => RealInstruction::Remu(Remu::new(rd, rs1, rs2)),
        _ => return None,
    };
    Some(AsmToken::Real(real))
}
