//! RV64 Zicsr - Control and Status Register instructions.
//!
//! All CSR instructions use opcode `0x73` (SYSTEM).
//! The 12‑bit CSR address is placed in the I‑type immediate field.
//! For register‑operand instructions (CSRRW, CSRRS, CSRRC) the source
//! register is `rs1`. For immediate variants (CSRRWI, CSRRSI, CSRRCI)
//! the 5‑bit unsigned immediate is placed in the `rs1` field bits.

use crate::assembly_language::encode_decode::{IType, Reg, RiscvFormat as _};
use crate::assembly_language::traits::Instruction;
use crate::assembly_language::utils::reg_name;

const OP_SYSTEM: u8 = 0x73;

// ---------------------------------------------------------------------------
// Common CSR addresses
// ---------------------------------------------------------------------------
pub mod csr {
    pub const FFLAGS: u16 = 0x001;
    pub const FRM: u16 = 0x002;
    pub const FCSR: u16 = 0x003;
    pub const CYCLE: u16 = 0xC00;
    pub const TIME: u16 = 0xC01;
    pub const INSTRET: u16 = 0xC02;
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MTVEC: u16 = 0x305;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
}

// ---------------------------------------------------------------------------
// Register‑operand CSR instructions (funct3 = 1‑3)
// ---------------------------------------------------------------------------

macro_rules! csr_reg_inst {
    ($name:ident, $funct3:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register (rd). If rd = x0 the old CSR value is discarded.
            pub rd: Reg,
            /// CSR address (12‑bit).
            pub csr: u16,
            /// Source register (rs1). If rs1 = x0, the CSR write is suppressed
            /// for CSRRS/CSRRC (not for CSRRW).
            pub rs1: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, csr: u16, rs1: Reg) -> Self {
                assert!(
                    csr <= 0xFFF,
                    "{}: CSR address {:#x} out of range [0x000, 0xFFF]",
                    $mnem,
                    csr
                );
                Self { rd, csr, rs1 }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                IType {
                    opcode: OP_SYSTEM,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.rs1,
                    imm: self.csr as i32, // CSR address in imm[11:0]
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}, {}",
                    $mnem,
                    reg_name(self.rd, false),
                    self.csr,
                    reg_name(self.rs1, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

csr_reg_inst!(Csrrw, 1, "csrrw");
csr_reg_inst!(Csrrs, 2, "csrrs");
csr_reg_inst!(Csrrc, 3, "csrrc");

// ---------------------------------------------------------------------------
// Immediate‑operand CSR instructions (funct3 = 5‑7)
// ---------------------------------------------------------------------------

macro_rules! csr_imm_inst {
    ($name:ident, $funct3:expr, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register (rd).
            pub rd: Reg,
            /// CSR address (12‑bit).
            pub csr: u16,
            /// 5‑bit unsigned immediate (zero‑extended).
            pub uimm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, csr: u16, uimm: u8) -> Self {
                assert!(
                    csr <= 0xFFF,
                    "{}: CSR address {:#x} out of range [0x000, 0xFFF]",
                    $mnem,
                    csr
                );
                Self { rd, csr, uimm }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                IType {
                    opcode: OP_SYSTEM,
                    rd: self.rd,
                    funct3: $funct3,
                    rs1: self.uimm & 0x1F, // uimm in rs1 field
                    imm: self.csr as i32,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{:<6} {}, {}, {}",
                    $mnem,
                    reg_name(self.rd, false),
                    self.csr,
                    self.uimm,
                )
            }

            fn mnemonic(&self) -> &'static str {
                $mnem
            }
        }
    };
}

csr_imm_inst!(Csrrwi, 5, "csrrwi");
csr_imm_inst!(Csrrsi, 6, "csrrsi");
csr_imm_inst!(Csrrci, 7, "csrrci");
