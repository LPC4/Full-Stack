//! RV64I - base integer instructions.
//!
//! This module defines the concrete instruction structs used throughout the
//! assembler. The reusable macro helpers live in `macros.rs`; here we simply
//! invoke them to generate the RV64I instruction set and add the few special
//! cases that need custom encoding.

use crate::encode_decode::Reg;
use crate::traits::Instruction;
use crate::utils::reg_name;
use std::fmt::Debug;

fn encode_immediate(id: riscv_model::InstructionId, rd: Reg, rs1: Reg, immediate: i32) -> u32 {
    let rd = riscv_model::IntReg::new(rd).expect("validated destination register");
    let rs1 = riscv_model::IntReg::new(rs1).expect("validated source register");
    riscv_model::encode_base_integer(
        id,
        riscv_model::DecodedOperands::Immediate { rd, rs1, immediate },
    )
    .unwrap_or_else(|error| panic!("{} descriptor error: {error:?}", id.mnemonic()))
}

fn encode_fixed(id: riscv_model::InstructionId) -> u32 {
    riscv_model::encode_base_integer(id, riscv_model::DecodedOperands::None)
        .unwrap_or_else(|error| panic!("{} descriptor error: {error:?}", id.mnemonic()))
}

// --- Integer register-register instructions ---

r_inst!(Add, mnemonic = "add");
r_inst!(Sub, mnemonic = "sub");
r_inst!(Sll, mnemonic = "sll");
r_inst!(Slt, mnemonic = "slt");
r_inst!(Sltu, mnemonic = "sltu");
r_inst!(Xor, mnemonic = "xor");
r_inst!(Srl, mnemonic = "srl");
r_inst!(Sra, mnemonic = "sra");
r_inst!(Or, mnemonic = "or");
r_inst!(And, mnemonic = "and");

r_inst!(Addw, mnemonic = "addw");
r_inst!(Subw, mnemonic = "subw");
r_inst!(Sllw, mnemonic = "sllw");
r_inst!(Srlw, mnemonic = "srlw");
r_inst!(Sraw, mnemonic = "sraw");

// --- Integer immediate arithmetic / shifts ---

i_imm_inst!(Addi, mnemonic = "addi");
i_imm_inst!(Slti, mnemonic = "slti");
i_imm_inst!(Sltiu, mnemonic = "sltiu");
i_imm_inst!(Xori, mnemonic = "xori");
i_imm_inst!(Ori, mnemonic = "ori");
i_imm_inst!(Andi, mnemonic = "andi");

// RV64W immediate instructions use opcode 0x1B.
i_imm_inst!(Addiw, mnemonic = "addiw");

// --- Loads / stores / branches / upper-immediate / jumps ---

i_load_inst!(Lb, mnemonic = "lb");
i_load_inst!(Lh, mnemonic = "lh");
i_load_inst!(Lw, mnemonic = "lw");
i_load_inst!(Ld, mnemonic = "ld");
i_load_inst!(Lbu, mnemonic = "lbu");
i_load_inst!(Lhu, mnemonic = "lhu");
i_load_inst!(Lwu, mnemonic = "lwu");

s_inst!(Sb, mnemonic = "sb");
s_inst!(Sh, mnemonic = "sh");
s_inst!(Sw, mnemonic = "sw");
s_inst!(Sd, mnemonic = "sd");

b_inst!(Beq, mnemonic = "beq");
b_inst!(Bne, mnemonic = "bne");
b_inst!(Blt, mnemonic = "blt");
b_inst!(Bge, mnemonic = "bge");
b_inst!(Bltu, mnemonic = "bltu");
b_inst!(Bgeu, mnemonic = "bgeu");

u_inst!(Lui, mnemonic = "lui");
u_inst!(Auipc, mnemonic = "auipc");

j_inst!(Jal, mnemonic = "jal");

// --- Special-case instructions ---

// Shift immediates
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slli {
    pub rd: Reg,
    pub rs1: Reg,
    pub shamt: u8, // 0..63
}
impl Slli {
    pub fn new(rd: Reg, rs1: Reg, shamt: u8) -> Self {
        assert!(rd < 32, "slli: register x{rd} is out of range");
        assert!(rs1 < 32, "slli: register x{rs1} is out of range");
        assert!(
            shamt <= 63,
            "slli: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Slli {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Slli,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "srli: register x{rd} is out of range");
        assert!(rs1 < 32, "srli: register x{rs1} is out of range");
        assert!(
            shamt <= 63,
            "srli: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srli {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Srli,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "srai: register x{rd} is out of range");
        assert!(rs1 < 32, "srai: register x{rs1} is out of range");
        assert!(
            shamt <= 63,
            "srai: shift amount {shamt} out of range [0, 63]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srai {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Srai,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "slliw: register x{rd} is out of range");
        assert!(rs1 < 32, "slliw: register x{rs1} is out of range");
        assert!(
            shamt <= 31,
            "slliw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Slliw {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Slliw,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "srliw: register x{rd} is out of range");
        assert!(rs1 < 32, "srliw: register x{rs1} is out of range");
        assert!(
            shamt <= 31,
            "srliw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Srliw {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Srliw,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "sraiw: register x{rd} is out of range");
        assert!(rs1 < 32, "sraiw: register x{rs1} is out of range");
        assert!(
            shamt <= 31,
            "sraiw: shift amount {shamt} out of range [0, 31]"
        );
        Self { rd, rs1, shamt }
    }
}
impl Instruction for Sraiw {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Sraiw,
            self.rd,
            self.rs1,
            i32::from(self.shamt),
        )
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
        assert!(rd < 32, "jalr: register x{rd} is out of range");
        assert!(rs1 < 32, "jalr: register x{rs1} is out of range");
        assert!(
            (-2048..=2047).contains(&imm),
            "jalr: immediate {imm} out of range [-2048, 2047]"
        );
        Self { rd, rs1, imm }
    }
}

impl Instruction for Jalr {
    fn encode(&self) -> u32 {
        encode_immediate(
            riscv_model::InstructionId::Jalr,
            self.rd,
            self.rs1,
            self.imm,
        )
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
        encode_fixed(riscv_model::InstructionId::Ecall)
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
        encode_fixed(riscv_model::InstructionId::Ebreak)
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
        encode_fixed(riscv_model::InstructionId::Mret)
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
        encode_fixed(riscv_model::InstructionId::Sret)
    }

    fn to_asm(&self) -> String {
        "sret".into()
    }

    fn mnemonic(&self) -> &'static str {
        "sret"
    }
}

/// `SfenceVma` - supervisor fence (opcode 0x12000073, sfence.vma x0, x0)
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
        encode_fixed(riscv_model::InstructionId::SfenceVma)
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
        encode_fixed(riscv_model::InstructionId::Wfi)
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
        assert!(
            pred <= 0xf,
            "fence: predecessor mask {pred:#x} is out of range"
        );
        assert!(
            succ <= 0xf,
            "fence: successor mask {succ:#x} is out of range"
        );
        Self { fm: 0, pred, succ }
    }
}

impl Instruction for Fence {
    fn encode(&self) -> u32 {
        riscv_model::encode_base_integer(
            riscv_model::InstructionId::Fence,
            riscv_model::DecodedOperands::Fence {
                fm: self.fm,
                predecessor: self.pred,
                successor: self.succ,
            },
        )
        .unwrap_or_else(|error| panic!("fence descriptor error: {error:?}"))
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
        encode_fixed(riscv_model::InstructionId::FenceI)
    }

    fn to_asm(&self) -> String {
        "fence.i".into()
    }

    fn mnemonic(&self) -> &'static str {
        "fence.i"
    }
}
