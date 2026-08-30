//! Shared, dependency-light definitions for the supported RISC-V instruction set.

/// The architectural register file used by an instruction operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterFile {
    Integer,
    FloatingPoint,
}

/// Explicit register operands used by an instruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegisterUse {
    pub source1: Option<RegisterFile>,
    pub source2: Option<RegisterFile>,
    pub source3: Option<RegisterFile>,
    pub destination: Option<RegisterFile>,
}

impl RegisterUse {
    const NONE: Self = Self {
        source1: None,
        source2: None,
        source3: None,
        destination: None,
    };
}

/// Canonical identity of a real instruction supported by the current VM.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InstructionId {
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Ld,
    Lbu,
    Lhu,
    Lwu,
    Sb,
    Sh,
    Sw,
    Sd,
    Addi,
    Slli,
    Slti,
    Sltiu,
    Xori,
    Srli,
    Srai,
    Ori,
    Andi,
    Addiw,
    Slliw,
    Srliw,
    Sraiw,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
    Addw,
    Subw,
    Sllw,
    Srlw,
    Sraw,
    Mulw,
    Divw,
    Divuw,
    Remw,
    Remuw,
    Fence,
    FenceI,
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Wfi,
    Csrrw,
    Csrrs,
    Csrrc,
    Csrrwi,
    Csrrsi,
    Csrrci,
    Flw,
    Fld,
    Fsw,
    Fsd,
    FaddS,
    FaddD,
    FsubS,
    FsubD,
    FmulS,
    FmulD,
    FdivS,
    FdivD,
    FsqrtS,
    FsqrtD,
    FsgnjS,
    FsgnjD,
    FsgnjnS,
    FsgnjnD,
    FsgnjxS,
    FsgnjxD,
    FminS,
    FminD,
    FmaxS,
    FmaxD,
    FleS,
    FleD,
    FltS,
    FltD,
    FeqS,
    FeqD,
    FcvtWS,
    FcvtWuS,
    FcvtLS,
    FcvtLuS,
    FcvtWD,
    FcvtWuD,
    FcvtLD,
    FcvtLuD,
    FcvtSW,
    FcvtSWu,
    FcvtSL,
    FcvtSLu,
    FcvtDW,
    FcvtDWu,
    FcvtDL,
    FcvtDLu,
    FcvtSD,
    FcvtDS,
    FmvXW,
    FmvXD,
    FclassS,
    FclassD,
    FmvWX,
    FmvDX,
    FmaddS,
    FmaddD,
    FmsubS,
    FmsubD,
    FnmsubS,
    FnmsubD,
    FnmaddS,
    FnmaddD,
    LrW,
    LrD,
    ScW,
    ScD,
    AmoswapW,
    AmoswapD,
    AmoaddW,
    AmoaddD,
    AmoxorW,
    AmoxorD,
    AmoorW,
    AmoorD,
    AmoandW,
    AmoandD,
    AmominW,
    AmominD,
    AmomaxW,
    AmomaxD,
    AmominuW,
    AmominuD,
    AmomaxuW,
    AmomaxuD,
}

impl InstructionId {
    /// Returns the canonical assembly mnemonic.
    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive identity table is easier to audit in one place"
    )]
    pub const fn mnemonic(self) -> &'static str {
        use InstructionId::{
            Add as AddId, Addi, Addiw, Addw, AmoaddD, AmoaddW, AmoandD, AmoandW, AmomaxD, AmomaxW,
            AmomaxuD, AmomaxuW, AmominD, AmominW, AmominuD, AmominuW, AmoorD, AmoorW, AmoswapD,
            AmoswapW, AmoxorD, AmoxorW, And as AndId, Andi, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne,
            Csrrc, Csrrci, Csrrs, Csrrsi, Csrrw, Csrrwi, Div, Divu, Divuw, Divw, Ebreak, Ecall,
            FaddD, FaddS, FclassD, FclassS, FcvtDL, FcvtDLu, FcvtDS, FcvtDW, FcvtDWu, FcvtLD,
            FcvtLS, FcvtLuD, FcvtLuS, FcvtSD, FcvtSL, FcvtSLu, FcvtSW, FcvtSWu, FcvtWD, FcvtWS,
            FcvtWuD, FcvtWuS, FdivD, FdivS, Fence, FenceI, FeqD, FeqS, Fld, FleD, FleS, FltD, FltS,
            Flw, FmaddD, FmaddS, FmaxD, FmaxS, FminD, FminS, FmsubD, FmsubS, FmulD, FmulS, FmvDX,
            FmvWX, FmvXD, FmvXW, FnmaddD, FnmaddS, FnmsubD, FnmsubS, Fsd, FsgnjD, FsgnjS, FsgnjnD,
            FsgnjnS, FsgnjxD, FsgnjxS, FsqrtD, FsqrtS, FsubD, FsubS, Fsw, Jal, Jalr, Lb, Lbu, Ld,
            Lh, Lhu, LrD, LrW, Lui, Lw, Lwu, Mret, Mul, Mulh, Mulhsu, Mulhu, Mulw, Or as OrId, Ori,
            Rem, Remu, Remuw, Remw, Sb, ScD, ScW, Sd, SfenceVma, Sh, Sll, Slli, Slliw, Sllw, Slt,
            Slti, Sltiu, Sltu, Sra, Srai, Sraiw, Sraw, Sret, Srl, Srli, Srliw, Srlw, Sub, Subw, Sw,
            Wfi, Xor as XorId, Xori,
        };

        match self {
            Lui => "lui",
            Auipc => "auipc",
            Jal => "jal",
            Jalr => "jalr",
            Beq => "beq",
            Bne => "bne",
            Blt => "blt",
            Bge => "bge",
            Bltu => "bltu",
            Bgeu => "bgeu",
            Lb => "lb",
            Lh => "lh",
            Lw => "lw",
            Ld => "ld",
            Lbu => "lbu",
            Lhu => "lhu",
            Lwu => "lwu",
            Sb => "sb",
            Sh => "sh",
            Sw => "sw",
            Sd => "sd",
            Addi => "addi",
            Slli => "slli",
            Slti => "slti",
            Sltiu => "sltiu",
            Xori => "xori",
            Srli => "srli",
            Srai => "srai",
            Ori => "ori",
            Andi => "andi",
            Addiw => "addiw",
            Slliw => "slliw",
            Srliw => "srliw",
            Sraiw => "sraiw",
            AddId => "add",
            Sub => "sub",
            Sll => "sll",
            Slt => "slt",
            Sltu => "sltu",
            XorId => "xor",
            Srl => "srl",
            Sra => "sra",
            OrId => "or",
            AndId => "and",
            Mul => "mul",
            Mulh => "mulh",
            Mulhsu => "mulhsu",
            Mulhu => "mulhu",
            Div => "div",
            Divu => "divu",
            Rem => "rem",
            Remu => "remu",
            Addw => "addw",
            Subw => "subw",
            Sllw => "sllw",
            Srlw => "srlw",
            Sraw => "sraw",
            Mulw => "mulw",
            Divw => "divw",
            Divuw => "divuw",
            Remw => "remw",
            Remuw => "remuw",
            Fence => "fence",
            FenceI => "fence.i",
            Ecall => "ecall",
            Ebreak => "ebreak",
            Mret => "mret",
            Sret => "sret",
            SfenceVma => "sfence.vma",
            Wfi => "wfi",
            Csrrw => "csrrw",
            Csrrs => "csrrs",
            Csrrc => "csrrc",
            Csrrwi => "csrrwi",
            Csrrsi => "csrrsi",
            Csrrci => "csrrci",
            Flw => "flw",
            Fld => "fld",
            Fsw => "fsw",
            Fsd => "fsd",
            FaddS => "fadd.s",
            FaddD => "fadd.d",
            FsubS => "fsub.s",
            FsubD => "fsub.d",
            FmulS => "fmul.s",
            FmulD => "fmul.d",
            FdivS => "fdiv.s",
            FdivD => "fdiv.d",
            FsqrtS => "fsqrt.s",
            FsqrtD => "fsqrt.d",
            FsgnjS => "fsgnj.s",
            FsgnjD => "fsgnj.d",
            FsgnjnS => "fsgnjn.s",
            FsgnjnD => "fsgnjn.d",
            FsgnjxS => "fsgnjx.s",
            FsgnjxD => "fsgnjx.d",
            FminS => "fmin.s",
            FminD => "fmin.d",
            FmaxS => "fmax.s",
            FmaxD => "fmax.d",
            FleS => "fle.s",
            FleD => "fle.d",
            FltS => "flt.s",
            FltD => "flt.d",
            FeqS => "feq.s",
            FeqD => "feq.d",
            FcvtWS => "fcvt.w.s",
            FcvtWuS => "fcvt.wu.s",
            FcvtLS => "fcvt.l.s",
            FcvtLuS => "fcvt.lu.s",
            FcvtWD => "fcvt.w.d",
            FcvtWuD => "fcvt.wu.d",
            FcvtLD => "fcvt.l.d",
            FcvtLuD => "fcvt.lu.d",
            FcvtSW => "fcvt.s.w",
            FcvtSWu => "fcvt.s.wu",
            FcvtSL => "fcvt.s.l",
            FcvtSLu => "fcvt.s.lu",
            FcvtDW => "fcvt.d.w",
            FcvtDWu => "fcvt.d.wu",
            FcvtDL => "fcvt.d.l",
            FcvtDLu => "fcvt.d.lu",
            FcvtSD => "fcvt.s.d",
            FcvtDS => "fcvt.d.s",
            FmvXW => "fmv.x.w",
            FmvXD => "fmv.x.d",
            FclassS => "fclass.s",
            FclassD => "fclass.d",
            FmvWX => "fmv.w.x",
            FmvDX => "fmv.d.x",
            FmaddS => "fmadd.s",
            FmaddD => "fmadd.d",
            FmsubS => "fmsub.s",
            FmsubD => "fmsub.d",
            FnmsubS => "fnmsub.s",
            FnmsubD => "fnmsub.d",
            FnmaddS => "fnmadd.s",
            FnmaddD => "fnmadd.d",
            LrW => "lr.w",
            LrD => "lr.d",
            ScW => "sc.w",
            ScD => "sc.d",
            AmoswapW => "amoswap.w",
            AmoswapD => "amoswap.d",
            AmoaddW => "amoadd.w",
            AmoaddD => "amoadd.d",
            AmoxorW => "amoxor.w",
            AmoxorD => "amoxor.d",
            AmoorW => "amoor.w",
            AmoorD => "amoor.d",
            AmoandW => "amoand.w",
            AmoandD => "amoand.d",
            AmominW => "amomin.w",
            AmominD => "amomin.d",
            AmomaxW => "amomax.w",
            AmomaxD => "amomax.d",
            AmominuW => "amominu.w",
            AmominuD => "amominu.d",
            AmomaxuW => "amomaxu.w",
            AmomaxuD => "amomaxu.d",
        }
    }

    /// Returns explicit register-file use for hazard and forwarding logic.
    #[cfg_attr(
        not(test),
        expect(
            clippy::enum_glob_use,
            reason = "the exhaustive identity table reads as an instruction vocabulary"
        )
    )]
    pub const fn register_use(self) -> RegisterUse {
        use InstructionId::*;
        use RegisterFile::{FloatingPoint as Fp, Integer as Int};

        let int_rd = RegisterUse {
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let int_rs1_rd = RegisterUse {
            source1: Some(Int),
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let int_rs1_rs2_rd = RegisterUse {
            source1: Some(Int),
            source2: Some(Int),
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let fp_rs1_rs2_rd = RegisterUse {
            source1: Some(Fp),
            source2: Some(Fp),
            destination: Some(Fp),
            ..RegisterUse::NONE
        };

        match self {
            Lui | Auipc | Jal | Csrrwi | Csrrsi | Csrrci => int_rd,
            Jalr | Lb | Lh | Lw | Ld | Lbu | Lhu | Lwu | Addi | Slli | Slti | Sltiu | Xori
            | Srli | Srai | Ori | Andi | Addiw | Slliw | Srliw | Sraiw | Csrrw | Csrrs | Csrrc
            | LrW | LrD => int_rs1_rd,
            Beq | Bne | Blt | Bge | Bltu | Bgeu | Sb | Sh | Sw | Sd => RegisterUse {
                source1: Some(Int),
                source2: Some(Int),
                ..RegisterUse::NONE
            },
            Add | Sub | Sll | Slt | Sltu | Xor | Srl | Sra | Or | And | Mul | Mulh | Mulhsu
            | Mulhu | Div | Divu | Rem | Remu | Addw | Subw | Sllw | Srlw | Sraw | Mulw | Divw
            | Divuw | Remw | Remuw | ScW | ScD | AmoswapW | AmoswapD | AmoaddW | AmoaddD
            | AmoxorW | AmoxorD | AmoorW | AmoorD | AmoandW | AmoandD | AmominW | AmominD
            | AmomaxW | AmomaxD | AmominuW | AmominuD | AmomaxuW | AmomaxuD => int_rs1_rs2_rd,
            Fence | FenceI | Ecall | Ebreak | Mret | Sret | SfenceVma | Wfi => RegisterUse::NONE,
            Flw | Fld | FcvtSW | FcvtSWu | FcvtSL | FcvtSLu | FcvtDW | FcvtDWu | FcvtDL
            | FcvtDLu | FmvWX | FmvDX => RegisterUse {
                source1: Some(Int),
                destination: Some(Fp),
                ..RegisterUse::NONE
            },
            Fsw | Fsd => RegisterUse {
                source1: Some(Int),
                source2: Some(Fp),
                ..RegisterUse::NONE
            },
            FaddS | FaddD | FsubS | FsubD | FmulS | FmulD | FdivS | FdivD | FsgnjS | FsgnjD
            | FsgnjnS | FsgnjnD | FsgnjxS | FsgnjxD | FminS | FminD | FmaxS | FmaxD => {
                fp_rs1_rs2_rd
            }
            FsqrtS | FsqrtD | FcvtSD | FcvtDS => RegisterUse {
                source1: Some(Fp),
                destination: Some(Fp),
                ..RegisterUse::NONE
            },
            FleS | FleD | FltS | FltD | FeqS | FeqD | FcvtWS | FcvtWuS | FcvtLS | FcvtLuS
            | FcvtWD | FcvtWuD | FcvtLD | FcvtLuD | FmvXW | FmvXD | FclassS | FclassD => {
                RegisterUse {
                    source1: Some(Fp),
                    destination: Some(Int),
                    ..RegisterUse::NONE
                }
            }
            FmaddS | FmaddD | FmsubS | FmsubD | FnmsubS | FnmsubD | FnmaddS | FnmaddD => {
                RegisterUse {
                    source1: Some(Fp),
                    source2: Some(Fp),
                    source3: Some(Fp),
                    destination: Some(Fp),
                }
            }
        }
    }

    /// Returns whether memory produces the destination value after execute.
    pub const fn is_load(self) -> bool {
        matches!(
            self,
            Self::Lb
                | Self::Lh
                | Self::Lw
                | Self::Ld
                | Self::Lbu
                | Self::Lhu
                | Self::Lwu
                | Self::Flw
                | Self::Fld
        )
    }

    /// Returns whether this is an atomic memory instruction.
    pub const fn is_atomic(self) -> bool {
        matches!(
            self,
            Self::LrW
                | Self::LrD
                | Self::ScW
                | Self::ScD
                | Self::AmoswapW
                | Self::AmoswapD
                | Self::AmoaddW
                | Self::AmoaddD
                | Self::AmoxorW
                | Self::AmoxorD
                | Self::AmoorW
                | Self::AmoorD
                | Self::AmoandW
                | Self::AmoandD
                | Self::AmominW
                | Self::AmominD
                | Self::AmomaxW
                | Self::AmomaxD
                | Self::AmominuW
                | Self::AmominuD
                | Self::AmomaxuW
                | Self::AmomaxuD
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{InstructionId, RegisterFile};

    #[test]
    fn instruction_metadata_distinguishes_mixed_register_files() {
        let load = InstructionId::Fld.register_use();
        assert_eq!(load.source1, Some(RegisterFile::Integer));
        assert_eq!(load.destination, Some(RegisterFile::FloatingPoint));

        let conversion = InstructionId::FcvtDW.register_use();
        assert_eq!(conversion.source1, Some(RegisterFile::Integer));
        assert_eq!(conversion.destination, Some(RegisterFile::FloatingPoint));

        let compare = InstructionId::FltD.register_use();
        assert_eq!(compare.source1, Some(RegisterFile::FloatingPoint));
        assert_eq!(compare.destination, Some(RegisterFile::Integer));
    }

    #[test]
    fn memory_classes_cover_integer_fp_and_atomic_loads() {
        assert!(InstructionId::Ld.is_load());
        assert!(InstructionId::Flw.is_load());
        assert!(!InstructionId::LrD.is_load());
        assert!(InstructionId::LrD.is_atomic());
        assert!(!InstructionId::Ld.is_atomic());
    }
}
