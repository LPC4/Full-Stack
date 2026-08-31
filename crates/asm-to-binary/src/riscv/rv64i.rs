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

r_inst!(Add);
r_inst!(Sub);
r_inst!(Sll);
r_inst!(Slt);
r_inst!(Sltu);
r_inst!(Xor);
r_inst!(Srl);
r_inst!(Sra);
r_inst!(Or);
r_inst!(And);

r_inst!(Addw);
r_inst!(Subw);
r_inst!(Sllw);
r_inst!(Srlw);
r_inst!(Sraw);

// --- Integer immediate arithmetic / shifts ---

i_imm_inst!(Addi);
i_imm_inst!(Slti);
i_imm_inst!(Sltiu);
i_imm_inst!(Xori);
i_imm_inst!(Ori);
i_imm_inst!(Andi);

// RV64W immediate instructions use opcode 0x1B.
i_imm_inst!(Addiw);

// --- Loads / stores / branches / upper-immediate / jumps ---

i_load_inst!(Lb);
i_load_inst!(Lh);
i_load_inst!(Lw);
i_load_inst!(Ld);
i_load_inst!(Lbu);
i_load_inst!(Lhu);
i_load_inst!(Lwu);

s_inst!(Sb);
s_inst!(Sh);
s_inst!(Sw);
s_inst!(Sd);

b_inst!(Beq);
b_inst!(Bne);
b_inst!(Blt);
b_inst!(Bge);
b_inst!(Bltu);
b_inst!(Bgeu);

u_inst!(Lui);
u_inst!(Auipc);

j_inst!(Jal);

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
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Slli)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Slli)
        );
        assert!(
            shamt <= 63,
            "{}: shift amount {shamt} out of range [0, 63]",
            instruction_mnemonic!(Slli)
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
            "{}  {}, {}, {}",
            instruction_mnemonic!(Slli),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Slli)
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
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Srli)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Srli)
        );
        assert!(
            shamt <= 63,
            "{}: shift amount {shamt} out of range [0, 63]",
            instruction_mnemonic!(Srli)
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
            "{}  {}, {}, {}",
            instruction_mnemonic!(Srli),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Srli)
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
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Srai)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Srai)
        );
        assert!(
            shamt <= 63,
            "{}: shift amount {shamt} out of range [0, 63]",
            instruction_mnemonic!(Srai)
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
            "{}  {}, {}, {}",
            instruction_mnemonic!(Srai),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Srai)
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
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Slliw)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Slliw)
        );
        assert!(
            shamt <= 31,
            "{}: shift amount {shamt} out of range [0, 31]",
            instruction_mnemonic!(Slliw)
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
            "{} {}, {}, {}",
            instruction_mnemonic!(Slliw),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Slliw)
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
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Srliw)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Srliw)
        );
        assert!(
            shamt <= 31,
            "{}: shift amount {shamt} out of range [0, 31]",
            instruction_mnemonic!(Srliw)
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
            "{} {}, {}, {}",
            instruction_mnemonic!(Srliw),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Srliw)
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
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Sraiw)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Sraiw)
        );
        assert!(
            shamt <= 31,
            "{}: shift amount {shamt} out of range [0, 31]",
            instruction_mnemonic!(Sraiw)
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
            "{} {}, {}, {}",
            instruction_mnemonic!(Sraiw),
            reg_name(self.rd, false),
            reg_name(self.rs1, false),
            self.shamt
        )
    }
    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Sraiw)
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
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(Jalr)
        );
        assert!(
            rs1 < 32,
            "{}: register x{rs1} is out of range",
            instruction_mnemonic!(Jalr)
        );
        assert!(
            (-2048..=2047).contains(&imm),
            "{}: immediate {imm} out of range [-2048, 2047]",
            instruction_mnemonic!(Jalr)
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
            instruction_mnemonic!(Jalr),
            reg_name(self.rd, false),
            self.imm,
            reg_name(self.rs1, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Jalr)
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
        instruction_mnemonic!(Ecall).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Ecall)
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
        instruction_mnemonic!(Ebreak).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Ebreak)
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
        instruction_mnemonic!(Mret).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Mret)
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
        instruction_mnemonic!(Sret).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Sret)
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
        format!("{} x0, x0", instruction_mnemonic!(SfenceVma))
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(SfenceVma)
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
        instruction_mnemonic!(Wfi).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Wfi)
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
            "{}: predecessor mask {pred:#x} is out of range",
            instruction_mnemonic!(Fence)
        );
        assert!(
            succ <= 0xf,
            "{}: successor mask {succ:#x} is out of range",
            instruction_mnemonic!(Fence)
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
        .unwrap_or_else(|error| {
            panic!(
                "{} descriptor error: {error:?}",
                instruction_mnemonic!(Fence)
            )
        })
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
            format!(
                "{} {}, {}",
                instruction_mnemonic!(Fence),
                fence_mask(self.pred),
                fence_mask(self.succ)
            )
        } else {
            format!(
                "{} {}, {}, fm={:#x}",
                instruction_mnemonic!(Fence),
                fence_mask(self.pred),
                fence_mask(self.succ),
                self.fm
            )
        }
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(Fence)
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
        instruction_mnemonic!(FenceI).into()
    }

    fn mnemonic(&self) -> &'static str {
        instruction_mnemonic!(FenceI)
    }
}
