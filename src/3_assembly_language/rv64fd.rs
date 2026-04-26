//! RV64F/D - Single‑ and Double‑Precision Floating‑Point Extensions.
//!
//! This module defines all real (encodable) FP instructions, plus the standard
//! FP pseudo‑instructions.  The pseudo expansions are provided as convenience
//! methods; they are also available through `PseudoInstruction` in `pseudo.rs`
//! for the top‑level assembler.

use crate::assembly_language::encode_decode::{IType, R4Type, RType, Reg, RiscvFormat, SType};
use crate::assembly_language::traits::Instruction;
use crate::assembly_language::utils::reg_name;

// ---------------------------------------------------------------------------
// Rounding‑mode constants
// ---------------------------------------------------------------------------
pub const RNE: u8 = 0b000; // Round to Nearest, ties to Even
pub const RTZ: u8 = 0b001; // Round towards Zero
pub const RDN: u8 = 0b010; // Round Down (towards -∞)
pub const RUP: u8 = 0b011; // Round Up (towards +∞)
pub const RMM: u8 = 0b100; // Round to Nearest, ties to Max Magnitude
pub const DYN: u8 = 0b111; // Dynamic rounding mode (from fcsr)

// Format field for R/R4 instructions
const FMT_S: u8 = 0; // single
const FMT_D: u8 = 1; // double

// FP opcodes
const OP_LOAD_FP: u8 = 0x07;
const OP_STORE_FP: u8 = 0x27;
const OP_FP: u8 = 0x53; // OP-FP (R-type)
const OP_FMADD: u8 = 0x43;
const OP_FMSUB: u8 = 0x47;
const OP_FNMSUB: u8 = 0x4B;
const OP_FNMADD: u8 = 0x4F;

// ---------------------------------------------------------------------------
// Loads / Stores
// ---------------------------------------------------------------------------

/// FP load (single or double).  Opcode = 0x07.
macro_rules! fp_load_inst {
    ($name:ident, $funct3:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,     // FP destination
            pub base: Reg,   // integer base register
            pub offset: i32, // 12‑bit signed byte offset
        }

        impl $name {
            pub fn new(rd: Reg, base: Reg, offset: i32) -> Self {
                Self { rd, base, offset }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                IType {
                    opcode: OP_LOAD_FP,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.base,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}({})",
                    $mnem,
                    reg_name(self.rd, true),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

fp_load_inst!(Flw, 2, "flw");
fp_load_inst!(Fld, 3, "fld");

/// FP store (single or double).  Opcode = 0x27.
macro_rules! fp_store_inst {
    ($name:ident, $funct3:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub base: Reg, // integer base
            pub src: Reg,  // FP source
            pub offset: i32,
        }

        impl $name {
            pub fn new(base: Reg, src: Reg, offset: i32) -> Self {
                Self { base, src, offset }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                SType {
                    opcode: OP_STORE_FP,
                    funct3: $funct3,
                    rs1: self.base,
                    rs2: self.src,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}({})",
                    $mnem,
                    reg_name(self.src, true),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

fp_store_inst!(Fsw, 2, "fsw");
fp_store_inst!(Fsd, 3, "fsd");

// ---------------------------------------------------------------------------
// FP ALU – generic macro for 2‑source FP operations
// ---------------------------------------------------------------------------

macro_rules! fp_alu_inst {
    ($name:ident, $funct5:expr, $fmt:expr, $rm:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,
            pub rs1: Reg,
            pub rs2: Reg,
            pub rm: u8, // rounding mode, stored per instance (default RNE)
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg, rs2: Reg) -> Self {
                Self {
                    rd,
                    rs1,
                    rs2,
                    rm: $rm,
                }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: self.rm, // RM field is in funct3 position for OP-FP
                    rs1: self.rs1,
                    rs2: self.rs2,
                    funct7: ($funct5 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let fmt_char = if $fmt == FMT_S { 's' } else { 'd' };
                format!(
                    "{}.{} {}, {}, {}",
                    $mnem,
                    fmt_char,
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

// Standard ALU ops (RNE default)
fp_alu_inst!(Fadd, 0b00000, FMT_S, RNE, "fadd");
fp_alu_inst!(Fsub, 0b00001, FMT_S, RNE, "fsub");
fp_alu_inst!(Fmul, 0b00010, FMT_S, RNE, "fmul");
fp_alu_inst!(Fdiv, 0b00011, FMT_S, RNE, "fdiv");
// fsqrt is special (rs2 = 0)
fp_alu_inst!(Fsgnj, 0b00100, FMT_S, 0b000, "fsgnj");
fp_alu_inst!(Fsgnjn, 0b00100, FMT_S, 0b001, "fsgnjn");
fp_alu_inst!(Fsgnjx, 0b00100, FMT_S, 0b010, "fsgnjx");
fp_alu_inst!(Fmin, 0b00101, FMT_S, 0b000, "fmin");
fp_alu_inst!(Fmax, 0b00101, FMT_S, 0b001, "fmax");

// Double‑precision versions
fp_alu_inst!(FaddD, 0b00000, FMT_D, RNE, "fadd");
fp_alu_inst!(FsubD, 0b00001, FMT_D, RNE, "fsub");
fp_alu_inst!(FmulD, 0b00010, FMT_D, RNE, "fmul");
fp_alu_inst!(FdivD, 0b00011, FMT_D, RNE, "fdiv");
fp_alu_inst!(FsgnjD, 0b00100, FMT_D, 0b000, "fsgnj");
fp_alu_inst!(FsgnjnD, 0b00100, FMT_D, 0b001, "fsgnjn");
fp_alu_inst!(FsgnjxD, 0b00100, FMT_D, 0b010, "fsgnjx");
fp_alu_inst!(FminD, 0b00101, FMT_D, 0b000, "fmin");
fp_alu_inst!(FmaxD, 0b00101, FMT_D, 0b001, "fmax");

// ---------------------------------------------------------------------------
// FSQRT – single source, rs2 = 0
// ---------------------------------------------------------------------------
macro_rules! fsqrt_inst {
    ($name:ident, $fmt:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,
            pub rs1: Reg,
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: self.rm,
                    rs1: self.rs1,
                    rs2: 0, // must be x0 / f0
                    funct7: (0b01011 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let fmt_char = if $fmt == FMT_S { 's' } else { 'd' };
                format!(
                    "fsqrt.{} {}, {}",
                    fmt_char,
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                "fsqrt"
            }
        }
    };
}

fsqrt_inst!(FsqrtS, FMT_S, "fsqrt");
fsqrt_inst!(FsqrtD, FMT_D, "fsqrt");

// ---------------------------------------------------------------------------
// FP Compare – funct5 = 10100, rm selects comparison
// ---------------------------------------------------------------------------
macro_rules! fcmp_inst {
    ($name:ident, $fmt:expr, $rm:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // integer destination
            pub rs1: Reg, // FP source
            pub rs2: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg, rs2: Reg) -> Self {
                Self { rd, rs1, rs2 }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: $rm,
                    rs1: self.rs1,
                    rs2: self.rs2,
                    funct7: (0b10100 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let fmt_char = if $fmt == FMT_S { 's' } else { 'd' };
                format!(
                    "{}.{} {}, {}, {}",
                    $mnem,
                    fmt_char,
                    reg_name(self.rd, false), // integer result
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

fcmp_inst!(FleqS, FMT_S, 0b000, "fle");
fcmp_inst!(FltS, FMT_S, 0b001, "flt");
fcmp_inst!(FeqS, FMT_S, 0b010, "feq");
fcmp_inst!(FleqD, FMT_D, 0b000, "fle");
fcmp_inst!(FltD, FMT_D, 0b001, "flt");
fcmp_inst!(FeqD, FMT_D, 0b010, "feq");

// ---------------------------------------------------------------------------
// FCLASS – rs2 = 0, funct5 = 11100, rm = 001
// ---------------------------------------------------------------------------
macro_rules! fclass_inst {
    ($name:ident, $fmt:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg, // integer result
            pub rs1: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                Self { rd, rs1 }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: 0b001,
                    rs1: self.rs1,
                    rs2: 0,
                    funct7: (0b11100 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let fmt_char = if $fmt == FMT_S { 's' } else { 'd' };
                format!(
                    "fclass.{} {}, {}",
                    fmt_char,
                    reg_name(self.rd, false),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                "fclass"
            }
        }
    };
}

fclass_inst!(FclassS, FMT_S, "fclass");
fclass_inst!(FclassD, FMT_D, "fclass");

// ---------------------------------------------------------------------------
// FP Move (bitwise) – funct5 = 11100 (FP→int) or 11110 (int→FP)
// ---------------------------------------------------------------------------
macro_rules! fmv_x_f {
    ($name:ident, $funct5:expr, $fmt:expr, $to_int:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg, // destination (integer if $to_int, else FP)
            pub rs: Reg, // source (FP if $to_int, else integer)
        }

        impl $name {
            pub fn new(rd: Reg, rs: Reg) -> Self {
                Self { rd, rs }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: 0b000, // rm = 0
                    rs1: self.rs,
                    rs2: 0,
                    funct7: ($funct5 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let (dest_fp, src_fp) = if $to_int {
                    (false, true)
                } else {
                    (true, false)
                };
                format!(
                    "{:<6} {}, {}",
                    $mnem,
                    reg_name(self.rd, dest_fp),
                    reg_name(self.rs, src_fp),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

fmv_x_f!(FmvXW, 0b11100, FMT_S, true, "fmv.x.w");
fmv_x_f!(FmvWX, 0b11110, FMT_S, false, "fmv.w.x");
fmv_x_f!(FmvXD, 0b11100, FMT_D, true, "fmv.x.d");
fmv_x_f!(FmvDX, 0b11110, FMT_D, false, "fmv.d.x");

// ---------------------------------------------------------------------------
// FP Conversions – funct5 indicates direction, rs2 encodes integer type
// ---------------------------------------------------------------------------

/// Integer type codes used in the `rs2` field of fcvt instructions.
mod int_type {
    pub const W: u8 = 0b00000;
    pub const WU: u8 = 0b00001;
    pub const L: u8 = 0b00010;
    pub const LU: u8 = 0b00011;
}

/// FP → Integer conversion (funct5 = 0b11000)
macro_rules! fcvt_f2i {
    ($name:ident, $fmt:expr, $int_rs2:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // integer destination
            pub rs1: Reg, // FP source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: self.rm,
                    rs1: self.rs1,
                    rs2: $int_rs2,
                    funct7: (0b11000 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    $mnem,
                    reg_name(self.rd, false),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

/// Integer → FP conversion (funct5 = 0b11010)
macro_rules! fcvt_i2f {
    ($name:ident, $fmt:expr, $int_rs2:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // FP destination
            pub rs1: Reg, // integer source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: self.rm,
                    rs1: self.rs1,
                    rs2: $int_rs2,
                    funct7: (0b11010 << 2) | ($fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    $mnem,
                    reg_name(self.rd, true),
                    reg_name(self.rs1, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

// FP ↔ FP conversions (special funct5, rs2 encodes source format)
macro_rules! fcvt_f2f {
    ($name:ident, $funct5:expr, $dst_fmt:expr, $src_fmt:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: Reg,  // FP destination
            pub rs1: Reg, // FP source
            pub rm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, rs1: Reg) -> Self {
                Self { rd, rs1, rm: RNE }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                RType {
                    opcode: OP_FP,
                    rd: self.rd,
                    funct3: self.rm,
                    rs1: self.rs1,
                    rs2: $src_fmt, // source format
                    funct7: ($funct5 << 2) | ($dst_fmt & 0b11),
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}",
                    $mnem,
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

// Generate all fcvt variants from the specification
// F → I: from single
fcvt_f2i!(FcvtWS, FMT_S, int_type::W, "fcvt.w.s");
fcvt_f2i!(FcvtWUS, FMT_S, int_type::WU, "fcvt.wu.s");
fcvt_f2i!(FcvtLS, FMT_S, int_type::L, "fcvt.l.s");
fcvt_f2i!(FcvtLUS, FMT_S, int_type::LU, "fcvt.lu.s");
// F → I: from double
fcvt_f2i!(FcvtWD, FMT_D, int_type::W, "fcvt.w.d");
fcvt_f2i!(FcvtWUD, FMT_D, int_type::WU, "fcvt.wu.d");
fcvt_f2i!(FcvtLD, FMT_D, int_type::L, "fcvt.l.d");
fcvt_f2i!(FcvtLUD, FMT_D, int_type::LU, "fcvt.lu.d");

// I → F: to single
fcvt_i2f!(FcvtSW, FMT_S, int_type::W, "fcvt.s.w");
fcvt_i2f!(FcvtSWU, FMT_S, int_type::WU, "fcvt.s.wu");
fcvt_i2f!(FcvtSL, FMT_S, int_type::L, "fcvt.s.l");
fcvt_i2f!(FcvtSLU, FMT_S, int_type::LU, "fcvt.s.lu");
// I → F: to double
fcvt_i2f!(FcvtDW, FMT_D, int_type::W, "fcvt.d.w");
fcvt_i2f!(FcvtDWU, FMT_D, int_type::WU, "fcvt.d.wu");
fcvt_i2f!(FcvtDL, FMT_D, int_type::L, "fcvt.d.l");
fcvt_i2f!(FcvtDLU, FMT_D, int_type::LU, "fcvt.d.lu");

// F ↔ F
fcvt_f2f!(FcvtSD, 0b01000, FMT_S, FMT_D, "fcvt.s.d"); // double→single
fcvt_f2f!(FcvtDS, 0b01001, FMT_D, FMT_S, "fcvt.d.s"); // single→double

// ---------------------------------------------------------------------------
// FP Fused Multiply‑Add / Subtract (R4‑type)
// ---------------------------------------------------------------------------
macro_rules! fmac_inst {
    ($name:ident, $opcode:expr, $fmt:expr, $mnem:literal $(,)?) => {
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
                Self {
                    rd,
                    rs1,
                    rs2,
                    rs3,
                    rm: RNE,
                }
            }
            pub fn with_rm(mut self, rm: u8) -> Self {
                self.rm = rm;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                R4Type {
                    opcode: $opcode,
                    rd: self.rd,
                    rm: self.rm,
                    rs1: self.rs1,
                    rs2: self.rs2,
                    fmt: $fmt,
                    rs3: self.rs3,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                let fmt_char = if $fmt == FMT_S { 's' } else { 'd' };
                format!(
                    "{}.{} {}, {}, {}, {}",
                    $mnem,
                    fmt_char,
                    reg_name(self.rd, true),
                    reg_name(self.rs1, true),
                    reg_name(self.rs2, true),
                    reg_name(self.rs3, true),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

fmac_inst!(FmaddS, OP_FMADD, FMT_S, "fmadd");
fmac_inst!(FmaddD, OP_FMADD, FMT_D, "fmadd");
fmac_inst!(FmsubS, OP_FMSUB, FMT_S, "fmsub");
fmac_inst!(FmsubD, OP_FMSUB, FMT_D, "fmsub");
fmac_inst!(FnmsubS, OP_FNMSUB, FMT_S, "fnmsub");
fmac_inst!(FnmsubD, OP_FNMSUB, FMT_D, "fnmsub");
fmac_inst!(FnmaddS, OP_FNMADD, FMT_S, "fnmadd");
fmac_inst!(FnmaddD, OP_FNMADD, FMT_D, "fnmadd");

// ===========================================================================
// FP Pseudo‑instructions
// ===========================================================================

/// `fmv.s fd, fs` → `fsgnj.s fd, fs, fs`
pub fn fmv_s(fd: Reg, fs: Reg) -> Fsgnj {
    Fsgnj::new(fd, fs, fs)
}
/// `fmv.d fd, fs` → `fsgnj.d fd, fs, fs`
pub fn fmv_d(fd: Reg, fs: Reg) -> FsgnjD {
    FsgnjD::new(fd, fs, fs)
}

/// `fneg.s fd, fs` → `fsgnjn.s fd, fs, fs`
pub fn fneg_s(fd: Reg, fs: Reg) -> Fsgnjn {
    Fsgnjn::new(fd, fs, fs)
}
/// `fneg.d fd, fs` → `fsgnjn.d fd, fs, fs`
pub fn fneg_d(fd: Reg, fs: Reg) -> FsgnjnD {
    FsgnjnD::new(fd, fs, fs)
}

/// `fabs.s fd, fs` → `fsgnjx.s fd, fs, fs`
pub fn fabs_s(fd: Reg, fs: Reg) -> Fsgnjx {
    Fsgnjx::new(fd, fs, fs)
}
/// `fabs.d fd, fs` → `fsgnjx.d fd, fs, fs`
pub fn fabs_d(fd: Reg, fs: Reg) -> FsgnjxD {
    FsgnjxD::new(fd, fs, fs)
}
