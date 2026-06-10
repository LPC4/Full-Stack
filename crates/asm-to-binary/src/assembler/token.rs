/// Fully-typed token produced by the parser and consumed by layout/encode passes.
/// Unlike `RvInstruction`, every variant has no raw strings — unresolved labels are
/// preserved as `String` targets for symbol-table patching.
use super::section::SectionKind;
use crate::encode_decode::Reg;
use crate::real::RealInstruction;

/// Which B-type branch opcode to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
}

impl BranchKind {
    pub fn from_mnemonic(m: &str) -> Option<Self> {
        match m {
            "beq" => Some(Self::Beq),
            "bne" => Some(Self::Bne),
            "blt" => Some(Self::Blt),
            "bge" => Some(Self::Bge),
            "bltu" => Some(Self::Bltu),
            "bgeu" => Some(Self::Bgeu),
            _ => None,
        }
    }
}

/// A single logical assembler token, section-aware and fully typed.
#[derive(Debug, Clone)]
pub enum AsmToken {
    // ---- code ----
    Real(RealInstruction),
    Branch {
        kind: BranchKind,
        rs1: Reg,
        rs2: Reg,
        target: String,
    },
    Jal { rd: Reg, target: String },
    Call { symbol: String },
    Tail { symbol: String },
    La { rd: Reg, symbol: String },

    // ---- structure ----
    Section(SectionKind),
    Label(String),
    Globl(String),

    // ---- data ----
    Align(usize),
    Balign(usize),
    Space(u64),
    DataU8(u8),
    DataU16(u16),
    DataU32(u32),
    DataU64(u64),
    DataAsciz(String),

    // ---- meta ----
    Comment,
}

impl AsmToken {
    /// Fixed byte size contributed to the output section, or None for alignment tokens.
    pub fn fixed_size(&self) -> Option<usize> {
        match self {
            Self::Real(_) | Self::Branch { .. } | Self::Jal { .. } => Some(4),
            Self::Call { .. } | Self::Tail { .. } | Self::La { .. } => Some(8),
            Self::DataU8(_) => Some(1),
            Self::DataU16(_) => Some(2),
            Self::DataU32(_) => Some(4),
            Self::DataU64(_) => Some(8),
            Self::DataAsciz(s) => Some(s.len() + 1),
            Self::Space(n) => Some(*n as usize),
            Self::Align(_) | Self::Balign(_) => None,
            Self::Section(_) | Self::Label(_) | Self::Globl(_) | Self::Comment => Some(0),
        }
    }
}
