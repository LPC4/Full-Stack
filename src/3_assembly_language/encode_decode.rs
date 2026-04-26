pub type Reg = u8;

pub trait RiscvFormat: Sized {
    fn encode(self) -> u32;
    fn decode(word: u32) -> Self;
}

/// =============================================================================
/// R-Type: Register-Register
/// [funct7:31-25][rs2:24-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RType {
    pub opcode: u8,
    pub rd: Reg,
    pub funct3: u8,
    pub rs1: Reg,
    pub rs2: Reg,
    pub funct7: u8,
}

impl RiscvFormat for RType {
    #[inline]
    fn encode(self) -> u32 {
        ((self.funct7 as u32) << 25)
            | ((self.rs2 as u32) << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | ((self.rd as u32) << 7)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            funct7: ((word >> 25) & 0x7F) as u8,
            rs2: ((word >> 20) & 0x1F) as u8,
            rs1: ((word >> 15) & 0x1F) as u8,
            funct3: ((word >> 12) & 0x7) as u8,
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// I-Type: Register-Immediate / Loads / JALR / CSRs
/// [imm[11:0]:31-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IType {
    pub opcode: u8,
    pub rd: Reg,
    pub funct3: u8,
    pub rs1: Reg,
    pub imm: i32, // 12-bit signed
}

impl RiscvFormat for IType {
    #[inline]
    fn encode(self) -> u32 {
        let imm12 = (self.imm as u32) & 0xFFF;
        (imm12 << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | ((self.rd as u32) << 7)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        let imm12 = ((word >> 20) & 0xFFF) as i32;
        // Sign-extend 12-bit → 32-bit
        let imm = (imm12 << 20) >> 20;
        Self {
            imm,
            rs1: ((word >> 15) & 0x1F) as u8,
            funct3: ((word >> 12) & 0x7) as u8,
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// S-Type: Stores
/// [imm[11:5]:31-25][rs2:24-20][rs1:19-15][funct3:14-12][imm[4:0]:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SType {
    pub opcode: u8,
    pub funct3: u8,
    pub rs1: Reg,
    pub rs2: Reg,
    pub imm: i32, // 12-bit signed
}

impl RiscvFormat for SType {
    #[inline]
    fn encode(self) -> u32 {
        let imm11_5 = (self.imm as u32 >> 5) & 0x7F;
        let imm4_0 = (self.imm as u32) & 0x1F;
        (imm11_5 << 25)
            | ((self.rs2 as u32) << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | (imm4_0 << 7)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        let imm11_5 = ((word >> 25) & 0x7F) as i32;
        let imm4_0 = ((word >> 7) & 0x1F) as i32;
        let imm = (imm11_5 << 5) | imm4_0;
        // Sign-extend 12-bit → 32-bit
        let imm = (imm << 20) >> 20;
        Self {
            imm,
            rs2: ((word >> 20) & 0x1F) as u8,
            rs1: ((word >> 15) & 0x1F) as u8,
            funct3: ((word >> 12) & 0x7) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// B-Type: Conditional Branches
/// [imm[12|10:5]:31-25][rs2:24-20][rs1:19-15][funct3:14-12][imm[4:1|11]:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BType {
    pub opcode: u8,
    pub funct3: u8,
    pub rs1: Reg,
    pub rs2: Reg,
    pub imm: i32, // 13-bit signed byte offset (must be even)
}

impl BType {
    #[inline]
    fn encode_imm(imm: i32) -> u32 {
        debug_assert!(imm & 1 == 0, "B-type byte offset must be even");
        let offset = imm >> 1; // Convert to word offset
        let i12 = ((offset >> 11) & 1) as u32;
        let i10_5 = ((offset >> 5) & 0x3F) as u32;
        let i4_1 = (offset & 0xF) as u32;
        let i11 = ((offset >> 10) & 1) as u32;
        (i12 << 31) | (i10_5 << 25) | (i11 << 7) | (i4_1 << 8)
    }

    #[inline]
    fn decode_imm(word: u32) -> i32 {
        let i12 = ((word >> 31) & 1) as i32;
        let i10_5 = ((word >> 25) & 0x3F) as i32;
        let i4_1 = ((word >> 8) & 0xF) as i32;
        let i11 = ((word >> 7) & 1) as i32;
        let offset = (i12 << 11) | (i10_5 << 5) | (i11 << 10) | i4_1;
        // Sign-extend 13-bit word offset → 32-bit, then convert back to bytes
        (offset << 19) >> 18
    }
}

impl RiscvFormat for BType {
    #[inline]
    fn encode(self) -> u32 {
        Self::encode_imm(self.imm)
            | ((self.rs2 as u32) << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            imm: Self::decode_imm(word),
            rs2: ((word >> 20) & 0x1F) as u8,
            rs1: ((word >> 15) & 0x1F) as u8,
            funct3: ((word >> 12) & 0x7) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// U-Type: Upper Immediate (LUI, AUIPC)
/// [imm[31:12]:31-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UType {
    pub opcode: u8,
    pub rd: Reg,
    pub imm: i32, // Upper 20 bits (imm[31:12])
}

impl RiscvFormat for UType {
    #[inline]
    fn encode(self) -> u32 {
        // Only bits 31-12 are used; lower 12 must be zero
        ((self.imm as u32) & 0xFFFFF000) | ((self.rd as u32) << 7) | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            imm: (word & 0xFFFFF000) as i32,
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// J-Type: Unconditional Jump (JAL)
/// [imm[20|10:1|11|19:12]:31-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JType {
    pub opcode: u8,
    pub rd: Reg,
    pub imm: i32, // 21-bit signed byte offset (must be even)
}

impl JType {
    #[inline]
    fn encode_imm(imm: i32) -> u32 {
        debug_assert!(imm & 1 == 0, "J-type byte offset must be even");
        let offset = imm >> 1; // Convert to word offset
        let i20 = ((offset >> 20) & 1) as u32;
        let i10_1 = ((offset >> 1) & 0x3FF) as u32;
        let i11 = ((offset >> 11) & 1) as u32;
        let i19_12 = ((offset >> 12) & 0xFF) as u32;
        (i20 << 31) | (i10_1 << 21) | (i11 << 20) | (i19_12 << 12)
    }

    #[inline]
    fn decode_imm(word: u32) -> i32 {
        let i20 = ((word >> 31) & 1) as i32;
        let i10_1 = ((word >> 21) & 0x3FF) as i32;
        let i11 = ((word >> 20) & 1) as i32;
        let i19_12 = ((word >> 12) & 0xFF) as i32;
        let offset = (i20 << 20) | (i10_1 << 1) | (i11 << 11) | (i19_12 << 12);
        // Sign-extend 21-bit word offset → 32-bit, then convert back to bytes
        (offset << 11) >> 10
    }
}

impl RiscvFormat for JType {
    #[inline]
    fn encode(self) -> u32 {
        Self::encode_imm(self.imm) | ((self.rd as u32) << 7) | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            imm: Self::decode_imm(word),
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// R4-Type: FP Multiply-Accumulate (FMADD, FMSUB, FNMSUB, FNMADD)
/// [rs3:31-27][fmt:26-25][rs2:24-20][rs1:19-15][rm:14-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct R4Type {
    pub opcode: u8,
    pub rd: Reg,
    pub rm: u8, // 3-bit rounding mode
    pub rs1: Reg,
    pub rs2: Reg,
    pub fmt: u8, // 2-bit format (00=S, 01=D)
    pub rs3: Reg,
}

impl RiscvFormat for R4Type {
    #[inline]
    fn encode(self) -> u32 {
        ((self.rs3 as u32) << 27)
            | ((self.fmt as u32) << 25)
            | ((self.rs2 as u32) << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.rm as u32) << 12)
            | ((self.rd as u32) << 7)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            rs3: ((word >> 27) & 0x1F) as u8,
            fmt: ((word >> 25) & 0x3) as u8,
            rs2: ((word >> 20) & 0x1F) as u8,
            rs1: ((word >> 15) & 0x1F) as u8,
            rm: ((word >> 12) & 0x7) as u8,
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}

/// =============================================================================
/// Atomic-Type: A-Extension (LR, SC, AMO*)
/// [funct5:31-27][aq:26][rl:25][rs2:24-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
/// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicType {
    pub opcode: u8,
    pub rd: Reg,
    pub funct3: u8,
    pub rs1: Reg,
    pub rs2: Reg,
    pub rl: bool, // Release ordering
    pub aq: bool, // Acquire ordering
    pub funct5: u8,
}

impl RiscvFormat for AtomicType {
    #[inline]
    fn encode(self) -> u32 {
        ((self.funct5 as u32) << 27)
            | ((self.aq as u32) << 26)
            | ((self.rl as u32) << 25)
            | ((self.rs2 as u32) << 20)
            | ((self.rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | ((self.rd as u32) << 7)
            | self.opcode as u32
    }

    #[inline]
    fn decode(word: u32) -> Self {
        Self {
            funct5: ((word >> 27) & 0x1F) as u8,
            aq: ((word >> 26) & 1) != 0,
            rl: ((word >> 25) & 1) != 0,
            rs2: ((word >> 20) & 0x1F) as u8,
            rs1: ((word >> 15) & 0x1F) as u8,
            funct3: ((word >> 12) & 0x7) as u8,
            rd: ((word >> 7) & 0x1F) as u8,
            opcode: (word & 0x7F) as u8,
        }
    }
}
