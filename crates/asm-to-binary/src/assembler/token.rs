use super::section::SectionKind;
/// Internal token type produced by the parser and consumed by the layout/encode passes.
///
/// Unlike `RvInstruction`, every variant here is fully typed, there are no raw
/// string blobs.  Unresolved label references are preserved as `String` targets
/// so the encode pass can patch them using the symbol table built in the layout pass.
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
    /// An already-encoded real machine instruction (no unresolved references).
    Real(RealInstruction),

    /// A B-type branch whose target is a label name, resolved in the encode pass.
    Branch {
        kind: BranchKind,
        rs1: Reg,
        rs2: Reg,
        /// Label name the branch jumps to.
        target: String,
    },

    /// JAL whose target is a label name (rd = 0 encodes as the `j` pseudo).
    Jal {
        rd: Reg,
        target: String,
    },

    /// CALL pseudo: `auipc ra, %pcrel_hi(symbol); jalr ra, %pcrel_lo(ra)`
    Call {
        symbol: String,
    },

    /// TAIL pseudo: `auipc t1, %pcrel_hi(symbol); jalr x0, %pcrel_lo(t1)`
    Tail {
        symbol: String,
    },

    /// LA pseudo: `auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)`
    La {
        rd: Reg,
        symbol: String,
    },

    // ---- structure ----
    /// Switch the active section.
    Section(SectionKind),

    /// Define a label at the current position.
    Label(String),

    /// Mark a symbol as globally exported (`.globl`).
    Globl(String),

    // ---- data ----
    /// Align to 2^n bytes (`.align n`).
    Align(usize),

    /// Align to exactly n bytes (`.balign n`).
    Balign(usize),

    /// Zero-fill n bytes (`.space` / `.zero`).
    Space(u64),

    DataU8(u8),
    DataU16(u16),
    DataU32(u32),
    DataU64(u64),

    /// Null-terminated byte string (`.asciz`/`.string`).  The null byte is
    /// appended during encoding so callers supply only the string content.
    DataAsciz(String),

    // ---- meta ----
    Comment,
}

impl AsmToken {
    /// Fixed byte size contributed to the output section.
    /// Returns `None` for tokens whose size depends on current alignment offset
    /// (Align, Balign), the layout pass handles those specially.
    pub fn fixed_size(&self) -> Option<usize> {
        match self {
            Self::Real(_) | Self::Branch { .. } | Self::Jal { .. } => Some(4),
            // Call, Tail, and La expand to 2 instructions (8 bytes)
            Self::Call { .. } | Self::Tail { .. } | Self::La { .. } => Some(8),
            Self::DataU8(_) => Some(1),
            Self::DataU16(_) => Some(2),
            Self::DataU32(_) => Some(4),
            Self::DataU64(_) => Some(8),
            Self::DataAsciz(s) => Some(s.len() + 1), // +1 for null terminator
            Self::Space(n) => Some(*n as usize),
            // Size depends on current offset, layout pass computes these
            Self::Align(_) | Self::Balign(_) => None,
            // No bytes
            Self::Section(_) | Self::Label(_) | Self::Globl(_) | Self::Comment => Some(0),
        }
    }
}
