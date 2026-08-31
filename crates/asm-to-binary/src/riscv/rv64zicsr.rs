//! RV64 Zicsr - Control and Status Register instructions.
//!
//! All CSR instructions use opcode `0x73` (SYSTEM).
//! The 12-bit CSR address is placed in the I-type immediate field.
//! For register-operand instructions (CSRRW, CSRRS, CSRRC) the source
//! register is `rs1`. For immediate variants (CSRRWI, CSRRSI, CSRRCI)
//! the 5-bit unsigned immediate is placed in the `rs1` field bits.

use crate::encode_decode::Reg;
use crate::traits::Instruction;
use crate::utils::reg_name;

// --- Common CSR addresses ---
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

// --- Register-operand CSR instructions (funct3 = 1-3) ---

macro_rules! csr_reg_inst {
    ($name:ident, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register (rd). If rd = x0 the old CSR value is discarded.
            pub rd: Reg,
            /// CSR address (12-bit).
            pub csr: u16,
            /// Source register (rs1). If rs1 = x0, the CSR write is suppressed
            /// for CSRRS/CSRRC (not for CSRRW).
            pub rs1: Reg,
        }

        impl $name {
            pub fn new(rd: Reg, csr: u16, rs1: Reg) -> Self {
                assert!(rd < 32, "{}: register x{rd} is out of range", $mnem);
                assert!(rs1 < 32, "{}: register x{rs1} is out of range", $mnem);
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
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Csr {
                        rd,
                        source: self.rs1,
                        csr: self.csr,
                    },
                )
                .unwrap_or_else(|error| panic!("{} descriptor error: {error:?}", $mnem))
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

csr_reg_inst!(Csrrw, "csrrw");
csr_reg_inst!(Csrrs, "csrrs");
csr_reg_inst!(Csrrc, "csrrc");

// --- Immediate-operand CSR instructions (funct3 = 5-7) ---

macro_rules! csr_imm_inst {
    ($name:ident, $mnem:literal $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register (rd).
            pub rd: Reg,
            /// CSR address (12-bit).
            pub csr: u16,
            /// 5-bit unsigned immediate (zero-extended).
            pub uimm: u8,
        }

        impl $name {
            pub fn new(rd: Reg, csr: u16, uimm: u8) -> Self {
                assert!(rd < 32, "{}: register x{rd} is out of range", $mnem);
                assert!(
                    csr <= 0xFFF,
                    "{}: CSR address {:#x} out of range [0x000, 0xFFF]",
                    $mnem,
                    csr
                );
                assert!(
                    uimm < 32,
                    "{}: immediate {uimm} out of range [0, 31]",
                    $mnem
                );
                Self { rd, csr, uimm }
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Csr {
                        rd,
                        source: self.uimm,
                        csr: self.csr,
                    },
                )
                .unwrap_or_else(|error| panic!("{} descriptor error: {error:?}", $mnem))
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

csr_imm_inst!(Csrrwi, "csrrwi");
csr_imm_inst!(Csrrsi, "csrrsi");
csr_imm_inst!(Csrrci, "csrrci");
