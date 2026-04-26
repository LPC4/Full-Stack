/// Generates an R-type instruction struct.
/// The generated struct has fields `rd`, `rs1`, `rs2` and implements
/// [`Instruction`](super::traits::Instruction).
macro_rules! r_inst {
    (
        $name:ident,
        opcode   = $opcode:expr,
        funct3   = $funct3:expr,
        funct7   = $funct7:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::assembly_language::encode_decode::Reg,
            pub rs1: crate::assembly_language::encode_decode::Reg,
            pub rs2: crate::assembly_language::encode_decode::Reg,
        }

        impl $name {
            pub fn new(
                rd: crate::assembly_language::encode_decode::Reg,
                rs1: crate::assembly_language::encode_decode::Reg,
                rs2: crate::assembly_language::encode_decode::Reg,
            ) -> Self {
                Self { rd, rs1, rs2 }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{RType, RiscvFormat};
                RType {
                    opcode: $opcode,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.rs1,
                    rs2: self.rs2,
                    funct7: $funct7,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    $mnemonic,
                    reg_name(self.rd, false),
                    reg_name(self.rs1, false),
                    reg_name(self.rs2, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates an I-type immediate instruction (opcode 0x13).
/// Operand order in `to_asm`: `rd, rs1, imm`
macro_rules! i_imm_inst {
    (
        $name:ident,
        opcode   = $opcode:expr,
        funct3   = $funct3:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::assembly_language::encode_decode::Reg,
            pub rs1: crate::assembly_language::encode_decode::Reg,
            pub imm: i32,
        }

        impl $name {
            pub fn new(
                rd: crate::assembly_language::encode_decode::Reg,
                rs1: crate::assembly_language::encode_decode::Reg,
                imm: i32,
            ) -> Self {
                Self { rd, rs1, imm }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{IType, RiscvFormat};
                IType {
                    opcode: $opcode,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.rs1,
                    imm: self.imm,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    $mnemonic,
                    reg_name(self.rd, false),
                    reg_name(self.rs1, false),
                    self.imm,
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates an I-type load instruction (opcode 0x03).
/// Operand order in `to_asm`: `rd, imm(rs1)` — standard RISC-V load syntax.
macro_rules! i_load_inst {
    (
        $name:ident,
        funct3   = $funct3:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register.
            pub rd: crate::assembly_language::encode_decode::Reg,
            /// Base address register.
            pub base: crate::assembly_language::encode_decode::Reg,
            /// Byte offset added to `base`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                rd: crate::assembly_language::encode_decode::Reg,
                base: crate::assembly_language::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                Self { rd, base, offset }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{IType, RiscvFormat};
                IType {
                    opcode: 0x03,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.base,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}({})",
                    $mnemonic,
                    reg_name(self.rd, false),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates an S-type store instruction (opcode 0x23).
/// Operand order in `to_asm`: `rs2, offset(rs1)` — standard RISC-V store syntax.
macro_rules! s_inst {
    (
        $name:ident,
        funct3   = $funct3:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Base address register.
            pub base: crate::assembly_language::encode_decode::Reg,
            /// Source data register.
            pub src: crate::assembly_language::encode_decode::Reg,
            /// Byte offset added to `base`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                base: crate::assembly_language::encode_decode::Reg,
                src: crate::assembly_language::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                Self { base, src, offset }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{RiscvFormat, SType};
                SType {
                    opcode: 0x23,
                    funct3: $funct3,
                    rs1: self.base,
                    rs2: self.src,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}({})",
                    $mnemonic,
                    reg_name(self.src, false),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates a B-type branch instruction (opcode 0x63).
/// Operand order in `to_asm`: `rs1, rs2, offset`
macro_rules! b_inst {
    (
        $name:ident,
        funct3   = $funct3:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rs1: crate::assembly_language::encode_decode::Reg,
            pub rs2: crate::assembly_language::encode_decode::Reg,
            /// PC-relative byte offset. Must be even, in `[-4096, +4094]`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                rs1: crate::assembly_language::encode_decode::Reg,
                rs2: crate::assembly_language::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                Self { rs1, rs2, offset }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{BType, RiscvFormat};
                BType {
                    opcode: 0x63,
                    funct3: $funct3,
                    rs1: self.rs1,
                    rs2: self.rs2,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    $mnemonic,
                    reg_name(self.rs1, false),
                    reg_name(self.rs2, false),
                    self.offset,
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates a U-type instruction (`lui` or `auipc`).
/// Operand order in `to_asm`: `rd, imm` (imm printed as upper-20-bit hex)
macro_rules! u_inst {
    (
        $name:ident,
        opcode   = $opcode:expr,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::assembly_language::encode_decode::Reg,
            /// Full 32-bit value whose upper 20 bits are the immediate.
            /// Lower 12 bits should be zero; they are masked during encode.
            pub imm: i32,
        }

        impl $name {
            pub fn new(rd: crate::assembly_language::encode_decode::Reg, imm: i32) -> Self {
                Self { rd, imm }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{RiscvFormat, UType};
                UType {
                    opcode: $opcode,
                    rd: self.rd,
                    imm: self.imm,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {:#x}",
                    $mnemonic,
                    reg_name(self.rd, false),
                    (self.imm as u32) >> 12,
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}

/// Generates a J-type instruction (`jal`).
/// Operand order in `to_asm`: `rd, offset`
macro_rules! j_inst {
    (
        $name:ident,
        mnemonic = $mnemonic:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::assembly_language::encode_decode::Reg,
            /// PC-relative byte offset. Must be even, in `[-1048576, +1048574]`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(rd: crate::assembly_language::encode_decode::Reg, offset: i32) -> Self {
                Self { rd, offset }
            }
        }

        impl crate::assembly_language::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                use crate::assembly_language::encode_decode::{JType, RiscvFormat};
                JType {
                    opcode: 0x6F,
                    rd: self.rd,
                    imm: self.offset,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                use crate::assembly_language::utils::reg_name;
                format!(
                    "{:<6} {}, {}",
                    $mnemonic,
                    reg_name(self.rd, false),
                    self.offset,
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnemonic
            }
        }
    };
}
