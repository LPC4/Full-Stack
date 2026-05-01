//! Instruction decoder for the RV64IMAFD ISA.
//!
//! [`decode`] takes a raw 32-bit instruction word and returns a [`DecodedInsn`]
//! whose variant carries the fully extracted and sign-extended fields, ready for
//! the execute stage. All immediate values are sign-extended to `i64`.

use crate::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FMacOp {
    Fmadd,
    Fmsub,
    Fnmsub,
    Fnmadd,
}

#[derive(Debug, Clone)]
pub enum DecodedInsn {
    Lui    { rd: usize, imm: i64 },
    Auipc  { rd: usize, imm: i64 },
    Jal    { rd: usize, imm: i64 },
    Jalr   { rd: usize, rs1: usize, imm: i64 },
    Branch { funct3: u8, rs1: usize, rs2: usize, imm: i64 },
    Load   { funct3: u8, rd: usize, rs1: usize, imm: i64 },
    Store  { funct3: u8, rs1: usize, rs2: usize, imm: i64 },
    AluImm   { funct3: u8, funct7: u8, rd: usize, rs1: usize, imm: i64 },
    AluImm32 { funct3: u8, funct7: u8, rd: usize, rs1: usize, imm: i64 },
    Alu      { funct3: u8, funct7: u8, rd: usize, rs1: usize, rs2: usize },
    Alu32    { funct3: u8, funct7: u8, rd: usize, rs1: usize, rs2: usize },
    Fence  { fm: u8, pred: u8, succ: u8 },
    FenceI,
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Csr    { funct3: u8, rd: usize, rs1_uimm: usize, csr: u16 },
    FLoad  { funct3: u8, rd: usize, rs1: usize, imm: i64 },
    FStore { funct3: u8, rs1: usize, rs2: usize, imm: i64 },
    FOp    { funct5: u8, fmt: u8, rm: u8, rd: usize, rs1: usize, rs2: usize },
    FMac   { op: FMacOp, fmt: u8, rm: u8, rd: usize, rs1: usize, rs2: usize, rs3: usize },
    Atomic { funct5: u8, aq: bool, rl: bool, funct3: u8, rd: usize, rs1: usize, rs2: usize },
}

// ---------------------------------------------------------------------------
// Public decode entry point
// ---------------------------------------------------------------------------

pub fn decode(word: u32) -> Result<DecodedInsn, VmError> {
    let opcode = word & 0x7F;
    match opcode {
        0x37 => Ok(DecodedInsn::Lui   { rd: rd(word), imm: u_imm(word) }),
        0x17 => Ok(DecodedInsn::Auipc { rd: rd(word), imm: u_imm(word) }),
        0x6F => Ok(DecodedInsn::Jal   { rd: rd(word), imm: j_imm(word) }),
        0x67 => Ok(DecodedInsn::Jalr  {
            rd:  rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x63 => Ok(DecodedInsn::Branch {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: b_imm(word),
        }),
        0x03 => Ok(DecodedInsn::Load {
            funct3: funct3(word),
            rd:  rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x23 => Ok(DecodedInsn::Store {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: s_imm(word),
        }),
        // OP-IMM: pass the full bits[31:25] as funct7 so the execute stage can
        // disambiguate SRLI vs SRAI using bit 30 (and check for illegal encodings).
        0x13 => Ok(DecodedInsn::AluImm {
            funct3: funct3(word),
            funct7: field(word, 31, 25) as u8,
            rd:  rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x1B => Ok(DecodedInsn::AluImm32 {
            funct3: funct3(word),
            funct7: field(word, 31, 25) as u8,
            rd:  rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x33 => Ok(DecodedInsn::Alu {
            funct3: funct3(word),
            funct7: funct7(word),
            rd:  rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        0x3B => Ok(DecodedInsn::Alu32 {
            funct3: funct3(word),
            funct7: funct7(word),
            rd:  rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        0x0F => decode_fence(word),
        0x73 => decode_system(word),
        0x07 => Ok(DecodedInsn::FLoad {
            funct3: funct3(word),
            rd:  rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x27 => Ok(DecodedInsn::FStore {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: s_imm(word),
        }),
        0x53 => Ok(DecodedInsn::FOp {
            funct5: field(word, 31, 27) as u8,
            fmt:    field(word, 26, 25) as u8,
            rm:     funct3(word),
            rd:     rd(word),
            rs1:    rs1(word),
            rs2:    rs2(word),
        }),
        0x43 => Ok(fmac(word, FMacOp::Fmadd)),
        0x47 => Ok(fmac(word, FMacOp::Fmsub)),
        0x4B => Ok(fmac(word, FMacOp::Fnmsub)),
        0x4F => Ok(fmac(word, FMacOp::Fnmadd)),
        0x2F => Ok(DecodedInsn::Atomic {
            funct5: field(word, 31, 27) as u8,
            aq:     field(word, 26, 26) != 0,
            rl:     field(word, 25, 25) != 0,
            funct3: funct3(word),
            rd:     rd(word),
            rs1:    rs1(word),
            rs2:    rs2(word),
        }),
        _ => Err(VmError::IllegalInstruction(word)),
    }
}

// ---------------------------------------------------------------------------
// Sub-decoders
// ---------------------------------------------------------------------------

fn decode_fence(word: u32) -> Result<DecodedInsn, VmError> {
    match funct3(word) {
        0 => Ok(DecodedInsn::Fence {
            fm:   field(word, 31, 28) as u8,
            pred: field(word, 27, 24) as u8,
            succ: field(word, 23, 20) as u8,
        }),
        1 => Ok(DecodedInsn::FenceI),
        _ => Err(VmError::IllegalInstruction(word)),
    }
}

fn decode_system(word: u32) -> Result<DecodedInsn, VmError> {
    // Check for SFENCE.VMA first (opcode 0x73, funct3=0, but with specific pattern)
    // SFENCE.VMA: imm[11:0]=0b0001_0001_0000, rs1 and rs2 can be non-zero
    if funct3(word) == 0 {
        let imm = field(word, 31, 20);
        match imm {
            0 => Ok(DecodedInsn::Ecall),
            1 => Ok(DecodedInsn::Ebreak),
            0x302 => Ok(DecodedInsn::Mret),
            0x102 => Ok(DecodedInsn::Sret),
            0x120 => Ok(DecodedInsn::SfenceVma), // SFENCE.VMA
            _ => Err(VmError::IllegalInstruction(word)),
        }
    } else {
        // CSR instructions
        Ok(DecodedInsn::Csr {
            funct3:   funct3(word),
            rd:       rd(word),
            rs1_uimm: rs1(word), // for CSRRWI/CSRRSI/CSRRCI this is the uimm[4:0]
            csr:      field(word, 31, 20) as u16,
        })
    }
}

fn fmac(word: u32, op: FMacOp) -> DecodedInsn {
    DecodedInsn::FMac {
        op,
        rs3: field(word, 31, 27) as usize,
        fmt: field(word, 26, 25) as u8,
        rm:  funct3(word),
        rd:  rd(word),
        rs1: rs1(word),
        rs2: rs2(word),
    }
}

// ---------------------------------------------------------------------------
// Field extraction helpers
// ---------------------------------------------------------------------------

/// Extract bits [hi..lo] (inclusive) from `word`.
fn field(word: u32, hi: u32, lo: u32) -> u32 {
    let width = hi - lo + 1;
    if width >= 32 {
        word
    } else {
        (word >> lo) & ((1u32 << width) - 1)
    }
}

#[inline] fn rd    (word: u32) -> usize { field(word, 11,  7) as usize }
#[inline] fn rs1   (word: u32) -> usize { field(word, 19, 15) as usize }
#[inline] fn rs2   (word: u32) -> usize { field(word, 24, 20) as usize }
#[inline] fn funct3(word: u32) -> u8    { field(word, 14, 12) as u8 }
#[inline] fn funct7(word: u32) -> u8    { field(word, 31, 25) as u8 }

/// Sign-extend `val` (a value in the low `bits` bits) to `i64`.
fn sign_ext(val: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    ((val << shift) as i32 >> shift) as i64
}

/// I-type immediate: bits[31:20], sign-extended to 64 bits.
fn i_imm(word: u32) -> i64 {
    sign_ext(field(word, 31, 20), 12)
}

/// U-type immediate: upper 20 bits placed at [31:12], lower 12 bits zero.
/// We sign-extend from the 32-bit word so that large addresses work correctly.
fn u_imm(word: u32) -> i64 {
    (word & 0xFFFF_F000) as i32 as i64
}

/// S-type immediate: bits[31:25] || bits[11:7], sign-extended to 12 bits.
fn s_imm(word: u32) -> i64 {
    let hi = field(word, 31, 25);
    let lo = field(word, 11,  7);
    sign_ext((hi << 5) | lo, 12)
}

/// B-type immediate: [31]<<12 | [7]<<11 | [30:25]<<5 | [11:8]<<1, sign-extended to 13 bits.
fn b_imm(word: u32) -> i64 {
    let bit12  = field(word, 31, 31) << 12;
    let bit11  = field(word,  7,  7) << 11;
    let bits10_5 = field(word, 30, 25) << 5;
    let bits4_1  = field(word, 11,  8) << 1;
    sign_ext(bit12 | bit11 | bits10_5 | bits4_1, 13)
}

/// J-type immediate: [31]<<20 | [19:12]<<12 | [20]<<11 | [30:21]<<1, sign-extended to 21 bits.
fn j_imm(word: u32) -> i64 {
    let bit20    = field(word, 31, 31) << 20;
    let bits19_12 = field(word, 19, 12) << 12;
    let bit11    = field(word, 20, 20) << 11;
    let bits10_1 = field(word, 30, 21) << 1;
    sign_ext(bit20 | bits19_12 | bit11 | bits10_1, 21)
}
