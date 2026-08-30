//! Instruction decoder for the RV64IMAFD ISA.
//!
//! [`decode`] takes a raw 32-bit instruction word and returns a [`DecodedInsn`]
//! whose variant carries the fully extracted and sign-extended fields, ready for
//! the execute stage. All immediate values are sign-extended to `i64`.

use crate::error::VmError;
use asm_to_binary::isa::{float_register_name, integer_register_name};
use riscv_model::InstructionId;

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
            Self::Branch { funct3, .. } => match funct3 {
                0 => Some(Beq),
                1 => Some(Bne),
                4 => Some(Blt),
                5 => Some(Bge),
                6 => Some(Bltu),
                7 => Some(Bgeu),
                _ => None,
            },
            Self::Load { funct3, .. } => match funct3 {
                0 => Some(Lb),
                1 => Some(Lh),
                2 => Some(Lw),
                3 => Some(Ld),
                4 => Some(Lbu),
                5 => Some(Lhu),
                6 => Some(Lwu),
                _ => None,
            },
            Self::Store { funct3, .. } => match funct3 {
                0 => Some(Sb),
                1 => Some(Sh),
                2 => Some(Sw),
                3 => Some(Sd),
                _ => None,
            },
            Self::AluImm { funct3, funct7, .. } => match funct3 {
                0 => Some(Addi),
                1 => Some(Slli),
                2 => Some(Slti),
                3 => Some(Sltiu),
                4 => Some(Xori),
                5 if funct7 & 0x20 != 0 => Some(Srai),
                5 => Some(Srli),
                6 => Some(Ori),
                7 => Some(Andi),
                _ => None,
            },
            Self::AluImm32 { funct3, funct7, .. } => match funct3 {
                0 => Some(Addiw),
                1 => Some(Slliw),
                5 if funct7 & 0x20 != 0 => Some(Sraiw),
                5 => Some(Srliw),
                _ => None,
            },
            Self::Alu { funct3, funct7, .. } => match (*funct7, *funct3) {
                (0, 0) => Some(Add),
                (0, 1) => Some(Sll),
                (0, 2) => Some(Slt),
                (0, 3) => Some(Sltu),
                (0, 4) => Some(Xor),
                (0, 5) => Some(Srl),
                (0, 6) => Some(Or),
                (0, 7) => Some(And),
                (0x20, 0) => Some(Sub),
                (0x20, 5) => Some(Sra),
                (1, 0) => Some(Mul),
                (1, 1) => Some(Mulh),
                (1, 2) => Some(Mulhsu),
                (1, 3) => Some(Mulhu),
                (1, 4) => Some(Div),
                (1, 5) => Some(Divu),
                (1, 6) => Some(Rem),
                (1, 7) => Some(Remu),
                _ => None,
            },
            Self::Alu32 { funct3, funct7, .. } => match (*funct7, *funct3) {
                (0, 0) => Some(Addw),
                (0, 1) => Some(Sllw),
                (0, 5) => Some(Srlw),
                (0x20, 0) => Some(Subw),
                (0x20, 5) => Some(Sraw),
                (1, 0) => Some(Mulw),
                (1, 4) => Some(Divw),
                (1, 5) => Some(Divuw),
                (1, 6) => Some(Remw),
                (1, 7) => Some(Remuw),
                _ => None,
            },
            Self::Fence { .. } => Some(Fence),
            Self::FenceI => Some(FenceI),
            Self::Ecall => Some(Ecall),
            Self::Ebreak => Some(Ebreak),
            Self::Mret => Some(Mret),
            Self::Sret => Some(Sret),
            Self::SfenceVma => Some(SfenceVma),
            Self::Wfi => Some(Wfi),
            Self::Csr { funct3, .. } => match funct3 {
                1 => Some(Csrrw),
                2 => Some(Csrrs),
                3 => Some(Csrrc),
                5 => Some(Csrrwi),
                6 => Some(Csrrsi),
                7 => Some(Csrrci),
                _ => None,
            },
            Self::FLoad { funct3: 2, .. } => Some(Flw),
            Self::FLoad { funct3: 3, .. } => Some(Fld),
            Self::FLoad { .. } => None,
            Self::FStore { funct3: 2, .. } => Some(Fsw),
            Self::FStore { funct3: 3, .. } => Some(Fsd),
            Self::FStore { .. } => None,
            Self::FOp {
                funct5,
                fmt,
                rm,
                rs2,
                ..
            } => fp_instruction_id(*funct5, *fmt, *rm, *rs2),
            Self::FMac { op, fmt, .. } => fmac_instruction_id(op, *fmt),
            Self::Atomic { funct5, funct3, .. } => atomic_instruction_id(*funct5, *funct3),
        }
    }

    pub fn mnemonic(&self) -> &'static str {
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
            Self::Alu { funct3, funct7, .. } => {
                if *funct7 == 1 {
                    match funct3 {
                        0 => "mul",
                        1 => "mulh",
                        2 => "mulhsu",
                        3 => "mulhu",
                        4 => "div",
                        5 => "divu",
                        6 => "rem",
                        7 => "remu",
                        _ => "m??",
                    }
                } else {
                    match (funct3, funct7 & 0x20 != 0) {
                        (0, true) => "sub",
                        (0, false) => "add",
                        (1, _) => "sll",
                        (2, _) => "slt",
                        (3, _) => "sltu",
                        (4, _) => "xor",
                        (5, true) => "sra",
                        (5, false) => "srl",
                        (6, _) => "or",
                        (7, _) => "and",
                        _ => "alu??",
                    }
                }
            }
            Self::Alu32 { funct3, funct7, .. } => {
                if *funct7 == 1 {
                    match funct3 {
                        0 => "mulw",
                        4 => "divw",
                        5 => "divuw",
                        6 => "remw",
                        7 => "remuw",
                        _ => "mw??",
                    }
                } else {
                    match (funct3, funct7 & 0x20 != 0) {
                        (0, true) => "subw",
                        (0, false) => "addw",
                        (1, _) => "sllw",
                        (5, true) => "sraw",
                        (5, false) => "srlw",
                        _ => "alu32??",
                    }
                }
            }
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

fn fp_instruction_id(funct5: u8, fmt: u8, rm: u8, rs2: usize) -> Option<InstructionId> {
    use InstructionId as Id;

    match (funct5, fmt, rm, rs2) {
        (0b00000, 0, _, _) => Some(Id::FaddS),
        (0b00000, 1, _, _) => Some(Id::FaddD),
        (0b00001, 0, _, _) => Some(Id::FsubS),
        (0b00001, 1, _, _) => Some(Id::FsubD),
        (0b00010, 0, _, _) => Some(Id::FmulS),
        (0b00010, 1, _, _) => Some(Id::FmulD),
        (0b00011, 0, _, _) => Some(Id::FdivS),
        (0b00011, 1, _, _) => Some(Id::FdivD),
        (0b01011, 0, _, _) => Some(Id::FsqrtS),
        (0b01011, 1, _, _) => Some(Id::FsqrtD),
        (0b00100, 0, 0, _) => Some(Id::FsgnjS),
        (0b00100, 1, 0, _) => Some(Id::FsgnjD),
        (0b00100, 0, 1, _) => Some(Id::FsgnjnS),
        (0b00100, 1, 1, _) => Some(Id::FsgnjnD),
        (0b00100, 0, 2, _) => Some(Id::FsgnjxS),
        (0b00100, 1, 2, _) => Some(Id::FsgnjxD),
        (0b00101, 0, 0, _) => Some(Id::FminS),
        (0b00101, 1, 0, _) => Some(Id::FminD),
        (0b00101, 0, 1, _) => Some(Id::FmaxS),
        (0b00101, 1, 1, _) => Some(Id::FmaxD),
        (0b10100, 0, 0, _) => Some(Id::FleS),
        (0b10100, 1, 0, _) => Some(Id::FleD),
        (0b10100, 0, 1, _) => Some(Id::FltS),
        (0b10100, 1, 1, _) => Some(Id::FltD),
        (0b10100, 0, 2, _) => Some(Id::FeqS),
        (0b10100, 1, 2, _) => Some(Id::FeqD),
        (0b11000, 0, _, 0) => Some(Id::FcvtWS),
        (0b11000, 0, _, 1) => Some(Id::FcvtWuS),
        (0b11000, 0, _, 2) => Some(Id::FcvtLS),
        (0b11000, 0, _, 3) => Some(Id::FcvtLuS),
        (0b11000, 1, _, 0) => Some(Id::FcvtWD),
        (0b11000, 1, _, 1) => Some(Id::FcvtWuD),
        (0b11000, 1, _, 2) => Some(Id::FcvtLD),
        (0b11000, 1, _, 3) => Some(Id::FcvtLuD),
        (0b11010, 0, _, 0) => Some(Id::FcvtSW),
        (0b11010, 0, _, 1) => Some(Id::FcvtSWu),
        (0b11010, 0, _, 2) => Some(Id::FcvtSL),
        (0b11010, 0, _, 3) => Some(Id::FcvtSLu),
        (0b11010, 1, _, 0) => Some(Id::FcvtDW),
        (0b11010, 1, _, 1) => Some(Id::FcvtDWu),
        (0b11010, 1, _, 2) => Some(Id::FcvtDL),
        (0b11010, 1, _, 3) => Some(Id::FcvtDLu),
        (0b01000, 0, _, _) => Some(Id::FcvtSD),
        (0b01001, 1, _, _) => Some(Id::FcvtDS),
        (0b11100, 0, 0, _) => Some(Id::FmvXW),
        (0b11100, 1, 0, _) => Some(Id::FmvXD),
        (0b11100, 0, 1, _) => Some(Id::FclassS),
        (0b11100, 1, 1, _) => Some(Id::FclassD),
        (0b11110, 0, _, _) => Some(Id::FmvWX),
        (0b11110, 1, _, _) => Some(Id::FmvDX),
        _ => None,
    }
}

fn fmac_instruction_id(op: &FMacOp, fmt: u8) -> Option<InstructionId> {
    use InstructionId as Id;

    match (op, fmt) {
        (FMacOp::Fmadd, 0) => Some(Id::FmaddS),
        (FMacOp::Fmadd, 1) => Some(Id::FmaddD),
        (FMacOp::Fmsub, 0) => Some(Id::FmsubS),
        (FMacOp::Fmsub, 1) => Some(Id::FmsubD),
        (FMacOp::Fnmsub, 0) => Some(Id::FnmsubS),
        (FMacOp::Fnmsub, 1) => Some(Id::FnmsubD),
        (FMacOp::Fnmadd, 0) => Some(Id::FnmaddS),
        (FMacOp::Fnmadd, 1) => Some(Id::FnmaddD),
        _ => None,
    }
}

fn atomic_instruction_id(funct5: u8, funct3: u8) -> Option<InstructionId> {
    use InstructionId as Id;

    match (funct5, funct3) {
        (2, 2) => Some(Id::LrW),
        (2, 3) => Some(Id::LrD),
        (3, 2) => Some(Id::ScW),
        (3, 3) => Some(Id::ScD),
        (1, 2) => Some(Id::AmoswapW),
        (1, 3) => Some(Id::AmoswapD),
        (0, 2) => Some(Id::AmoaddW),
        (0, 3) => Some(Id::AmoaddD),
        (4, 2) => Some(Id::AmoxorW),
        (4, 3) => Some(Id::AmoxorD),
        (8, 2) => Some(Id::AmoorW),
        (8, 3) => Some(Id::AmoorD),
        (12, 2) => Some(Id::AmoandW),
        (12, 3) => Some(Id::AmoandD),
        (16, 2) => Some(Id::AmominW),
        (16, 3) => Some(Id::AmominD),
        (20, 2) => Some(Id::AmomaxW),
        (20, 3) => Some(Id::AmomaxD),
        (24, 2) => Some(Id::AmominuW),
        (24, 3) => Some(Id::AmominuD),
        (28, 2) => Some(Id::AmomaxuW),
        (28, 3) => Some(Id::AmomaxuD),
        _ => None,
    }
}

// --- ABI register names for disassembly ---

fn reg_name(r: usize) -> &'static str {
    integer_register_name(r).unwrap_or("??")
}

fn freg_name(r: usize) -> &'static str {
    float_register_name(r).unwrap_or("f??")
}

// Decode the FOp mnemonic with format suffix (.s/.d).
fn fop_mnemonic(funct5: u8, fmt: u8) -> &'static str {
    match funct5 {
        0b00000 => {
            if fmt == 1 {
                "fadd.d"
            } else {
                "fadd.s"
            }
        }
        0b00001 => {
            if fmt == 1 {
                "fsub.d"
            } else {
                "fsub.s"
            }
        }
        0b00010 => {
            if fmt == 1 {
                "fmul.d"
            } else {
                "fmul.s"
            }
        }
        0b00011 => {
            if fmt == 1 {
                "fdiv.d"
            } else {
                "fdiv.s"
            }
        }
        0b01011 => {
            if fmt == 1 {
                "fsqrt.d"
            } else {
                "fsqrt.s"
            }
        }
        0b00100 => {
            if fmt == 1 {
                "fsgnj.d"
            } else {
                "fsgnj.s"
            }
        }
        0b00101 => {
            if fmt == 1 {
                "fsgnjn.d"
            } else {
                "fsgnjn.s"
            }
        }
        0b00110 => {
            if fmt == 1 {
                "fsgnjx.d"
            } else {
                "fsgnjx.s"
            }
        }
        0b10100 => {
            if fmt == 1 {
                "feq.d"
            } else {
                "feq.s"
            }
        }
        0b10101 => {
            if fmt == 1 {
                "flt.d"
            } else {
                "flt.s"
            }
        }
        0b10110 => {
            if fmt == 1 {
                "fle.d"
            } else {
                "fle.s"
            }
        }
        0b11000 => {
            if fmt == 1 {
                "fcvt.w.d"
            } else {
                "fcvt.w.s"
            }
        }
        0b11100 => {
            if fmt == 1 {
                "fcvt.d.w"
            } else {
                "fcvt.s.w"
            }
        }
        _ => {
            if fmt == 1 {
                "fop.d"
            } else {
                "fop.s"
            }
        }
    }
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
            Self::FOp {
                funct5,
                fmt,
                rd,
                rs1,
                rs2,
                ..
            } => {
                write!(
                    f,
                    "{:<10} {}, {}, {}",
                    fop_mnemonic(*funct5, *fmt),
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
    let opcode = word & 0x7F;
    match opcode {
        0x37 => Ok(DecodedInsn::Lui {
            rd: rd(word),
            imm: u_imm(word),
        }),
        0x17 => Ok(DecodedInsn::Auipc {
            rd: rd(word),
            imm: u_imm(word),
        }),
        0x6F => Ok(DecodedInsn::Jal {
            rd: rd(word),
            imm: j_imm(word),
        }),
        0x67 => Ok(DecodedInsn::Jalr {
            rd: rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x63 => Ok(DecodedInsn::Branch {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: b_imm(word),
        }),
        0x03 => Ok(DecodedInsn::Load {
            funct3: funct3(word),
            rd: rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x23 => Ok(DecodedInsn::Store {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: s_imm(word),
        }),
        // OP-IMM: pass the full bits[31:25] as funct7 so the execute stage can
        // disambiguate SRLI vs SRAI using bit 30 (and check for illegal encodings).
        0x13 => Ok(DecodedInsn::AluImm {
            funct3: funct3(word),
            funct7: field(word, 31, 25) as u8,
            rd: rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x1B => Ok(DecodedInsn::AluImm32 {
            funct3: funct3(word),
            funct7: field(word, 31, 25) as u8,
            rd: rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x33 => Ok(DecodedInsn::Alu {
            funct3: funct3(word),
            funct7: funct7(word),
            rd: rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        0x3B => Ok(DecodedInsn::Alu32 {
            funct3: funct3(word),
            funct7: funct7(word),
            rd: rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        0x0F => decode_fence(word),
        0x73 => decode_system(word),
        0x07 => Ok(DecodedInsn::FLoad {
            funct3: funct3(word),
            rd: rd(word),
            rs1: rs1(word),
            imm: i_imm(word),
        }),
        0x27 => Ok(DecodedInsn::FStore {
            funct3: funct3(word),
            rs1: rs1(word),
            rs2: rs2(word),
            imm: s_imm(word),
        }),
        0x53 => Ok(DecodedInsn::FOp {
            funct5: field(word, 31, 27) as u8,
            fmt: field(word, 26, 25) as u8,
            rm: funct3(word),
            rd: rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        0x43 => Ok(fmac(word, FMacOp::Fmadd)),
        0x47 => Ok(fmac(word, FMacOp::Fmsub)),
        0x4B => Ok(fmac(word, FMacOp::Fnmsub)),
        0x4F => Ok(fmac(word, FMacOp::Fnmadd)),
        0x2F => Ok(DecodedInsn::Atomic {
            funct5: field(word, 31, 27) as u8,
            aq: field(word, 26, 26) != 0,
            rl: field(word, 25, 25) != 0,
            funct3: funct3(word),
            rd: rd(word),
            rs1: rs1(word),
            rs2: rs2(word),
        }),
        _ => Err(VmError::IllegalInstruction(word)),
    }
}

// --- Sub-decoders ---

fn decode_fence(word: u32) -> Result<DecodedInsn, VmError> {
    match funct3(word) {
        0 => Ok(DecodedInsn::Fence {
            fm: field(word, 31, 28) as u8,
            pred: field(word, 27, 24) as u8,
            succ: field(word, 23, 20) as u8,
        }),
        1 => Ok(DecodedInsn::FenceI),
        _ => Err(VmError::IllegalInstruction(word)),
    }
}

fn decode_system(word: u32) -> Result<DecodedInsn, VmError> {
    // Check for SFENCE.VMA first (opcode 0x73, funct3=0, but with specific pattern)
    // SFENCE.VMA: imm[11:0]=0b0001_0001_0000, rs1 and rs2 can be non-zero
    if funct3(word) == 0 {
        let imm = field(word, 31, 20);
        match imm {
            0 => Ok(DecodedInsn::Ecall),
            1 => Ok(DecodedInsn::Ebreak),
            0x302 => Ok(DecodedInsn::Mret),
            0x102 => Ok(DecodedInsn::Sret),
            0x120 => Ok(DecodedInsn::SfenceVma), // SFENCE.VMA
            0x105 => Ok(DecodedInsn::Wfi),       // WFI
            _ => Err(VmError::IllegalInstruction(word)),
        }
    } else {
        // CSR instructions
        Ok(DecodedInsn::Csr {
            funct3: funct3(word),
            rd: rd(word),
            rs1_uimm: rs1(word), // for CSRRWI/CSRRSI/CSRRCI this is the uimm[4:0]
            csr: field(word, 31, 20) as u16,
        })
    }
}

fn fmac(word: u32, op: FMacOp) -> DecodedInsn {
    DecodedInsn::FMac {
        op,
        rs3: field(word, 31, 27) as usize,
        fmt: field(word, 26, 25) as u8,
        rm: funct3(word),
        rd: rd(word),
        rs1: rs1(word),
        rs2: rs2(word),
    }
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
fn rd(word: u32) -> usize {
    field(word, 11, 7) as usize
}
#[inline]
fn rs1(word: u32) -> usize {
    field(word, 19, 15) as usize
}
#[inline]
fn rs2(word: u32) -> usize {
    field(word, 24, 20) as usize
}
#[inline]
fn funct3(word: u32) -> u8 {
    field(word, 14, 12) as u8
}
#[inline]
fn funct7(word: u32) -> u8 {
    field(word, 31, 25) as u8
}

// Sign-extend `val` (a value in the low `bits` bits) to `i64`.
fn sign_ext(val: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    ((val << shift) as i32 >> shift) as i64
}

// I-type immediate: bits[31:20], sign-extended to 64 bits.
fn i_imm(word: u32) -> i64 {
    sign_ext(field(word, 31, 20), 12)
}

// U-type immediate: upper 20 bits placed at [31:12], lower 12 bits zero.
// We sign-extend from the 32-bit word so that large addresses work correctly.
fn u_imm(word: u32) -> i64 {
    (word & 0xFFFF_F000) as i32 as i64
}

// S-type immediate: bits[31:25] || bits[11:7], sign-extended to 12 bits.
fn s_imm(word: u32) -> i64 {
    let hi = field(word, 31, 25);
    let lo = field(word, 11, 7);
    sign_ext((hi << 5) | lo, 12)
}

// B-type immediate: [31]<<12 | [7]<<11 | [30:25]<<5 | [11:8]<<1, sign-extended to 13 bits.
fn b_imm(word: u32) -> i64 {
    let bit12 = field(word, 31, 31) << 12;
    let bit11 = field(word, 7, 7) << 11;
    let bits10_5 = field(word, 30, 25) << 5;
    let bits4_1 = field(word, 11, 8) << 1;
    sign_ext(bit12 | bit11 | bits10_5 | bits4_1, 13)
}

// J-type immediate: [31]<<20 | [19:12]<<12 | [20]<<11 | [30:21]<<1, sign-extended to 21 bits.
fn j_imm(word: u32) -> i64 {
    let bit20 = field(word, 31, 31) << 20;
    let bits19_12 = field(word, 19, 12) << 12;
    let bit11 = field(word, 20, 20) << 11;
    let bits10_1 = field(word, 30, 21) << 1;
    sign_ext(bit20 | bits19_12 | bit11 | bits10_1, 21)
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
