//! Instruction decoder for the RV64IMAFD ISA.
//!
//! [`decode`] takes a raw 32-bit instruction word and returns a [`DecodedInsn`]
//! whose variant carries the fully extracted and sign-extended fields, ready for
//! the execute stage. All immediate values are sign-extended to `i64`.

use crate::error::VmError;
use riscv_model::{
    DecodedOperands, InstructionFormat, InstructionId, decode_atomic, decode_base_integer,
    decode_floating, decode_register_register, decode_upper_immediate, float_register_name,
    instruction_spec, integer_register_name, register_register_instruction,
};

// --- Public types ---

#[derive(Debug, Clone)]
pub enum FMacOp {
    Fmadd,
    Fmsub,
    Fnmsub,
    Fnmadd,
}

#[derive(Debug, Clone)]
pub enum DecodedInsn {
    Lui {
        rd: usize,
        imm: i64,
    },
    Auipc {
        rd: usize,
        imm: i64,
    },
    Jal {
        rd: usize,
        imm: i64,
    },
    Jalr {
        rd: usize,
        rs1: usize,
        imm: i64,
    },
    Branch {
        funct3: u8,
        rs1: usize,
        rs2: usize,
        imm: i64,
    },
    Load {
        funct3: u8,
        rd: usize,
        rs1: usize,
        imm: i64,
    },
    Store {
        funct3: u8,
        rs1: usize,
        rs2: usize,
        imm: i64,
    },
    AluImm {
        funct3: u8,
        funct7: u8,
        rd: usize,
        rs1: usize,
        imm: i64,
    },
    AluImm32 {
        funct3: u8,
        funct7: u8,
        rd: usize,
        rs1: usize,
        imm: i64,
    },
    Alu {
        funct3: u8,
        funct7: u8,
        rd: usize,
        rs1: usize,
        rs2: usize,
    },
    Alu32 {
        funct3: u8,
        funct7: u8,
        rd: usize,
        rs1: usize,
        rs2: usize,
    },
    Fence {
        fm: u8,
        pred: u8,
        succ: u8,
    },
    FenceI,
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Wfi,
    Csr {
        funct3: u8,
        rd: usize,
        rs1_uimm: usize,
        csr: u16,
    },
    FLoad {
        funct3: u8,
        rd: usize,
        rs1: usize,
        imm: i64,
    },
    FStore {
        funct3: u8,
        rs1: usize,
        rs2: usize,
        imm: i64,
    },
    FOp {
        funct5: u8,
        fmt: u8,
        rm: u8,
        rd: usize,
        rs1: usize,
        rs2: usize,
    },
    FMac {
        op: FMacOp,
        fmt: u8,
        rm: u8,
        rd: usize,
        rs1: usize,
        rs2: usize,
        rs3: usize,
    },
    Atomic {
        funct5: u8,
        aq: bool,
        rl: bool,
        funct3: u8,
        rd: usize,
        rs1: usize,
        rs2: usize,
    },
}

impl DecodedInsn {
    pub fn instruction_id(&self) -> Option<InstructionId> {
        #[cfg_attr(
            not(test),
            expect(
                clippy::enum_glob_use,
                reason = "the exhaustive mapping reads as an instruction vocabulary"
            )
        )]
        use InstructionId::*;

        match self {
            Self::Lui { .. } => Some(Lui),
            Self::Auipc { .. } => Some(Auipc),
            Self::Jal { .. } => Some(Jal),
            Self::Jalr { .. } => Some(Jalr),
            Self::Branch { funct3, .. } => decode_base_integer((u32::from(*funct3) << 12) | 0x63)
                .ok()
                .map(|decoded| decoded.id),
            Self::Load { funct3, .. } => decode_base_integer((u32::from(*funct3) << 12) | 0x03)
                .ok()
                .map(|decoded| decoded.id),
            Self::Store { funct3, .. } => decode_base_integer((u32::from(*funct3) << 12) | 0x23)
                .ok()
                .map(|decoded| decoded.id),
            Self::AluImm { funct3, funct7, .. } => {
                decode_base_integer((u32::from(*funct7) << 25) | (u32::from(*funct3) << 12) | 0x13)
                    .ok()
                    .map(|decoded| decoded.id)
            }
            Self::AluImm32 { funct3, funct7, .. } => {
                decode_base_integer((u32::from(*funct7) << 25) | (u32::from(*funct3) << 12) | 0x1b)
                    .ok()
                    .map(|decoded| decoded.id)
            }
            Self::Alu { funct3, funct7, .. } => {
                register_register_instruction(InstructionFormat::RegisterRegister, *funct3, *funct7)
            }
            Self::Alu32 { funct3, funct7, .. } => register_register_instruction(
                InstructionFormat::RegisterRegisterWord,
                *funct3,
                *funct7,
            ),
            Self::Fence { .. } => Some(Fence),
            Self::FenceI => Some(FenceI),
            Self::Ecall => Some(Ecall),
            Self::Ebreak => Some(Ebreak),
            Self::Mret => Some(Mret),
            Self::Sret => Some(Sret),
            Self::SfenceVma => Some(SfenceVma),
            Self::Wfi => Some(Wfi),
            Self::Csr { funct3, .. } => decode_base_integer((u32::from(*funct3) << 12) | 0x73)
                .ok()
                .map(|decoded| decoded.id),
            Self::FLoad { funct3, .. } => decode_floating((u32::from(*funct3) << 12) | 0x07)
                .ok()
                .map(|decoded| decoded.id),
            Self::FStore { funct3, .. } => decode_floating((u32::from(*funct3) << 12) | 0x27)
                .ok()
                .map(|decoded| decoded.id),
            Self::FOp {
                funct5,
                fmt,
                rm,
                rs2,
                ..
            } => decode_floating(
                (u32::from(*funct5) << 27)
                    | (u32::from(*fmt) << 25)
                    | (u32::try_from(*rs2).ok()? << 20)
                    | (u32::from(*rm) << 12)
                    | 0x53,
            )
            .ok()
            .map(|decoded| decoded.id),
            Self::FMac { op, fmt, .. } => {
                let opcode = match op {
                    FMacOp::Fmadd => 0x43,
                    FMacOp::Fmsub => 0x47,
                    FMacOp::Fnmsub => 0x4b,
                    FMacOp::Fnmadd => 0x4f,
                };
                decode_floating((u32::from(*fmt) << 25) | opcode)
                    .ok()
                    .map(|decoded| decoded.id)
            }
            Self::Atomic {
                funct5,
                funct3,
                rs2,
                ..
            } => decode_atomic(
                (u32::from(*funct5) << 27)
                    | (u32::try_from(*rs2).ok()? << 20)
                    | (u32::from(*funct3) << 12)
                    | 0x2f,
            )
            .ok()
            .map(|decoded| decoded.id),
        }
    }

    pub fn mnemonic(&self) -> &'static str {
        if let Some(id) = self.instruction_id() {
            return id.mnemonic();
        }
        match self {
            Self::Lui { .. } => "lui",
            Self::Auipc { .. } => "auipc",
            Self::Jal { .. } => "jal",
            Self::Jalr { .. } => "jalr",
            Self::Branch { funct3, .. } => match funct3 {
                0 => "beq",
                1 => "bne",
                4 => "blt",
                5 => "bge",
                6 => "bltu",
                7 => "bgeu",
                _ => "b??",
            },
            Self::Load { funct3, .. } => match funct3 {
                0 => "lb",
                1 => "lh",
                2 => "lw",
                3 => "ld",
                4 => "lbu",
                5 => "lhu",
                6 => "lwu",
                _ => "l??",
            },
            Self::Store { funct3, .. } => match funct3 {
                0 => "sb",
                1 => "sh",
                2 => "sw",
                3 => "sd",
                _ => "s??",
            },
            Self::AluImm { funct3, funct7, .. } => match funct3 {
                0 => "addi",
                1 => "slli",
                2 => "slti",
                3 => "sltiu",
                4 => "xori",
                5 => {
                    if funct7 & 0x20 != 0 {
                        "srai"
                    } else {
                        "srli"
                    }
                }
                6 => "ori",
                7 => "andi",
                _ => "imm??",
            },
            Self::AluImm32 { funct3, funct7, .. } => match funct3 {
                0 => "addiw",
                1 => "slliw",
                5 => {
                    if funct7 & 0x20 != 0 {
                        "sraiw"
                    } else {
                        "srliw"
                    }
                }
                _ => "immw??",
            },
            Self::Alu { .. } => self
                .instruction_id()
                .map_or("alu??", InstructionId::mnemonic),
            Self::Alu32 { .. } => self
                .instruction_id()
                .map_or("alu32??", InstructionId::mnemonic),
            Self::Fence { .. } => "fence",
            Self::FenceI => "fence.i",
            Self::Ecall => "ecall",
            Self::Ebreak => "ebreak",
            Self::Mret => "mret",
            Self::Sret => "sret",
            Self::SfenceVma => "sfence.vma",
            Self::Wfi => "wfi",
            Self::Csr { funct3, .. } => match funct3 {
                1 => "csrrw",
                2 => "csrrs",
                3 => "csrrc",
                5 => "csrrwi",
                6 => "csrrsi",
                7 => "csrrci",
                _ => "csr??",
            },
            Self::FLoad { funct3, .. } => {
                if *funct3 == 2 {
                    "flw"
                } else {
                    "fld"
                }
            }
            Self::FStore { funct3, .. } => {
                if *funct3 == 2 {
                    "fsw"
                } else {
                    "fsd"
                }
            }
            Self::FOp { .. } => "f.op",
            Self::FMac { op, .. } => match op {
                FMacOp::Fmadd => "fmadd",
                FMacOp::Fmsub => "fmsub",
                FMacOp::Fnmsub => "fnmsub",
                FMacOp::Fnmadd => "fnmadd",
            },
            Self::Atomic { funct5, .. } => match funct5 {
                2 => "lr.w",
                3 => "sc.w",
                1 => "amoswap",
                0 => "amoadd",
                4 => "amoxor",
                8 => "amoor",
                12 => "amoand",
                16 => "amomin",
                20 => "amomax",
                24 => "amominu",
                28 => "amomaxu",
                _ => "amo??",
            },
        }
    }
}

// --- ABI register names for disassembly ---

fn reg_name(r: usize) -> &'static str {
    integer_register_name(r).unwrap_or("??")
}

fn freg_name(r: usize) -> &'static str {
    float_register_name(r).unwrap_or("f??")
}

impl std::fmt::Display for DecodedInsn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lui { rd, imm } => {
                write!(
                    f,
                    "{:<10} {}, {:#x}",
                    "lui",
                    reg_name(*rd),
                    (*imm as u64) >> 12
                )
            }
            Self::Auipc { rd, imm } => {
                write!(
                    f,
                    "{:<10} {}, {:#x}",
                    "auipc",
                    reg_name(*rd),
                    (*imm as u64) >> 12
                )
            }
            Self::Jal { rd, imm } => {
                write!(f, "{:<10} {}, {}", "jal", reg_name(*rd), imm)
            }
            Self::Jalr { rd, rs1, imm } => {
                write!(
                    f,
                    "{:<10} {}, {}({})",
                    "jalr",
                    reg_name(*rd),
                    imm,
                    reg_name(*rs1)
                )
            }
            Self::Branch { rs1, rs2, imm, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    reg_name(*rs1),
                    reg_name(*rs2),
                    imm
                )
            }
            Self::Load { rd, rs1, imm, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}({})",
                    self.mnemonic(),
                    reg_name(*rd),
                    imm,
                    reg_name(*rs1)
                )
            }
            Self::Store { rs1, rs2, imm, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}({})",
                    self.mnemonic(),
                    reg_name(*rs2),
                    imm,
                    reg_name(*rs1)
                )
            }
            Self::AluImm {
                rd,
                rs1,
                imm,
                funct3,
                ..
            } => {
                let shift = if *funct3 == 1 || *funct3 == 5 {
                    imm & 0x3F
                } else {
                    *imm
                };
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    reg_name(*rd),
                    reg_name(*rs1),
                    shift
                )
            }
            Self::AluImm32 {
                rd,
                rs1,
                imm,
                funct3,
                ..
            } => {
                let shift = if *funct3 == 1 || *funct3 == 5 {
                    imm & 0x1F
                } else {
                    *imm
                };
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    reg_name(*rd),
                    reg_name(*rs1),
                    shift
                )
            }
            Self::Alu { rd, rs1, rs2, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    reg_name(*rd),
                    reg_name(*rs1),
                    reg_name(*rs2)
                )
            }
            Self::Alu32 { rd, rs1, rs2, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    reg_name(*rd),
                    reg_name(*rs1),
                    reg_name(*rs2)
                )
            }
            Self::Fence { pred, succ, .. } => {
                write!(f, "{:<10} {}, {}", "fence", pred, succ)
            }
            Self::FenceI => write!(f, "fence.i"),
            Self::Ecall => write!(f, "ecall"),
            Self::Ebreak => write!(f, "ebreak"),
            Self::Mret => write!(f, "mret"),
            Self::Sret => write!(f, "sret"),
            Self::SfenceVma => write!(f, "sfence.vma"),
            Self::Wfi => write!(f, "wfi"),
            Self::Csr {
                rd, rs1_uimm, csr, ..
            } => {
                let mnem = self.mnemonic();
                if mnem == "csrrwi" || mnem == "csrrsi" || mnem == "csrrci" {
                    write!(
                        f,
                        "{:<10} {}, {:#x}, {}",
                        mnem,
                        reg_name(*rd),
                        csr,
                        rs1_uimm
                    )
                } else {
                    write!(
                        f,
                        "{:<10} {}, {:#x}, {}",
                        mnem,
                        reg_name(*rd),
                        csr,
                        reg_name(*rs1_uimm)
                    )
                }
            }
            Self::FLoad { rd, rs1, imm, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}({})",
                    self.mnemonic(),
                    freg_name(*rd),
                    imm,
                    reg_name(*rs1)
                )
            }
            Self::FStore { rs1, rs2, imm, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}({})",
                    self.mnemonic(),
                    freg_name(*rs2),
                    imm,
                    reg_name(*rs1)
                )
            }
            Self::FOp { rd, rs1, rs2, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    self.mnemonic(),
                    freg_name(*rd),
                    freg_name(*rs1),
                    freg_name(*rs2)
                )
            }
            Self::FMac {
                fmt,
                rd,
                rs1,
                rs2,
                rs3,
                ..
            } => {
                let mnem = match self.mnemonic() {
                    "fmadd" => {
                        if *fmt == 1 {
                            "fmadd.d"
                        } else {
                            "fmadd.s"
                        }
                    }
                    "fmsub" => {
                        if *fmt == 1 {
                            "fmsub.d"
                        } else {
                            "fmsub.s"
                        }
                    }
                    "fnmsub" => {
                        if *fmt == 1 {
                            "fnmsub.d"
                        } else {
                            "fnmsub.s"
                        }
                    }
                    "fnmadd" => {
                        if *fmt == 1 {
                            "fnmadd.d"
                        } else {
                            "fnmadd.s"
                        }
                    }
                    _ => "fmac??",
                };
                write!(
                    f,
                    "{:<10} {}, {}, {}, {}",
                    mnem,
                    freg_name(*rd),
                    freg_name(*rs1),
                    freg_name(*rs2),
                    freg_name(*rs3)
                )
            }
            Self::Atomic { rd, rs1, rs2, .. } => {
                write!(
                    f,
                    "{:<10} {}, {}, ({})",
                    self.mnemonic(),
                    reg_name(*rd),
                    reg_name(*rs2),
                    reg_name(*rs1)
                )
            }
        }
    }
}

// --- Public decode entry point ---

pub fn decode(word: u32) -> Result<DecodedInsn, VmError> {
    if let Ok(decoded) = decode_upper_immediate(word) {
        let DecodedOperands::UpperImmediate { rd, immediate } = decoded.operands else {
            return Err(VmError::IllegalInstruction(word));
        };
        return match decoded.id {
            InstructionId::Lui => Ok(DecodedInsn::Lui {
                rd: usize::from(rd.number()),
                imm: i64::from(immediate),
            }),
            InstructionId::Auipc => Ok(DecodedInsn::Auipc {
                rd: usize::from(rd.number()),
                imm: i64::from(immediate),
            }),
            _ => Err(VmError::IllegalInstruction(word)),
        };
    }

    if let Ok(decoded) = decode_register_register(word) {
        let DecodedOperands::RegisterRegister { rd, rs1, rs2 } = decoded.operands else {
            return Err(VmError::IllegalInstruction(word));
        };
        let Some(spec) = instruction_spec(decoded.id) else {
            return Err(VmError::IllegalInstruction(word));
        };
        let fields = (
            funct3(word),
            funct7(word),
            usize::from(rd.number()),
            usize::from(rs1.number()),
            usize::from(rs2.number()),
        );
        return match spec.format {
            InstructionFormat::RegisterRegister => Ok(DecodedInsn::Alu {
                funct3: fields.0,
                funct7: fields.1,
                rd: fields.2,
                rs1: fields.3,
                rs2: fields.4,
            }),
            InstructionFormat::RegisterRegisterWord => Ok(DecodedInsn::Alu32 {
                funct3: fields.0,
                funct7: fields.1,
                rd: fields.2,
                rs1: fields.3,
                rs2: fields.4,
            }),
            _ => Err(VmError::IllegalInstruction(word)),
        };
    }

    if let Ok(decoded) = decode_base_integer(word) {
        let funct3 = funct3(word);
        let funct7 = funct7(word);
        return match (decoded.id, decoded.operands) {
            (InstructionId::Jal, DecodedOperands::Jump { rd, offset }) => Ok(DecodedInsn::Jal {
                rd: usize::from(rd.number()),
                imm: i64::from(offset),
            }),
            (InstructionId::Jalr, DecodedOperands::Immediate { rd, rs1, immediate }) => {
                Ok(DecodedInsn::Jalr {
                    rd: usize::from(rd.number()),
                    rs1: usize::from(rs1.number()),
                    imm: i64::from(immediate),
                })
            }
            (
                InstructionId::Beq
                | InstructionId::Bne
                | InstructionId::Blt
                | InstructionId::Bge
                | InstructionId::Bltu
                | InstructionId::Bgeu,
                DecodedOperands::Branch { rs1, rs2, offset },
            ) => Ok(DecodedInsn::Branch {
                funct3,
                rs1: usize::from(rs1.number()),
                rs2: usize::from(rs2.number()),
                imm: i64::from(offset),
            }),
            (
                InstructionId::Lb
                | InstructionId::Lh
                | InstructionId::Lw
                | InstructionId::Ld
                | InstructionId::Lbu
                | InstructionId::Lhu
                | InstructionId::Lwu,
                DecodedOperands::Immediate { rd, rs1, immediate },
            ) => Ok(DecodedInsn::Load {
                funct3,
                rd: usize::from(rd.number()),
                rs1: usize::from(rs1.number()),
                imm: i64::from(immediate),
            }),
            (
                InstructionId::Sb | InstructionId::Sh | InstructionId::Sw | InstructionId::Sd,
                DecodedOperands::Store {
                    rs1,
                    rs2,
                    immediate,
                },
            ) => Ok(DecodedInsn::Store {
                funct3,
                rs1: usize::from(rs1.number()),
                rs2: usize::from(rs2.number()),
                imm: i64::from(immediate),
            }),
            (
                InstructionId::Addi
                | InstructionId::Slti
                | InstructionId::Sltiu
                | InstructionId::Xori
                | InstructionId::Ori
                | InstructionId::Andi
                | InstructionId::Slli
                | InstructionId::Srli
                | InstructionId::Srai,
                DecodedOperands::Immediate { rd, rs1, immediate },
            ) => Ok(DecodedInsn::AluImm {
                funct3,
                funct7,
                rd: usize::from(rd.number()),
                rs1: usize::from(rs1.number()),
                imm: i64::from(immediate),
            }),
            (
                InstructionId::Addiw
                | InstructionId::Slliw
                | InstructionId::Srliw
                | InstructionId::Sraiw,
                DecodedOperands::Immediate { rd, rs1, immediate },
            ) => Ok(DecodedInsn::AluImm32 {
                funct3,
                funct7,
                rd: usize::from(rd.number()),
                rs1: usize::from(rs1.number()),
                imm: i64::from(immediate),
            }),
            (
                InstructionId::Fence,
                DecodedOperands::Fence {
                    fm,
                    predecessor,
                    successor,
                },
            ) => Ok(DecodedInsn::Fence {
                fm,
                pred: predecessor,
                succ: successor,
            }),
            (InstructionId::FenceI, DecodedOperands::None) => Ok(DecodedInsn::FenceI),
            (InstructionId::Ecall, DecodedOperands::None) => Ok(DecodedInsn::Ecall),
            (InstructionId::Ebreak, DecodedOperands::None) => Ok(DecodedInsn::Ebreak),
            (InstructionId::Mret, DecodedOperands::None) => Ok(DecodedInsn::Mret),
            (InstructionId::Sret, DecodedOperands::None) => Ok(DecodedInsn::Sret),
            (InstructionId::SfenceVma, DecodedOperands::None) => Ok(DecodedInsn::SfenceVma),
            (InstructionId::Wfi, DecodedOperands::None) => Ok(DecodedInsn::Wfi),
            (
                InstructionId::Csrrw
                | InstructionId::Csrrs
                | InstructionId::Csrrc
                | InstructionId::Csrrwi
                | InstructionId::Csrrsi
                | InstructionId::Csrrci,
                DecodedOperands::Csr { rd, source, csr },
            ) => Ok(DecodedInsn::Csr {
                funct3,
                rd: usize::from(rd.number()),
                rs1_uimm: usize::from(source),
                csr,
            }),
            _ => Err(VmError::IllegalInstruction(word)),
        };
    }

    if let Ok(decoded) = decode_atomic(word) {
        let DecodedOperands::Atomic {
            rd,
            rs1,
            rs2,
            acquire,
            release,
        } = decoded.operands
        else {
            return Err(VmError::IllegalInstruction(word));
        };
        return Ok(DecodedInsn::Atomic {
            funct5: field(word, 31, 27) as u8,
            aq: acquire,
            rl: release,
            funct3: funct3(word),
            rd: usize::from(rd.number()),
            rs1: usize::from(rs1.number()),
            rs2: usize::from(rs2.number()),
        });
    }

    if let Ok(decoded) = decode_floating(word) {
        let DecodedOperands::Floating {
            rd,
            rs1,
            rs2,
            rs3,
            rounding_mode,
            immediate,
        } = decoded.operands
        else {
            return Err(VmError::IllegalInstruction(word));
        };
        let Some(spec) = instruction_spec(decoded.id) else {
            return Err(VmError::IllegalInstruction(word));
        };
        return match spec.format {
            InstructionFormat::FloatingLoad => Ok(DecodedInsn::FLoad {
                funct3: funct3(word),
                rd: usize::from(rd),
                rs1: usize::from(rs1),
                imm: i64::from(immediate),
            }),
            InstructionFormat::FloatingStore => Ok(DecodedInsn::FStore {
                funct3: funct3(word),
                rs1: usize::from(rs1),
                rs2: usize::from(rs2),
                imm: i64::from(immediate),
            }),
            InstructionFormat::FloatingOp => Ok(DecodedInsn::FOp {
                funct5: field(word, 31, 27) as u8,
                fmt: field(word, 26, 25) as u8,
                rm: rounding_mode,
                rd: usize::from(rd),
                rs1: usize::from(rs1),
                rs2: usize::from(rs2),
            }),
            InstructionFormat::FloatingFused => {
                let op = match decoded.id {
                    InstructionId::FmaddS | InstructionId::FmaddD => FMacOp::Fmadd,
                    InstructionId::FmsubS | InstructionId::FmsubD => FMacOp::Fmsub,
                    InstructionId::FnmsubS | InstructionId::FnmsubD => FMacOp::Fnmsub,
                    InstructionId::FnmaddS | InstructionId::FnmaddD => FMacOp::Fnmadd,
                    _ => return Err(VmError::IllegalInstruction(word)),
                };
                Ok(DecodedInsn::FMac {
                    op,
                    fmt: field(word, 26, 25) as u8,
                    rm: rounding_mode,
                    rd: usize::from(rd),
                    rs1: usize::from(rs1),
                    rs2: usize::from(rs2),
                    rs3: usize::from(rs3),
                })
            }
            _ => Err(VmError::IllegalInstruction(word)),
        };
    }

    Err(VmError::IllegalInstruction(word))
}

// --- Field extraction helpers ---

// Extract bits [hi..lo] (inclusive) from `word`.
fn field(word: u32, hi: u32, lo: u32) -> u32 {
    let width = hi - lo + 1;
    if width >= 32 {
        word
    } else {
        (word >> lo) & ((1u32 << width) - 1)
    }
}

#[inline]
fn funct3(word: u32) -> u8 {
    field(word, 14, 12) as u8
}
#[inline]
fn funct7(word: u32) -> u8 {
    field(word, 31, 25) as u8
}

#[cfg(test)]
mod tests {
    use super::{DecodedInsn, FMacOp};
    use riscv_model::InstructionId;

    #[test]
    fn decoded_operands_map_to_shared_instruction_identity() {
        let add = DecodedInsn::Alu {
            funct3: 0,
            funct7: 0,
            rd: 1,
            rs1: 2,
            rs2: 3,
        };
        let convert = DecodedInsn::FOp {
            funct5: 0b11010,
            fmt: 1,
            rm: 0,
            rd: 1,
            rs1: 2,
            rs2: 3,
        };
        let fused = DecodedInsn::FMac {
            op: FMacOp::Fnmadd,
            fmt: 0,
            rm: 0,
            rd: 1,
            rs1: 2,
            rs2: 3,
            rs3: 4,
        };
        let atomic = DecodedInsn::Atomic {
            funct5: 0,
            aq: false,
            rl: false,
            funct3: 3,
            rd: 1,
            rs1: 2,
            rs2: 3,
        };

        assert_eq!(add.instruction_id(), Some(InstructionId::Add));
        assert_eq!(convert.instruction_id(), Some(InstructionId::FcvtDLu));
        assert_eq!(fused.instruction_id(), Some(InstructionId::FnmaddS));
        assert_eq!(atomic.instruction_id(), Some(InstructionId::AmoaddD));
    }

    #[test]
    fn unsupported_selectors_have_no_instruction_identity() {
        let branch = DecodedInsn::Branch {
            funct3: 2,
            rs1: 1,
            rs2: 2,
            imm: 4,
        };

        assert_eq!(branch.instruction_id(), None);
    }
}
