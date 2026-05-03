use super::directive::Directive;
use super::reg_parse::parse_int_reg;
use super::section::SectionKind;
use super::token::{AsmToken, BranchKind};
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
        if let Some(tok) = parse_instruction_line(trimmed) {
            out.push(tok);
        } else {
            out.push(AsmToken::Comment(format!(
                "unrecognised instruction: {trimmed}"
            )));
        }
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

/// Try to parse a trimmed instruction line such as `bne a0, a1, .Lelse` or
/// `j .Ltop` into a typed `AsmToken`.
fn parse_instruction_line(line: &str) -> Option<AsmToken> {
    // Split mnemonic from operands
    let (mnemonic, rest) = split_mnemonic(line);

    // Branch instructions: `bne rs1, rs2, label`
    if let Some(kind) = BranchKind::from_mnemonic(mnemonic) {
        return parse_branch(kind, rest);
    }

    // Unconditional jump: `j label`  (pseudo for `jal x0, label`)
    if mnemonic == "j" {
        let target = rest.trim().to_owned();
        return if target.is_empty() {
            None
        } else {
            Some(AsmToken::Jal { rd: 0, target })
        };
    }

    // JAL with destination: `jal rd, label`
    if mnemonic == "jal" {
        return parse_jal(rest);
    }

    // CALL pseudo: `call symbol` -> expands to auipc + jalr
    if mnemonic == "call" {
        let symbol = rest.trim().to_owned();
        return if symbol.is_empty() {
            None
        } else {
            Some(AsmToken::Call { symbol })
        };
    }

    // TAIL pseudo: `tail symbol` -> expands to auipc + jalr (tail call)
    if mnemonic == "tail" {
        let symbol = rest.trim().to_owned();
        return if symbol.is_empty() {
            None
        } else {
            Some(AsmToken::Tail { symbol })
        };
    }

    // LA pseudo: `la rd, symbol` -> expands to auipc + addi
    if mnemonic == "la" {
        return parse_la(rest);
    }

    None
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
