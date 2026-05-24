//! RV64I - base integer instructions.
//!
//! This module defines the concrete instruction structs used throughout the
//! assembler. The reusable macro helpers live in `macros.rs`; here we simply
//! invoke them to generate the RV64I instruction set and add the few special
//! cases that need custom encoding.

use crate::encode_decode::{IType, Reg, RiscvFormat as _};
use crate::traits::Instruction;
use crate::utils::reg_name;
use std::fmt::Debug;

// ---------------------------------------------------------------------------
// Integer register-register instructions
// ---------------------------------------------------------------------------

r_inst!(
    Add,
    opcode = 0x33,
    funct3 = 0,
    funct7 = 0x00,
    mnemonic = "add"
);
r_inst!(
    Sub,
    opcode = 0x33,
    funct3 = 0,
    funct7 = 0x20,
    mnemonic = "sub"
);
r_inst!(
    Sll,
    opcode = 0x33,
    funct3 = 1,
    funct7 = 0x00,
    mnemonic = "sll"
);
r_inst!(
    Slt,
    opcode = 0x33,
    funct3 = 2,
    funct7 = 0x00,
    mnemonic = "slt"
);
r_inst!(
    Sltu,
    opcode = 0x33,
    funct3 = 3,
    funct7 = 0x00,
    mnemonic = "sltu"
);
r_inst!(
    Xor,
    opcode = 0x33,
    funct3 = 4,
    funct7 = 0x00,
    mnemonic = "xor"
);
r_inst!(
    Srl,
    opcode = 0x33,
    funct3 = 5,
    funct7 = 0x00,
    mnemonic = "srl"
);
r_inst!(
    Sra,
    opcode = 0x33,
    funct3 = 5,
    funct7 = 0x20,
    mnemonic = "sra"
);
r_inst!(
    Or,
    opcode = 0x33,
    funct3 = 6,
    funct7 = 0x00,
    mnemonic = "or"
);
r_inst!(
    And,
    opcode = 0x33,
    funct3 = 7,
    funct7 = 0x00,
    mnemonic = "and"
);

r_inst!(
    Addw,
    opcode = 0x3B,
    funct3 = 0,
    funct7 = 0x00,
    mnemonic = "addw"
);
r_inst!(
    Subw,
    opcode = 0x3B,
    funct3 = 0,
    funct7 = 0x20,
    mnemonic = "subw"
);
r_inst!(
    Sllw,
    opcode = 0x3B,
    funct3 = 1,
    funct7 = 0x00,
    mnemonic = "sllw"
);
r_inst!(
    Srlw,
    opcode = 0x3B,
    funct3 = 5,
    funct7 = 0x00,
    mnemonic = "srlw"
);
r_inst!(
    Sraw,
    opcode = 0x3B,
    funct3 = 5,
    funct7 = 0x20,
    mnemonic = "sraw"
);

// ---------------------------------------------------------------------------
// Integer immediate arithmetic / shifts
// ---------------------------------------------------------------------------

i_imm_inst!(Addi, opcode = 0x13, funct3 = 0, mnemonic = "addi");
i_imm_inst!(Slti, opcode = 0x13, funct3 = 2, mnemonic = "slti");
i_imm_inst!(Sltiu, opcode = 0x13, funct3 = 3, mnemonic = "sltiu");
i_imm_inst!(Xori, opcode = 0x13, funct3 = 4, mnemonic = "xori");
i_imm_inst!(Ori, opcode = 0x13, funct3 = 6, mnemonic = "ori");
i_imm_inst!(Andi, opcode = 0x13, funct3 = 7, mnemonic = "andi");

// RV64W immediate instructions use opcode 0x1B.
i_imm_inst!(Addiw, opcode = 0x1B, funct3 = 0, mnemonic = "addiw");

// ---------------------------------------------------------------------------
// Loads / stores / branches / upper-immediate / jumps
// ---------------------------------------------------------------------------

i_load_inst!(Lb, funct3 = 0, mnemonic = "lb");
i_load_inst!(Lh, funct3 = 1, mnemonic = "lh");
i_load_inst!(Lw, funct3 = 2, mnemonic = "lw");
i_load_inst!(Ld, funct3 = 3, mnemonic = "ld");
i_load_inst!(Lbu, funct3 = 4, mnemonic = "lbu");
i_load_inst!(Lhu, funct3 = 5, mnemonic = "lhu");
i_load_inst!(Lwu, funct3 = 6, mnemonic = "lwu");

s_inst!(Sb, funct3 = 0, mnemonic = "sb");
s_inst!(Sh, funct3 = 1, mnemonic = "sh");
s_inst!(Sw, funct3 = 2, mnemonic = "sw");
s_inst!(Sd, funct3 = 3, mnemonic = "sd");

b_inst!(Beq, funct3 = 0, mnemonic = "beq");
b_inst!(Bne, funct3 = 1, mnemonic = "bne");
b_inst!(Blt, funct3 = 4, mnemonic = "blt");
b_inst!(Bge, funct3 = 5, mnemonic = "bge");
b_inst!(Bltu, funct3 = 6, mnemonic = "bltu");
b_inst!(Bgeu, funct3 = 7, mnemonic = "bgeu");

u_inst!(Lui, opcode = 0x37, mnemonic = "lui");
u_inst!(Auipc, opcode = 0x17, mnemonic = "auipc");

j_inst!(Jal, mnemonic = "jal");

// ---------------------------------------------------------------------------
// Special-case instructions
// ---------------------------------------------------------------------------

// Shift immediates
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slli {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8, // 0..63
}
impl Slli {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 63,
            "slli: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Slli {
    fn encode(&self) -> u32 {
        let imm = self.shamt as i32; // funct7 = 0x00
        IType {
            opcode: 0x13,
            rd: self.rd,
            funct3: 1,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "slli  {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "slli"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Srli {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8,
}
impl Srli {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 63,
            "srli: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srli {
    fn encode(&self) -> u32 {
        let imm = self.shamt as i32;
        IType {
            opcode: 0x13,
            rd: self.rd,
            funct3: 5,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "srli  {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "srli"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Srai {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8,
}
impl Srai {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 63,
            "srai: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srai {
    fn encode(&self) -> u32 {
        let imm = (self.shamt as i32) | (0x20i32 << 5); // funct7 = 0x20
        IType {
            opcode: 0x13,
            rd: self.rd,
            funct3: 5,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "srai  {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "srai"
    }
}

// 32-bit word shifts (5-bit shamt, opcode 0x1B)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slliw {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8, // 0..31
}
impl Slliw {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 31,
            "slliw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Slliw {
    fn encode(&self) -> u32 {
        let imm = self.shamt as i32; // funct7 = 0x00
        IType {
            opcode: 0x1B,
            rd: self.rd,
            funct3: 1,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "slliw {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "slliw"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Srliw {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8,
}
impl Srliw {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 31,
            "srliw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srliw {
    fn encode(&self) -> u32 {
        let imm = self.shamt as i32;
        IType {
            opcode: 0x1B,
            rd: self.rd,
            funct3: 5,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "srliw {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "srliw"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sraiw {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8,
}
impl Sraiw {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(
            shamt <= 31,
            "sraiw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Sraiw {
    fn encode(&self) -> u32 {
        let imm = (self.shamt as i32) | (0x20i32 << 5); // funct7 = 0x20
        IType {
            opcode: 0x1B,
            rd: self.rd,
            funct3: 5,
            rs1: self.rs1,
            imm,
        }
        .encode()
    }
    fn to_asm(&self) -> String {
        format!(
            "sraiw {}, {}, {}",
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        "sraiw"
    }
}

/// Jalr
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jalr {
    pub rd: Reg,
    pub rs1: Reg,
    pub imm: i32,
}

impl Jalr {
    pub fn new(rd: Reg, rs1: Reg, imm: i32) -> Self {
        Self { rd, rs1, imm }
    }
}

impl Instruction for Jalr {
    fn encode(&self) -> u32 {
        IType {
            opcode: 0x67,
            rd: self.rd,
            funct3: 0,
            rs1: self.rs1,
            imm: self.imm,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        format!(
            "{:<6} {}, {}({})",
            "jalr",
            reg_name(self.rd, false),
            self.imm,
            reg_name(self.rs1, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        "jalr"
    }
}

/// Ecall
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ecall;

impl Default for Ecall {
    fn default() -> Self {
        Self::new()
    }
}

impl Ecall {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for Ecall {
    fn encode(&self) -> u32 {
        IType {
            opcode: 0x73,
            rd: 0,
            funct3: 0,
            rs1: 0,
            imm: 0x000,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        "ecall".into()
    }

    fn mnemonic(&self) -> &'static str {
        "ecall"
    }
}

/// Ebreak
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ebreak;

impl Default for Ebreak {
    fn default() -> Self {
        Self::new()
    }
}

impl Ebreak {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for Ebreak {
    fn encode(&self) -> u32 {
        IType {
            opcode: 0x73,
            rd: 0,
            funct3: 0,
            rs1: 0,
            imm: 0x001,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        "ebreak".into()
    }

    fn mnemonic(&self) -> &'static str {
        "ebreak"
    }
}

/// Mret - return from machine-mode trap (opcode 0x30200073)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mret;

impl Default for Mret {
    fn default() -> Self {
        Self::new()
    }
}

impl Mret {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for Mret {
    fn encode(&self) -> u32 {
        0x30200073
    }

    fn to_asm(&self) -> String {
        "mret".into()
    }

    fn mnemonic(&self) -> &'static str {
        "mret"
    }
}

/// Sret - return from supervisor-mode trap (opcode 0x10200073)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sret;

impl Default for Sret {
    fn default() -> Self {
        Self::new()
    }
}

impl Sret {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for Sret {
    fn encode(&self) -> u32 {
        0x10200073
    }

    fn to_asm(&self) -> String {
        "sret".into()
    }

    fn mnemonic(&self) -> &'static str {
        "sret"
    }
}

/// SfenceVma - supervisor fence (opcode 0x12000073, sfence.vma x0, x0)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfenceVma;

impl Default for SfenceVma {
    fn default() -> Self {
        Self::new()
    }
}

impl SfenceVma {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for SfenceVma {
    fn encode(&self) -> u32 {
        0x12000073
    }

    fn to_asm(&self) -> String {
        "sfence.vma x0, x0".into()
    }

    fn mnemonic(&self) -> &'static str {
        "sfence.vma"
    }
}

/// WFI - wait for interrupt (system instruction)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wfi;

impl Default for Wfi {
    fn default() -> Self {
        Self::new()
    }
}

impl Wfi {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for Wfi {
    fn encode(&self) -> u32 {
        // Encoding for WFI (SYSTEM) -- instruction immediate 0x105, opcode 0x73
        0x10500073
    }

    fn to_asm(&self) -> String {
        "wfi".into()
    }

    fn mnemonic(&self) -> &'static str {
        "wfi"
    }
}

/// `fence pred, succ` - memory ordering fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence {
    pub fm: u8,
    pub pred: u8,
    pub succ: u8,
}

impl Fence {
    pub fn new(pred: u8, succ: u8) -> Self {
        Self { fm: 0, pred, succ }
    }

    pub fn with_fm(mut self, fm: u8) -> Self {
        self.fm = fm;
        self
    }
}

impl Instruction for Fence {
    fn encode(&self) -> u32 {
        let imm = (((self.fm as i32) & 0xF) << 8)
            | (((self.pred as i32) & 0xF) << 4)
            | ((self.succ as i32) & 0xF);
        IType {
            opcode: 0x0F,
            rd: 0,
            funct3: 0,
            rs1: 0,
            imm,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        fn fence_mask(mask: u8) -> String {
            let mut s = String::new();
            if mask & 0b1000 != 0 {
                s.push('i');
            }
            if mask & 0b0100 != 0 {
                s.push('o');
            }
            if mask & 0b0010 != 0 {
                s.push('r');
            }
            if mask & 0b0001 != 0 {
                s.push('w');
            }
            if s.is_empty() {
                s.push('0');
            }
            s
        }

        if self.fm == 0 {
            format!("fence {}, {}", fence_mask(self.pred), fence_mask(self.succ))
        } else {
            format!(
                "fence {}, {}, fm={:#x}",
                fence_mask(self.pred),
                fence_mask(self.succ),
                self.fm
            )
        }
    }

    fn mnemonic(&self) -> &'static str {
        "fence"
    }
}

/// `fence.i` - synchronize the instruction stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceI;

impl Default for FenceI {
    fn default() -> Self {
        Self::new()
    }
}

impl FenceI {
    pub fn new() -> Self {
        Self
    }
}

impl Instruction for FenceI {
    fn encode(&self) -> u32 {
        IType {
            opcode: 0x0F,
            rd: 0,
            funct3: 1,
            rs1: 0,
            imm: 0,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        "fence.i".into()
    }

    fn mnemonic(&self) -> &'static str {
        "fence.i"
    }
}
