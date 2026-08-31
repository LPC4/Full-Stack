//! RV64F/D - Single- and Double-Precision Floating-Point Extensions.
//!
//! This module defines all real (encodable) FP instructions, plus the standard
//! FP pseudo-instructions.  The pseudo expansions are provided as convenience
//! methods; they are also available through `PseudoInstruction` in `pseudo.rs`
//! for the top-level assembler.

use crate::encode_decode::Reg;
use crate::traits::Instruction;
use crate::utils::reg_name;

// --- Rounding-mode constants ---
pub const RNE: u8 = 0b000; // Round to Nearest, ties to Even
pub const RTZ: u8 = 0b001; // Round towards Zero
pub const RDN: u8 = 0b010; // Round Down (towards -inf)
pub const RUP: u8 = 0b011; // Round Up (towards +inf)
pub const RMM: u8 = 0b100; // Round to Nearest, ties to Max Magnitude
pub const DYN: u8 = 0b111; // Dynamic rounding mode (from fcsr)

fn encode_fp(
    id: riscv_model::InstructionId,
    rd: Reg,
    rs1: Reg,
    rs2: Reg,
    rs3: Reg,
    rounding_mode: u8,
    immediate: i32,
) -> u32 {
    riscv_model::encode_floating(id, rd, rs1, rs2, rs3, rounding_mode, immediate)
        .unwrap_or_else(|error| panic!("{} descriptor error: {error:?}", id.mnemonic()))
}

// --- Loads / Stores ---

/// FP load (single or double).  Opcode = 0x07.
macro_rules! fp_load_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,     // FP destination
            pub base: Reg,   // integer base register
            pub offset: i32, // 12-bit signed byte offset
        }

        impl $name {
            pub fn new(rd: Reg, base: Reg, offset: i32) -> Self {
                assert!(
                    rd < 32,
                    "{}: register f{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    base < 32,
                    "{}: register x{base} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (-2048..=2047).contains(&offset),
                    "{}: offset {offset} out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, base, offset }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.base,
                    0,
                    0,
                    0,
                    self.offset,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}({})",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, true),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fp_load_inst!(Flw);
fp_load_inst!(Fld);

/// FP store (single or double).  Opcode = 0x27.
macro_rules! fp_store_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub base: Reg, // integer base
            pub src: Reg,  // FP source
            pub offset: i32,
        }

        impl $name {
            pub fn new(base: Reg, src: Reg, offset: i32) -> Self {
                assert!(
                    base < 32,
                    "{}: register x{base} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    src < 32,
                    "{}: register f{src} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (-2048..=2047).contains(&offset),
                    "{}: offset {offset} out of range",
                    instruction_mnemonic!($name)
                );
                Self { base, src, offset }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    0,
                    self.base,
                    self.src,
                    0,
                    0,
                    self.offset,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}({})",
                    instruction_mnemonic!($name),
                    reg_name(self.src, true),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fp_store_inst!(Fsw);
fp_store_inst!(Fsd);

// --- FP ALU - generic macro for 2-source FP operations ---

macro_rules! fp_alu_inst {
    ($name:ident, $id:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,
            pub rs1: Reg,
            pub rs2: Reg,
            pub rm: u8, // rounding mode, stored per instance (default RNE)
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg, rs2: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32 && rs2 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($id)
                );
                Self {
                    rd,
                    rs1,
                    rs2,
                    rm: riscv_model::floating_default_rounding_mode(
                        riscv_model::InstructionId::$id,
                    )
                    .expect("floating descriptor"),
                }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($id),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$id,
                    self.rd,
                    self.rs1,
                    self.rs2,
                    0,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{} {}, {}, {}",
                    instruction_mnemonic!($id),
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($id)
            }
        }
    };
}

// Standard ALU ops (RNE default)
fp_alu_inst!(Fadd, FaddS);
fp_alu_inst!(Fsub, FsubS);
fp_alu_inst!(Fmul, FmulS);
fp_alu_inst!(Fdiv, FdivS);
// fsqrt is special (rs2 = 0)
fp_alu_inst!(Fsgnj, FsgnjS);
fp_alu_inst!(Fsgnjn, FsgnjnS);
fp_alu_inst!(Fsgnjx, FsgnjxS);
fp_alu_inst!(Fmin, FminS);
fp_alu_inst!(Fmax, FmaxS);

// Double-precision versions
fp_alu_inst!(FaddD, FaddD);
fp_alu_inst!(FsubD, FsubD);
fp_alu_inst!(FmulD, FmulD);
fp_alu_inst!(FdivD, FdivD);
fp_alu_inst!(FsgnjD, FsgnjD);
fp_alu_inst!(FsgnjnD, FsgnjnD);
fp_alu_inst!(FsgnjxD, FsgnjxD);
fp_alu_inst!(FminD, FminD);
fp_alu_inst!(FmaxD, FmaxD);

// --- FSQRT - single source, rs2 = 0 ---
macro_rules! fsqrt_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,
            pub rs1: Reg,
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($name),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.rs1,
                    0,
                    0,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{} {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fsqrt_inst!(FsqrtS);
fsqrt_inst!(FsqrtD);

// --- FP Compare - funct5 = 10100, rm selects comparison ---
macro_rules! fcmp_inst {
    ($name:ident, $id:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // integer destination
            pub rs1: Reg, // FP source
            pub rs2: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg, rs2: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32 && rs2 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($id)
                );
                Self { rd, rs1, rs2 }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$id,
                    self.rd,
                    self.rs1,
                    self.rs2,
                    0,
                    riscv_model::floating_default_rounding_mode(riscv_model::InstructionId::$id)
                        .expect("floating descriptor"),
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{} {}, {}, {}",
                    instruction_mnemonic!($id),
                    reg_name(self.rd, false), // integer result
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($id)
            }
        }
    };
}

fcmp_inst!(FleqS, FleS);
fcmp_inst!(FltS, FltS);
fcmp_inst!(FeqS, FeqS);
fcmp_inst!(FleqD, FleD);
fcmp_inst!(FltD, FltD);
fcmp_inst!(FeqD, FeqD);

// --- FCLASS - rs2 = 0, funct5 = 11100, rm = 001 ---
macro_rules! fclass_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg, // integer result
            pub rs1: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, rs1 }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.rs1,
                    0,
                    0,
                    1,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{} {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fclass_inst!(FclassS);
fclass_inst!(FclassD);

// --- FP Move (bitwise) - funct5 = 11100 (FP->int) or 11110 (int->FP) ---
macro_rules! fmv_x_f {
    ($name:ident, $dest_fp:literal, $src_fp:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg, // destination (integer if $to_int, else FP)
            pub rs: Reg, // source (FP if $to_int, else integer)
        }

        impl $name {
            pub fn new(rd: Reg, rs: Reg) -> Self {
                assert!(
                    rd < 32 && rs < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, rs }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.rs,
                    0,
                    0,
                    0,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, $dest_fp),
                    reg_name(self.rs, $src_fp),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fmv_x_f!(FmvXW, false, true);
fmv_x_f!(FmvWX, true, false);
fmv_x_f!(FmvXD, false, true);
fmv_x_f!(FmvDX, true, false);

// --- FP Conversions - funct5 indicates direction, rs2 encodes integer type ---

/// FP -> Integer conversion (funct5 = 0b11000)
macro_rules! fcvt_f2i {
    ($name:ident, $id:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // integer destination
            pub rs1: Reg, // FP source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($id)
                );
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($id),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$id,
                    self.rd,
                    self.rs1,
                    0,
                    0,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    instruction_mnemonic!($id),
                    reg_name(self.rd, false),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($id)
            }
        }
    };
}

/// Integer -> FP conversion (funct5 = 0b11010)
macro_rules! fcvt_i2f {
    ($name:ident, $id:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // FP destination
            pub rs1: Reg, // integer source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($id)
                );
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($id),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$id,
                    self.rd,
                    self.rs1,
                    0,
                    0,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    instruction_mnemonic!($id),
                    reg_name(self.rd, true),
                    reg_name(self.rs1, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($id)
            }
        }
    };
}

// FP <-> FP conversions (special funct5, rs2 encodes source format)
macro_rules! fcvt_f2f {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // FP destination
            pub rs1: Reg, // FP source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($name),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.rs1,
                    0,
                    0,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

// Generate all fcvt variants from the specification
// F -> I: from single
fcvt_f2i!(FcvtWS, FcvtWS);
fcvt_f2i!(FcvtWUS, FcvtWuS);
fcvt_f2i!(FcvtLS, FcvtLS);
fcvt_f2i!(FcvtLUS, FcvtLuS);
// F -> I: from double
fcvt_f2i!(FcvtWD, FcvtWD);
fcvt_f2i!(FcvtWUD, FcvtWuD);
fcvt_f2i!(FcvtLD, FcvtLD);
fcvt_f2i!(FcvtLUD, FcvtLuD);

// I -> F: to single
fcvt_i2f!(FcvtSW, FcvtSW);
fcvt_i2f!(FcvtSWU, FcvtSWu);
fcvt_i2f!(FcvtSL, FcvtSL);
fcvt_i2f!(FcvtSLU, FcvtSLu);
// I -> F: to double
fcvt_i2f!(FcvtDW, FcvtDW);
fcvt_i2f!(FcvtDWU, FcvtDWu);
fcvt_i2f!(FcvtDL, FcvtDL);
fcvt_i2f!(FcvtDLU, FcvtDLu);

// F <-> F
fcvt_f2f!(FcvtSD);
fcvt_f2f!(FcvtDS);

// --- FP Fused Multiply-Add / Subtract (R4-type) ---
macro_rules! fmac_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,
            pub rs1: Reg,
            pub rs2: Reg,
            pub rs3: Reg,
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg, rs2: Reg, rs3: Reg) -> Self {
                assert!(
                    rd < 32 && rs1 < 32 && rs2 < 32 && rs3 < 32,
                    "{}: register out of range",
                    instruction_mnemonic!($name)
                );
                Self {
                    rd,
                    rs1,
                    rs2,
                    rs3,
                    rm: RNE,
                }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                assert!(
                    rm <= 4 || rm == 7,
                    "{}: invalid rounding mode {} (must be 0-4 or 7)",
                    instruction_mnemonic!($name),
                    rm
                );
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                encode_fp(
                    riscv_model::InstructionId::$name,
                    self.rd,
                    self.rs1,
                    self.rs2,
                    self.rs3,
                    self.rm,
                    0,
                )
            }

            fn to_asm(&self) -> String {
                format!(
                    "{} {}, {}, {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                    reg_name(self.rs3, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

fmac_inst!(FmaddS);
fmac_inst!(FmaddD);
fmac_inst!(FmsubS);
fmac_inst!(FmsubD);
fmac_inst!(FnmsubS);
fmac_inst!(FnmsubD);
fmac_inst!(FnmaddS);
fmac_inst!(FnmaddD);

// --- FP Pseudo-instructions ---

/// `fmv.s fd, fs` -> `fsgnj.s fd, fs, fs`
pub fn fmv_s(fd: Reg, fs: Reg) -> Fsgnj {
    Fsgnj::new(fd, fs, fs)
}
/// `fmv.d fd, fs` -> `fsgnj.d fd, fs, fs`
pub fn fmv_d(fd: Reg, fs: Reg) -> FsgnjD {
    FsgnjD::new(fd, fs, fs)
}

/// `fneg.s fd, fs` -> `fsgnjn.s fd, fs, fs`
pub fn fneg_s(fd: Reg, fs: Reg) -> Fsgnjn {
    Fsgnjn::new(fd, fs, fs)
}
/// `fneg.d fd, fs` -> `fsgnjn.d fd, fs, fs`
pub fn fneg_d(fd: Reg, fs: Reg) -> FsgnjnD {
    FsgnjnD::new(fd, fs, fs)
}

/// `fabs.s fd, fs` -> `fsgnjx.s fd, fs, fs`
pub fn fabs_s(fd: Reg, fs: Reg) -> Fsgnjx {
    Fsgnjx::new(fd, fs, fs)
}
/// `fabs.d fd, fs` -> `fsgnjx.d fd, fs, fs`
pub fn fabs_d(fd: Reg, fs: Reg) -> FsgnjxD {
    FsgnjxD::new(fd, fs, fs)
}
