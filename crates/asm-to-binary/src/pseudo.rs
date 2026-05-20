//! Standard RISC-V pseudo-instructions.
//!
//! Each variant stores exactly the information visible at the source level.
//! [`PseudoInstruction::expand`] produces the canonical real-instruction expansion.
//!
//! Integer pseudo-instructions are covered here.  FP pseudos (fmv.s, fneg.d, ...)
//! live in the `rv64fd` module alongside the FP real instructions they expand to.

use super::real::RealInstruction;
use super::riscv::rv64i::{
    Addi, Addiw, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne, Jal, Jalr, Lui, Or, Slli, Slt, Sltiu, Sltu,
    Srli, Sub, Subw, Xori,
};
use crate::encode_decode::Reg;
use crate::riscv::rv64fd;
use crate::utils::reg_name;

#[derive(Debug, Clone)]
pub enum PseudoInstruction {
    /// `nop` -> `addi x0, x0, 0`
    Nop,

    /// `li rd, imm` - Load immediate.
    /// Expands to 1-4 real instructions depending on the value of `imm`.
    Li {
        rd: Reg,
        imm: i64,
    },

    /// `mv rd, rs` -> `addi rd, rs, 0`
    Mv {
        rd: Reg,
        rs: Reg,
    },

    /// `not rd, rs` -> `xori rd, rs, -1`
    Not {
        rd: Reg,
        rs: Reg,
    },

    /// `neg rd, rs` -> `sub rd, x0, rs`
    Neg {
        rd: Reg,
        rs: Reg,
    },

    /// `negw rd, rs` -> `subw rd, x0, rs`
    Negw {
        rd: Reg,
        rs: Reg,
    },

    /// `sext.w rd, rs` -> `addiw rd, rs, 0`
    SextW {
        rd: Reg,
        rs: Reg,
    },

    /// `seqz rd, rs` -> `sltiu rd, rs, 1`
    Seqz {
        rd: Reg,
        rs: Reg,
    },

    /// `snez rd, rs` -> `sltu rd, x0, rs`
    Snez {
        rd: Reg,
        rs: Reg,
    },

    /// `sltz rd, rs` -> `slt rd, rs, x0`
    Sltz {
        rd: Reg,
        rs: Reg,
    },

    /// `sgtz rd, rs` -> `slt rd, x0, rs`
    Sgtz {
        rd: Reg,
        rs: Reg,
    },

    /// `beqz rs, offset` -> `beq rs, x0, offset`
    Beqz {
        rs: Reg,
        offset: i32,
    },

    /// `bnez rs, offset` -> `bne rs, x0, offset`
    Bnez {
        rs: Reg,
        offset: i32,
    },

    /// `blez rs, offset` -> `bge x0, rs, offset`
    Blez {
        rs: Reg,
        offset: i32,
    },

    /// `bgez rs, offset` -> `bge rs, x0, offset`
    Bgez {
        rs: Reg,
        offset: i32,
    },

    /// `bltz rs, offset` -> `blt rs, x0, offset`
    Bltz {
        rs: Reg,
        offset: i32,
    },

    /// `bgtz rs, offset` -> `blt x0, rs, offset`
    Bgtz {
        rs: Reg,
        offset: i32,
    },

    /// `bgt rs1, rs2, offset` -> `blt rs2, rs1, offset`
    Bgt {
        rs1: Reg,
        rs2: Reg,
        offset: i32,
    },

    /// `ble rs1, rs2, offset` -> `bge rs2, rs1, offset`
    Ble {
        rs1: Reg,
        rs2: Reg,
        offset: i32,
    },

    /// `bgtu rs1, rs2, offset` -> `bltu rs2, rs1, offset`
    Bgtu {
        rs1: Reg,
        rs2: Reg,
        offset: i32,
    },

    /// `bleu rs1, rs2, offset` -> `bgeu rs2, rs1, offset`
    Bleu {
        rs1: Reg,
        rs2: Reg,
        offset: i32,
    },

    /// `j offset` -> `jal x0, offset`
    J {
        offset: i32,
    },

    /// `jr rs` -> `jalr x0, 0(rs)`
    Jr {
        rs: Reg,
    },

    /// `ret` -> `jalr x0, 0(ra)`
    Ret,

    /// `call symbol` -> `auipc ra, %pcrel_hi(symbol); jalr ra, %pcrel_lo(symbol)(ra)`
    /// The `hi` and `lo` offsets must be filled in by the linker/assembler
    /// during symbol resolution; they are stored as `0` here.
    Call {
        symbol: String,
    },

    /// `tail symbol` -> `auipc t1, %pcrel_hi(symbol); jalr x0, %pcrel_lo(symbol)(t1)`
    Tail {
        symbol: String,
    },

    /// `la rd, symbol` -> `auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)`
    La {
        rd: Reg,
        symbol: String,
    },

    /// FP Pseudo instructions
    FmvS {
        fd: Reg,
        fs: Reg,
    },
    FmvD {
        fd: Reg,
        fs: Reg,
    },
    FnegS {
        fd: Reg,
        fs: Reg,
    },
    FnegD {
        fd: Reg,
        fs: Reg,
    },
    FabsS {
        fd: Reg,
        fs: Reg,
    },
    FabsD {
        fd: Reg,
        fs: Reg,
    },
}

const X0: Reg = 0;
const RA: Reg = 1;
const T1: Reg = 6;

impl PseudoInstruction {
    /// Expand this pseudo-instruction into a sequence of real instructions.
    ///
    /// Symbol-relative offsets (`call`, `tail`, `la`) produce placeholder
    /// encodings with zero offsets; the caller is responsible for patching
    /// them after symbol resolution.
    pub fn expand(&self) -> Vec<RealInstruction> {
        match self {
            Self::Nop => vec![RealInstruction::Addi(Addi::new(X0, X0, 0))],

            Self::Li { rd, imm } => expand_li(*rd, *imm),

            Self::Mv { rd, rs } => vec![RealInstruction::Addi(Addi::new(*rd, *rs, 0))],

            Self::Not { rd, rs } => vec![RealInstruction::Xori(Xori::new(*rd, *rs, -1))],

            Self::Neg { rd, rs } => vec![RealInstruction::Sub(Sub::new(*rd, X0, *rs))],

            Self::Negw { rd, rs } => vec![RealInstruction::Subw(Subw::new(*rd, X0, *rs))],

            Self::SextW { rd, rs } => vec![RealInstruction::Addiw(Addiw::new(*rd, *rs, 0))],

            Self::Seqz { rd, rs } => vec![RealInstruction::Sltiu(Sltiu::new(*rd, *rs, 1))],

            Self::Snez { rd, rs } => vec![RealInstruction::Sltu(Sltu::new(*rd, X0, *rs))],

            Self::Sltz { rd, rs } => vec![RealInstruction::Slt(Slt::new(*rd, *rs, X0))],

            Self::Sgtz { rd, rs } => vec![RealInstruction::Slt(Slt::new(*rd, X0, *rs))],

            Self::Beqz { rs, offset } => vec![RealInstruction::Beq(Beq::new(*rs, X0, *offset))],

            Self::Bnez { rs, offset } => vec![RealInstruction::Bne(Bne::new(*rs, X0, *offset))],

            Self::Blez { rs, offset } => vec![RealInstruction::Bge(Bge::new(X0, *rs, *offset))],

            Self::Bgez { rs, offset } => vec![RealInstruction::Bge(Bge::new(*rs, X0, *offset))],

            Self::Bltz { rs, offset } => vec![RealInstruction::Blt(Blt::new(*rs, X0, *offset))],

            Self::Bgtz { rs, offset } => vec![RealInstruction::Blt(Blt::new(X0, *rs, *offset))],

            Self::Bgt { rs1, rs2, offset } => {
                vec![RealInstruction::Blt(Blt::new(*rs2, *rs1, *offset))]
            }

            Self::Ble { rs1, rs2, offset } => {
                vec![RealInstruction::Bge(Bge::new(*rs2, *rs1, *offset))]
            }

            Self::Bgtu { rs1, rs2, offset } => {
                vec![RealInstruction::Bltu(Bltu::new(*rs2, *rs1, *offset))]
            }

            Self::Bleu { rs1, rs2, offset } => {
                vec![RealInstruction::Bgeu(Bgeu::new(*rs2, *rs1, *offset))]
            }

            Self::J { offset } => vec![RealInstruction::Jal(Jal::new(X0, *offset))],

            Self::Jr { rs } => vec![RealInstruction::Jalr(Jalr::new(X0, *rs, 0))],

            Self::Ret => vec![RealInstruction::Jalr(Jalr::new(X0, RA, 0))],

            Self::Call { .. } => vec![
                RealInstruction::Auipc(Auipc::new(RA, 0)),
                RealInstruction::Jalr(Jalr::new(RA, RA, 0)),
            ],

            Self::Tail { .. } => vec![
                RealInstruction::Auipc(Auipc::new(T1, 0)),
                RealInstruction::Jalr(Jalr::new(X0, T1, 0)),
            ],

            Self::La { rd, .. } => vec![
                RealInstruction::Auipc(Auipc::new(*rd, 0)),
                RealInstruction::Addi(Addi::new(*rd, *rd, 0)),
            ],
            Self::FmvS { fd, fs } => vec![RealInstruction::Fsgnj(rv64fd::fmv_s(*fd, *fs))],

            Self::FmvD { fd, fs } => vec![RealInstruction::FsgnjD(rv64fd::fmv_d(*fd, *fs))],

            Self::FnegS { fd, fs } => vec![RealInstruction::Fsgnjn(rv64fd::fneg_s(*fd, *fs))],

            Self::FnegD { fd, fs } => vec![RealInstruction::FsgnjnD(rv64fd::fneg_d(*fd, *fs))],

            Self::FabsS { fd, fs } => vec![RealInstruction::Fsgnjx(rv64fd::fabs_s(*fd, *fs))],

            Self::FabsD { fd, fs } => vec![RealInstruction::FsgnjxD(rv64fd::fabs_d(*fd, *fs))],
        }
    }

    /// Assembly source string for this pseudo-instruction.
    pub fn to_asm(&self) -> String {
        match self {
            Self::Nop => "nop".into(),
            Self::Li { rd, imm } => format!("li     {}, {}", reg_name(*rd, false), imm),
            Self::Mv { rd, rs } => {
                format!("mv     {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Not { rd, rs } => {
                format!("not    {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Neg { rd, rs } => {
                format!("neg    {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Negw { rd, rs } => {
                format!("negw   {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::SextW { rd, rs } => {
                format!("sext.w {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Seqz { rd, rs } => {
                format!("seqz   {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Snez { rd, rs } => {
                format!("snez   {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Sltz { rd, rs } => {
                format!("sltz   {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Sgtz { rd, rs } => {
                format!("sgtz   {}, {}", reg_name(*rd, false), reg_name(*rs, false))
            }
            Self::Beqz { rs, offset } => format!("beqz   {}, {}", reg_name(*rs, false), offset),
            Self::Bnez { rs, offset } => format!("bnez   {}, {}", reg_name(*rs, false), offset),
            Self::Blez { rs, offset } => format!("blez   {}, {}", reg_name(*rs, false), offset),
            Self::Bgez { rs, offset } => format!("bgez   {}, {}", reg_name(*rs, false), offset),
            Self::Bltz { rs, offset } => format!("bltz   {}, {}", reg_name(*rs, false), offset),
            Self::Bgtz { rs, offset } => format!("bgtz   {}, {}", reg_name(*rs, false), offset),
            Self::Bgt { rs1, rs2, offset } => format!(
                "bgt    {}, {}, {}",
                reg_name(*rs1, false),
                reg_name(*rs2, false),
                offset
            ),
            Self::Ble { rs1, rs2, offset } => format!(
                "ble    {}, {}, {}",
                reg_name(*rs1, false),
                reg_name(*rs2, false),
                offset
            ),
            Self::Bgtu { rs1, rs2, offset } => format!(
                "bgtu   {}, {}, {}",
                reg_name(*rs1, false),
                reg_name(*rs2, false),
                offset
            ),
            Self::Bleu { rs1, rs2, offset } => format!(
                "bleu   {}, {}, {}",
                reg_name(*rs1, false),
                reg_name(*rs2, false),
                offset
            ),
            Self::J { offset } => format!("j      {offset}"),
            Self::Jr { rs } => format!("jr     {}", reg_name(*rs, false)),
            Self::Ret => "ret".into(),
            Self::Call { symbol } => format!("call   {symbol}"),
            Self::Tail { symbol } => format!("tail   {symbol}"),
            Self::La { rd, symbol } => format!("la     {}, {}", reg_name(*rd, false), symbol),
            Self::FmvS { fd, fs } => {
                format!("fmv.s  {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
            Self::FmvD { fd, fs } => {
                format!("fmv.d  {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
            Self::FnegS { fd, fs } => {
                format!("fneg.s {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
            Self::FnegD { fd, fs } => {
                format!("fneg.d {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
            Self::FabsS { fd, fs } => {
                format!("fabs.s {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
            Self::FabsD { fd, fs } => {
                format!("fabs.d {}, {}", reg_name(*fd, true), reg_name(*fs, true))
            }
        }
    }
}

/// Expands `li rd, imm` into the minimal real instruction sequence.
fn expand_li(rd: Reg, imm: i64) -> Vec<RealInstruction> {
    // 12-bit sign-extended case: addi rd, x0, imm
    if (-2048..=2047).contains(&imm) {
        return vec![RealInstruction::Addi(Addi::new(rd, 0, imm as i32))];
    }

    // 32-bit case: the value must fit into a signed 32-bit word.
    // If so, we can use lui + addi.
    let fits_i32 = (-2_147_483_648..=2_147_483_647).contains(&imm);
    if fits_i32 {
        let imm32 = imm as i32;
        let lo12 = imm32 & 0xFFF; // unsigned lower 12 bits
        let lo12_signed = if lo12 >= 0x800 { lo12 - 0x1000 } else { lo12 };
        // hi20_val is the value whose upper 20 bits are the lui immediate.
        let hi20_val = imm32.wrapping_sub(lo12_signed); // hi20_val is a multiple of 0x1000
        let mut out = vec![RealInstruction::Lui(Lui::new(rd, hi20_val))];
        if lo12_signed != 0 {
            out.push(RealInstruction::Addi(Addi::new(rd, rd, lo12_signed)));
        }
        return out;
    }

    // 64-bit case: construct the value from two 32-bit halves.
    // The expansion uses t1 (x6) as a temporary - it is caller-saved
    let low32 = (imm & 0xFFFF_FFFF) as i32; // as 32-bit signed
    let high32 = ((imm >> 32) & 0xFFFF_FFFF) as i32; // as 32-bit signed

    // Helper to produce the lui+addi sequence for a 32-bit constant
    fn load32(rd: Reg, val32: i32) -> Vec<RealInstruction> {
        let lo12 = val32 & 0xFFF;
        let lo12_signed = if lo12 >= 0x800 { lo12 - 0x1000 } else { lo12 };
        let hi20 = val32.wrapping_sub(lo12_signed);
        let mut seq = vec![RealInstruction::Lui(Lui::new(rd, hi20))];
        if lo12_signed != 0 {
            seq.push(RealInstruction::Addi(Addi::new(rd, rd, lo12_signed)));
        }
        seq
    }

    let mut seq = Vec::new();

    // Load the high 32 bits into t1 and shift them left by 32.
    seq.append(&mut load32(T1, high32)); // T1 = sign_ext(high32)
    seq.push(RealInstruction::Slli(Slli::new(T1, T1, 32))); // T1 = high32 << 32

    // Load the low 32 bits into rd, then zero-extend to 64 bits.
    seq.append(&mut load32(rd, low32)); // rd = sign_ext(low32)
    seq.push(RealInstruction::Slli(Slli::new(rd, rd, 32))); // clear upper 32 bits
    seq.push(RealInstruction::Srli(Srli::new(rd, rd, 32))); // rd = zero_ext(low32)

    // Combine: rd = zero_ext(low32) | (high32 << 32)
    seq.push(RealInstruction::Or(Or::new(rd, rd, T1)));

    seq
}
