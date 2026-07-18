//! Top-level instruction representation used by the assembler.
//!
//! An `RvInstruction` is one line in a RISC-V assembly source file.
//! It can be a real machine instruction, a pseudo-instruction, a label,
//! a comment, or an assembler directive.

use super::pseudo::PseudoInstruction;
use super::real::RealInstruction;
use std::fmt;

/// One logical line of RISC-V assembly.
#[derive(Debug, Clone)]
pub enum RvInstruction {
    /// A real, encodable machine instruction.
    Real(RealInstruction),
    /// A pseudo-instruction (expands to one or more real instructions).
    Pseudo(PseudoInstruction),
    /// A label definition, e.g. `main:`.
    Label(String),
    /// A line comment, e.g. `; this does X`.
    Comment(String),
    /// An assembler directive, e.g. `.text`, `.globl main`, `.word 42`.
    Directive(String),
    /// A source-location marker for the pc-to-source line table. Emits no code
    /// and no assembly text; the encoder attributes following instructions to it.
    SourceLoc { file: String, line: u32 },
}

impl RvInstruction {
    pub fn encode_words(&self) -> Vec<u32> {
        match self {
            Self::Real(r) => vec![r.encode()],
            Self::Pseudo(p) => p.expand().iter().map(|r| r.encode()).collect(),
            Self::Label(_) | Self::Comment(_) | Self::Directive(_) | Self::SourceLoc { .. } => {
                vec![]
            }
        }
    }

    /// Returns `true` if this line contributes machine code bytes.
    pub fn is_code(&self) -> bool {
        matches!(self, Self::Real(_) | Self::Pseudo(_))
    }
}

/// Parse raw assembly text into labels/directives/instruction lines. With `file`
/// set, instruction lines get `SourceLoc` markers for the pc-to-source table.
pub fn parse_asm_source(file: Option<&str>, src: &str) -> Vec<RvInstruction> {
    let mut out = Vec::new();
    for (index, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_suffix(':')
            && !name.is_empty()
            && !name.contains(|c: char| c.is_whitespace())
        {
            out.push(RvInstruction::Label(name.to_owned()));
            continue;
        }
        if line.starts_with('.') {
            out.push(RvInstruction::Directive(line.to_owned()));
            continue;
        }
        if let Some(file) = file {
            out.push(RvInstruction::SourceLoc {
                file: file.to_owned(),
                line: (index + 1) as u32,
            });
        }
        // Leading tab marks an instruction line for the assembler's text parser.
        out.push(RvInstruction::Directive(format!("\t{line}")));
    }
    out
}

impl fmt::Display for RvInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Real(r) => write!(f, "\t{}", r.to_asm()),
            Self::Pseudo(p) => write!(f, "\t{}", p.to_asm()),
            Self::Label(l) => write!(f, "{l}:"),
            Self::Comment(c) => write!(f, "; {c}"),
            Self::Directive(d) => write!(f, "{d}"),
            // No textual form: line markers never appear in emitted assembly.
            Self::SourceLoc { .. } => Ok(()),
        }
    }
}
