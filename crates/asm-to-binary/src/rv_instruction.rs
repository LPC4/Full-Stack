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
