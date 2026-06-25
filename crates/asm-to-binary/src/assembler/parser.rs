macro_rules! asm_warn {
    ($out:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("asm warning: {msg}");
        $out.push(AsmToken::Comment);
    }};
}

/// Try to parse `$expr`; on success push to `$out`, else warn with `$label`.
macro_rules! try_parse_or_warn {
    ($out:expr, $line:expr, $label:literal, $expr:expr) => {
        if let Some(tok) = $expr {
            $out.push(tok);
        } else {
            asm_warn!($out, "unrecognised {}: {}", $label, $line);
        }
    };
}

/// Pass 0: parse `Vec<RvInstruction>` into `Vec<AsmToken>`.
///
/// `RvInstruction` carries some untyped raw strings (branches emitted via
/// `emit_raw`, data-section directives, etc.).  This pass converts every token
/// into a fully-typed `AsmToken` so subsequent passes never touch raw strings.
use super::directive::Directive;
use super::reg_parse::parse_int_reg;
use super::section::SectionKind;
use super::token::{AsmToken, BranchKind};
use crate::pseudo::PseudoInstruction;
use crate::real::RealInstruction;
use crate::riscv::rv64i::Srai;
use crate::riscv::rv64i::{
    Add, Addi, And, Andi, Ecall, Jalr, Lb, Lbu, Ld, Lui, Mret, Or, Ori, Sb, Sd, SfenceVma, Slli,
    Sret, Srl, Srli, Sub, Wfi,
};
use crate::riscv::rv64m::{Divu, Mul, Remu};
use crate::riscv::rv64zicsr::{Csrrc, Csrrs, Csrrw};
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
            RvInstruction::Pseudo(p) => match p {
                // Keep symbol-bearing pseudos unresolved so pass-2 can
                // either resolve them immediately or emit relocation records.
                PseudoInstruction::Call { symbol } => out.push(AsmToken::Call {
                    symbol: symbol.clone(),
                }),
                PseudoInstruction::Tail { symbol } => out.push(AsmToken::Tail {
                    symbol: symbol.clone(),
                }),
                PseudoInstruction::La { rd, symbol } => out.push(AsmToken::La {
                    rd: *rd,
                    symbol: symbol.clone(),
                }),
                _ => {
                    for real in p.expand() {
                        out.push(AsmToken::Real(real));
                    }
                }
            },
            RvInstruction::Label(name) => out.push(AsmToken::Label(name.clone())),
            RvInstruction::Comment(_) => out.push(AsmToken::Comment),
            RvInstruction::Directive(raw) => {
                parse_directive_or_instruction(raw, &mut out);
            }
        }
    }
    out
}

// --- Parsing `Directive` variants ---

fn parse_directive_or_instruction(raw: &str, out: &mut Vec<AsmToken>) {
    // Strip trailing inline `;` or `#` comments before any classification.
    // Exception: `.asciz` lines may contain these characters inside the
    // string literal and are handled by the directive parser which already
    // understands quoting.
    let raw = if raw.trim_start().starts_with(".asciz") {
        raw
    } else {
        match raw.find([';', '#']) {
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
            asm_warn!(out, "unrecognised directive: {trimmed}");
        }
        return;
    }

    // Instruction lines start with whitespace (tab or spaces) in our emitter.
    if raw.starts_with('\t') || raw.starts_with(' ') {
        parse_instruction_line(trimmed, out);
        return;
    }

    out.push(AsmToken::Comment);
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
        Directive::Equ(_, _) | Directive::Unknown(_) => {}
    }
}

// --- Parsing raw instruction lines ---

/// Parse a trimmed instruction line and push typed `AsmToken`s into `out`.
///
/// May push more than one token (e.g. `li` expands to up to two real instructions).
/// Unrecognised mnemonics are emitted as `AsmToken::Comment` so nothing is silently lost.
fn parse_instruction_line(line: &str, out: &mut Vec<AsmToken>) {
    // Strip inline `;` or `#` comments before any operand parsing.
    let line = match line.find([';', '#']) {
        Some(i) => line[..i].trim_end(),
        None => line,
    };
    if line.is_empty() {
        return;
    }
    let (mnemonic, rest) = split_mnemonic(line);

    // --- Dispatch by mnemonic ---

    // Branch instructions (`bne`, `beq`, `blt`, `bge`, `bltu`, `bgeu`)
    if let Some(kind) = BranchKind::from_mnemonic(mnemonic) {
        if let Some(tok) = parse_branch(kind, rest) {
            out.push(tok);
        }
        return;
    }

    match mnemonic {
        // --- Symbol-bearing pseudos ---
        "call" => {
            let sym = rest.trim().to_owned();
            if !sym.is_empty() {
                out.push(AsmToken::Call { symbol: sym });
            }
        }
        "tail" => {
            let sym = rest.trim().to_owned();
            if !sym.is_empty() {
                out.push(AsmToken::Tail { symbol: sym });
            }
        }
        "j" => {
            let target = rest.trim().to_owned();
            if !target.is_empty() {
                out.push(AsmToken::Jal { rd: 0, target });
            }
        }
        "jal" => try_parse_or_warn!(out, line, "jal", parse_jal(rest)),
        "la" => try_parse_or_warn!(out, line, "la", parse_la(rest)),

        // --- No-operand instructions ---
        "ecall" => out.push(AsmToken::Real(RealInstruction::Ecall(Ecall))),
        "wfi" => out.push(AsmToken::Real(RealInstruction::Wfi(Wfi::new()))),
        "ret" => out.push(AsmToken::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0)))),
        "nop" => out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(0, 0, 0)))),
        "mret" => out.push(AsmToken::Real(RealInstruction::Mret(Mret::new()))),
        "sret" => out.push(AsmToken::Real(RealInstruction::Sret(Sret::new()))),
        "sfence.vma" => {
            out.push(AsmToken::Real(RealInstruction::SfenceVma(SfenceVma::new())));
        }

        // --- Two-operand pseudos ---
        "li" => {
            if let Some(tokens) = try_parse_li(rest) {
                out.extend(tokens);
            } else {
                asm_warn!(out, "unrecognised li: {line}");
            }
        }
        "mv" => {
            if let Some((rd, rs)) = parse_two_regs(rest) {
                out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(rd, rs, 0))));
            } else {
                asm_warn!(out, "unrecognised mv: {line}");
            }
        }

        // --- Stores: `rs2, imm(rs1)` format ---
        "sb" | "sw" | "sd" => {
            if let Some((rs2, rs1, imm)) = parse_store_mem(rest) {
                let inst = store_inst(mnemonic, rs1, rs2, imm);
                out.push(inst);
            } else {
                asm_warn!(out, "unrecognised {mnemonic}: {line}");
            }
        }

        // --- Loads: `rd, imm(rs1)` format ---
        "lb" | "lbu" | "lw" | "ld" => {
            if let Some((rd, rs1, imm)) = parse_load_mem(rest) {
                let inst = load_inst(mnemonic, rd, rs1, imm);
                out.push(inst);
            } else {
                asm_warn!(out, "unrecognised {mnemonic}: {line}");
            }
        }

        // --- I-type: `rd, rs1, imm` format ---
        "addi" | "andi" | "ori" => {
            if let Some((rd, rs1, imm)) = parse_r_i_imm(rest) {
                let inst = i_imm_inst(mnemonic, rd, rs1, imm);
                out.push(inst);
            } else {
                asm_warn!(out, "unrecognised {mnemonic}: {line}");
            }
        }

        // --- I-type shifts: `rd, rs1, shamt` with range check ---
        "slli" | "srli" | "srai" => {
            if let Some((rd, rs1, shamt)) = parse_r_i_imm(rest)
                && (0..=63).contains(&shamt)
            {
                let inst = shift_inst(mnemonic, rd, rs1, shamt as u8);
                out.push(inst);
            } else {
                asm_warn!(out, "unrecognised {mnemonic}: {line}");
            }
        }

        // --- CSR read shorthand: `csrr rd, csr` -> `csrrs rd, csr, x0` ---
        "csrr" => {
            if let Some(tok) = parse_csrr(rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised csrr: {line}");
            }
        }

        // --- CSR write shorthand: `csrw csr, rs` -> `csrrw x0, csr, rs` ---
        "csrw" => {
            if let Some(tok) = parse_csrw(rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised csrw: {line}");
            }
        }

        // --- Full CSR RMW: `csrrw/csrrs/csrrc rd, csr, rs1` ---
        "csrrw" | "csrrs" | "csrrc" => {
            if let Some(tok) = parse_csr_rmw(mnemonic, rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised {mnemonic}: {line}");
            }
        }

        // --- Zero-register branch pseudos ---
        "beqz" => {
            if let Some(tok) = parse_branch_zero(BranchKind::Beq, rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised beqz: {line}");
            }
        }
        "bnez" => {
            if let Some(tok) = parse_branch_zero(BranchKind::Bne, rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised bnez: {line}");
            }
        }

        // --- R-type fallback (add, sub, and, srl, or, divu, remu, mul) ---
        _ => {
            if let Some(tok) = parse_r_type(mnemonic, rest) {
                out.push(tok);
            } else {
                asm_warn!(out, "unrecognised instruction: {line}");
            }
        }
    }
}

// --- Instruction constructors (per-format dispatch helpers) ---

/// Build a store `AsmToken` for the given mnemonic.
fn store_inst(mnemonic: &str, rs1: u8, rs2: u8, imm: i32) -> AsmToken {
    use crate::riscv::rv64i::Sw;
    match mnemonic {
        "sb" => AsmToken::Real(RealInstruction::Sb(Sb::new(rs1, rs2, imm))),
        "sw" => AsmToken::Real(RealInstruction::Sw(Sw::new(rs1, rs2, imm))),
        "sd" => AsmToken::Real(RealInstruction::Sd(Sd::new(rs1, rs2, imm))),
        _ => unreachable!(),
    }
}

/// Build a load `AsmToken` for the given mnemonic.
fn load_inst(mnemonic: &str, rd: u8, rs1: u8, imm: i32) -> AsmToken {
    use crate::riscv::rv64i::Lw;
    match mnemonic {
        "lb" => AsmToken::Real(RealInstruction::Lb(Lb::new(rd, rs1, imm))),
        "lbu" => AsmToken::Real(RealInstruction::Lbu(Lbu::new(rd, rs1, imm))),
        "lw" => AsmToken::Real(RealInstruction::Lw(Lw::new(rd, rs1, imm))),
        "ld" => AsmToken::Real(RealInstruction::Ld(Ld::new(rd, rs1, imm))),
        _ => unreachable!(),
    }
}

/// Build an I-type ALU immediate `AsmToken` (addi, andi, ori).
fn i_imm_inst(mnemonic: &str, rd: u8, rs1: u8, imm: i32) -> AsmToken {
    match mnemonic {
        "addi" => AsmToken::Real(RealInstruction::Addi(Addi::new(rd, rs1, imm))),
        "andi" => AsmToken::Real(RealInstruction::Andi(Andi::new(rd, rs1, imm))),
        "ori" => AsmToken::Real(RealInstruction::Ori(Ori::new(rd, rs1, imm))),
        _ => unreachable!(),
    }
}

/// Build a shift-immediate `AsmToken` (slli, srli, srai).
fn shift_inst(mnemonic: &str, rd: u8, rs1: u8, shamt: u8) -> AsmToken {
    match mnemonic {
        "slli" => AsmToken::Real(RealInstruction::Slli(Slli::new(rd, rs1, shamt))),
        "srli" => AsmToken::Real(RealInstruction::Srli(Srli::new(rd, rs1, shamt))),
        "srai" => AsmToken::Real(RealInstruction::Srai(Srai::new(rd, rs1, shamt))),
        _ => unreachable!(),
    }
}

// --- `li` immediate expansion ---

/// Try to parse `li rd, imm` and return the expanded instruction tokens.
fn try_parse_li(rest: &str) -> Option<Vec<AsmToken>> {
    let (rd_str, imm_str) = parse_two_fields(rest)?;
    let rd = parse_int_reg(rd_str)?;
    let imm = parse_imm_i64(imm_str)?;
    Some(expand_li(rd, imm))
}

/// Expand `li rd, imm` into the minimal real instruction sequence.
/// Handles 12-bit, 32-bit signed, and full 64-bit immediates.
fn expand_li(rd: u8, imm: i64) -> Vec<AsmToken> {
    let mut out = Vec::new();

    // Emit lui+addi for a signed 32-bit value into `rd`.
    let load32 = |reg: u8, val32: i32, out: &mut Vec<AsmToken>| {
        if (-2048..=2047).contains(&(val32 as i64)) {
            out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
                reg, 0, val32,
            ))));
            return;
        }
        let lo12 = val32 & 0xFFF;
        let lo12_signed = if lo12 >= 0x800 { lo12 - 0x1000 } else { lo12 };
        let hi20 = val32.wrapping_sub(lo12_signed);
        out.push(AsmToken::Real(RealInstruction::Lui(Lui::new(reg, hi20))));
        if lo12_signed != 0 {
            out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
                reg,
                reg,
                lo12_signed,
            ))));
        }
    };

    if (-2048..=2047).contains(&imm) {
        out.push(AsmToken::Real(RealInstruction::Addi(Addi::new(
            rd, 0, imm as i32,
        ))));
        return out;
    }

    if (-2_147_483_648..=2_147_483_647).contains(&imm) {
        load32(rd, imm as i32, &mut out);
        return out;
    }

    // 64-bit: split into high32 and low32, combine with shifts.
    // Uses t1 (x6) as a temporary for the high half.
    const T1: u8 = 6;
    let low32 = (imm & 0xFFFF_FFFF) as i32;
    let high32 = ((imm >> 32) & 0xFFFF_FFFF) as i32;

    load32(T1, high32, &mut out);
    out.push(AsmToken::Real(RealInstruction::Slli(Slli::new(T1, T1, 32))));
    load32(rd, low32, &mut out);
    // Zero-extend low32 into rd (clear sign-extension from load32).
    out.push(AsmToken::Real(RealInstruction::Slli(Slli::new(rd, rd, 32))));
    out.push(AsmToken::Real(RealInstruction::Srli(Srli::new(rd, rd, 32))));
    out.push(AsmToken::Real(RealInstruction::Or(Or::new(rd, rd, T1))));

    out
}

// --- Operand parsers ---

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

/// Parse `"rd, rs"` -> `(rd_reg, rs_reg)`.  Used by `mv`.
fn parse_two_regs(operands: &str) -> Option<(u8, u8)> {
    let (rd_str, rs_str) = parse_two_fields(operands)?;
    Some((parse_int_reg(rd_str)?, parse_int_reg(rs_str)?))
}

/// Parse `"rd, rs1, imm"` -> (`rd_reg`, `rs1_reg`, `imm_i32`).
fn parse_r_i_imm(operands: &str) -> Option<(u8, u8, i32)> {
    let mut parts = operands.splitn(3, ',');
    let rd = parse_int_reg(parts.next()?.trim())?;
    let rs1 = parse_int_reg(parts.next()?.trim())?;
    let imm = parse_imm_i64(parts.next()?.trim())? as i32;
    Some((rd, rs1, imm))
}

/// Parse `"rs2, imm(rs1)"` -> (`rs2_reg`, `rs1_reg`, `imm_i32`).  Used by stores.
fn parse_store_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rs2 = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rs2, rs1, imm))
}

/// Parse `"rd, imm(rs1)"` -> (`rd_reg`, `rs1_reg`, `imm_i32`).  Used by loads.
fn parse_load_mem(operands: &str) -> Option<(u8, u8, i32)> {
    let comma = operands.find(',')?;
    let rd = parse_int_reg(operands[..comma].trim())?;
    let (rs1, imm) = parse_mem_ref(operands[comma + 1..].trim())?;
    Some((rd, rs1, imm))
}

/// Parse `"imm(rs1)"` -> (`rs1_reg`, `imm_i32`).
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

/// Parse `rd, csr, rs1` for the full CSR read-modify-write instructions.
fn parse_csr_rmw(mnemonic: &str, operands: &str) -> Option<AsmToken> {
    let parts: Vec<&str> = operands.splitn(3, ',').collect();
    if parts.len() != 3 {
        return None;
    }
    let rd = parse_int_reg(parts[0].trim())?;
    let csr = parse_csr_name(parts[1].trim())?;
    let rs1 = parse_int_reg(parts[2].trim())?;
    let inst = match mnemonic {
        "csrrw" => RealInstruction::Csrrw(Csrrw::new(rd, csr, rs1)),
        "csrrs" => RealInstruction::Csrrs(Csrrs::new(rd, csr, rs1)),
        "csrrc" => RealInstruction::Csrrc(Csrrc::new(rd, csr, rs1)),
        _ => return None,
    };
    Some(AsmToken::Real(inst))
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
        "mul" => RealInstruction::Mul(Mul::new(rd, rs1, rs2)),
        _ => return None,
    };
    Some(AsmToken::Real(real))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::token::AsmToken;

    // Encode a single instruction line and return its 32-bit word.
    fn encode_line(line: &str) -> u32 {
        let mut out = Vec::new();
        parse_instruction_line(line, &mut out);
        assert_eq!(out.len(), 1, "expected exactly one token for `{line}`");
        match &out[0] {
            AsmToken::Real(inst) => inst.encode(),
            other => panic!("expected a real instruction for `{line}`, got {other:?}"),
        }
    }

    // sscratch is CSR 0x140; SYSTEM opcode is 0x73.
    fn expect_csr_word(rd: u32, rs1: u32, funct3: u32) -> u32 {
        (0x140 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x73
    }

    #[test]
    fn csrrw_full_form_swaps_via_rd_and_rs1() {
        // csrrw t0, sscratch, t1  -> rd=x5, rs1=x6, funct3=1
        assert_eq!(
            encode_line("csrrw t0, sscratch, t1"),
            expect_csr_word(5, 6, 1)
        );
    }

    #[test]
    fn csrrw_swap_same_register() {
        // The kernel trap entry relies on `csrrw sp, sscratch, sp` (rd==rs1==x2).
        assert_eq!(
            encode_line("csrrw sp, sscratch, sp"),
            expect_csr_word(2, 2, 1)
        );
    }

    #[test]
    fn csrrs_and_csrrc_full_forms() {
        assert_eq!(
            encode_line("csrrs t0, sscratch, t1"),
            expect_csr_word(5, 6, 2)
        );
        assert_eq!(
            encode_line("csrrc t0, sscratch, t1"),
            expect_csr_word(5, 6, 3)
        );
    }

    #[test]
    fn mul_encodes_as_rv64m_r_type() {
        // mul t5, t2, t3 -> rd=x30, rs1=x7, rs2=x28; OP opcode 0x33, funct3=0, funct7=0x01.
        let word = encode_line("mul t5, t2, t3");
        assert_eq!(word & 0x7f, 0x33, "opcode");
        assert_eq!((word >> 7) & 0x1f, 30, "rd");
        assert_eq!((word >> 12) & 0x7, 0, "funct3");
        assert_eq!((word >> 15) & 0x1f, 7, "rs1");
        assert_eq!((word >> 20) & 0x1f, 28, "rs2");
        assert_eq!((word >> 25) & 0x7f, 0x01, "funct7 (M extension)");
    }

    #[test]
    fn srai_encodes_with_arithmetic_funct7() {
        // srai a5, a5, 15 -> rd=rs1=x15, shamt=15; OP-IMM 0x13, funct3=5, funct7=0x20.
        let word = encode_line("srai a5, a5, 15");
        assert_eq!(word & 0x7f, 0x13, "opcode");
        assert_eq!((word >> 12) & 0x7, 5, "funct3");
        assert_eq!((word >> 20) & 0x1f, 15, "shamt");
        assert_eq!((word >> 25) & 0x7f, 0x20, "funct7 marks arithmetic shift");
    }
}
