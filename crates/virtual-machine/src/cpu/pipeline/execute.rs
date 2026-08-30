//! Execute stage, computes results without touching memory or registers.

use crate::cpu::alu::{self, RM_DYN};
use crate::cpu::csr::CsrFile;
use crate::cpu::decoder::DecodedInsn;
use crate::cpu::registers::Registers;
use crate::error::VmError;
use riscv_model::InstructionId;

pub(crate) struct ExecuteOperands<'a> {
    registers: &'a Registers,
    rs1: (usize, u64),
    rs2: (usize, u64),
    frs1: (usize, u64),
    frs2: (usize, u64),
}

impl<'a> ExecuteOperands<'a> {
    pub(crate) fn new(
        registers: &'a Registers,
        rs1: (usize, u64),
        rs2: (usize, u64),
        frs1: (usize, u64),
        frs2: (usize, u64),
    ) -> Self {
        Self {
            registers,
            rs1,
            rs2,
            frs1,
            frs2,
        }
    }

    fn read_x(&self, reg: usize) -> u64 {
        if reg == 0 {
            0
        } else if reg == self.rs1.0 {
            self.rs1.1
        } else if reg == self.rs2.0 {
            self.rs2.1
        } else {
            self.registers.read_x(reg)
        }
    }

    fn read_f_bits(&self, reg: usize) -> u64 {
        if reg == self.frs1.0 {
            self.frs1.1
        } else if reg == self.frs2.0 {
            self.frs2.1
        } else {
            self.registers.read_f_bits(reg)
        }
    }

    fn read_f32(&self, reg: usize) -> f32 {
        let bits = self.read_f_bits(reg);
        if bits & 0xFFFF_FFFF_0000_0000 != 0xFFFF_FFFF_0000_0000 {
            f32::NAN
        } else {
            f32::from_bits(bits as u32)
        }
    }

    fn read_f64(&self, reg: usize) -> f64 {
        f64::from_bits(self.read_f_bits(reg))
    }

    fn privilege_mode(&self) -> crate::cpu::registers::PrivilegeMode {
        self.registers.priv_mode
    }
}

// --- Public result type ---

#[derive(Clone, Debug)]
pub enum ExecResult {
    WriteInt {
        rd: usize,
        val: u64,
        next_pc: u64,
    },
    WriteFp {
        rd: usize,
        bits: u64,
        next_pc: u64,
    },
    WriteIntFlags {
        rd: usize,
        val: u64,
        fflags: u8,
        next_pc: u64,
    },
    WriteFpFlags {
        rd: usize,
        bits: u64,
        fflags: u8,
        next_pc: u64,
    },
    Jump {
        next_pc: u64,
    },
    Load {
        rd: usize,
        addr: u64,
        funct3: u8,
        next_pc: u64,
    },
    Store {
        addr: u64,
        val: u64,
        funct3: u8,
        next_pc: u64,
    },
    FLoad {
        rd: usize,
        addr: u64,
        funct3: u8,
        next_pc: u64,
    },
    FStore {
        addr: u64,
        bits: u64,
        funct3: u8,
        next_pc: u64,
    },
    Atomic {
        funct5: u8,
        aq: bool,
        rl: bool,
        funct3: u8,
        rd: usize,
        addr: u64,
        val: u64,
        next_pc: u64,
    },
    Csr {
        funct3: u8,
        rd: usize,
        rs1_uimm: usize,
        csr: u16,
        operand: u64,
        /// Old CSR value read in EX stage, forwarded to downstream instructions.
        old_val: u64,
        next_pc: u64,
    },
    Fence {
        next_pc: u64,
    },
    FenceI {
        next_pc: u64,
    },
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Wfi {
        next_pc: u64,
    },
}

// --- Public entry point ---

pub(crate) fn execute(
    insn: &DecodedInsn,
    regs: &ExecuteOperands<'_>,
    csrs: &CsrFile,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let instruction_id = insn.instruction_id();

    match insn {
        DecodedInsn::Lui { rd, imm } => Ok(ExecResult::WriteInt {
            rd: *rd,
            val: *imm as u64,
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::Auipc { rd, imm } => Ok(ExecResult::WriteInt {
            rd: *rd,
            val: pc.wrapping_add(*imm as u64),
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::Jal { rd, imm } => Ok(ExecResult::WriteInt {
            rd: *rd,
            val: pc.wrapping_add(4),
            next_pc: pc.wrapping_add(*imm as u64),
        }),

        DecodedInsn::Jalr { rd, rs1, imm } => {
            let rs1_val = regs.read_x(*rs1);
            let target = rs1_val.wrapping_add(*imm as u64) & !1u64;
            Ok(ExecResult::WriteInt {
                rd: *rd,
                val: pc.wrapping_add(4),
                next_pc: target,
            })
        }

        DecodedInsn::Branch { rs1, rs2, imm, .. } => {
            let lhs = regs.read_x(*rs1);
            let rhs = regs.read_x(*rs2);
            let taken = match instruction_id {
                Some(InstructionId::Beq) => lhs == rhs,
                Some(InstructionId::Bne) => lhs != rhs,
                Some(InstructionId::Blt) => (lhs as i64) < (rhs as i64),
                Some(InstructionId::Bge) => (lhs as i64) >= (rhs as i64),
                Some(InstructionId::Bltu) => lhs < rhs,
                Some(InstructionId::Bgeu) => lhs >= rhs,
                _ => return Err(VmError::IllegalInstruction(0)),
            };
            let next_pc = if taken {
                pc.wrapping_add(*imm as u64)
            } else {
                pc.wrapping_add(4)
            };
            Ok(ExecResult::Jump { next_pc })
        }

        DecodedInsn::Load {
            funct3,
            rd,
            rs1,
            imm,
            ..
        } => {
            let addr = regs.read_x(*rs1).wrapping_add(*imm as u64);
            Ok(ExecResult::Load {
                rd: *rd,
                addr,
                funct3: *funct3,
                next_pc: pc.wrapping_add(4),
            })
        }

        DecodedInsn::Store {
            funct3,
            rs1,
            rs2,
            imm,
        } => {
            let addr = regs.read_x(*rs1).wrapping_add(*imm as u64);
            let val = regs.read_x(*rs2);
            Ok(ExecResult::Store {
                addr,
                val,
                funct3: *funct3,
                next_pc: pc.wrapping_add(4),
            })
        }

        DecodedInsn::AluImm {
            funct3,
            rd,
            rs1,
            imm,
            ..
        } => exec_alu_imm(instruction_id, *funct3, *rd, *rs1, *imm, regs, pc),

        DecodedInsn::AluImm32 {
            funct3,
            rd,
            rs1,
            imm,
            ..
        } => exec_alu_imm32(instruction_id, *funct3, *rd, *rs1, *imm, regs, pc),

        DecodedInsn::Alu {
            funct3,
            rd,
            rs1,
            rs2,
            ..
        } => exec_alu(instruction_id, *funct3, *rd, *rs1, *rs2, regs, pc),

        DecodedInsn::Alu32 {
            funct3,
            rd,
            rs1,
            rs2,
            ..
        } => exec_alu32(instruction_id, *funct3, *rd, *rs1, *rs2, regs, pc),

        DecodedInsn::Fence { .. } => Ok(ExecResult::Fence {
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::FenceI => Ok(ExecResult::FenceI {
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::Ecall => Ok(ExecResult::Ecall),

        DecodedInsn::Ebreak => Ok(ExecResult::Ebreak),

        DecodedInsn::Mret => Ok(ExecResult::Mret),

        DecodedInsn::Sret => Ok(ExecResult::Sret),

        DecodedInsn::SfenceVma => Ok(ExecResult::SfenceVma),

        DecodedInsn::Wfi => Ok(ExecResult::Wfi {
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::Csr {
            funct3,
            rd,
            rs1_uimm,
            csr,
        } => {
            // CSR address bits [9:8] encode the minimum privilege required (0=U,1=S,3=M).
            let required_priv = ((*csr as u64) >> 8) & 0x3;
            if (regs.privilege_mode() as u64) < required_priv {
                return Err(VmError::IllegalInstruction(*csr as u32));
            }
            // Bits [11:10] == 0b11 marks a read-only CSR; writes are illegal.
            let is_write = match instruction_id {
                Some(InstructionId::Csrrw | InstructionId::Csrrwi) => true,
                Some(
                    InstructionId::Csrrs
                    | InstructionId::Csrrc
                    | InstructionId::Csrrsi
                    | InstructionId::Csrrci,
                ) => *rs1_uimm != 0,
                _ => false,
            };
            if is_write && ((*csr as u64) >> 10) & 0x3 == 0x3 {
                return Err(VmError::IllegalInstruction(*csr as u32));
            }
            let operand = match instruction_id {
                Some(InstructionId::Csrrw | InstructionId::Csrrs | InstructionId::Csrrc) => {
                    regs.read_x(*rs1_uimm)
                }
                Some(InstructionId::Csrrwi | InstructionId::Csrrsi | InstructionId::Csrrci) => {
                    *rs1_uimm as u64
                }
                _ => return Err(VmError::IllegalInstruction(*funct3 as u32)),
            };
            let old_val = csrs.read(*csr).unwrap_or(0);
            Ok(ExecResult::Csr {
                funct3: *funct3,
                rd: *rd,
                rs1_uimm: *rs1_uimm,
                csr: *csr,
                operand,
                old_val,
                next_pc: pc.wrapping_add(4),
            })
        }

        DecodedInsn::FLoad {
            funct3,
            rd,
            rs1,
            imm,
        } => {
            let addr = regs.read_x(*rs1).wrapping_add(*imm as u64);
            Ok(ExecResult::FLoad {
                rd: *rd,
                addr,
                funct3: *funct3,
                next_pc: pc.wrapping_add(4),
            })
        }

        DecodedInsn::FStore {
            funct3,
            rs1,
            rs2,
            imm,
        } => {
            let addr = regs.read_x(*rs1).wrapping_add(*imm as u64);
            let bits = regs.read_f_bits(*rs2);
            Ok(ExecResult::FStore {
                addr,
                bits,
                funct3: *funct3,
                next_pc: pc.wrapping_add(4),
            })
        }

        DecodedInsn::FOp {
            funct5,
            fmt,
            rm,
            rd,
            rs1,
            rs2,
        } => exec_fp_op(
            instruction_id,
            *funct5,
            *fmt,
            *rm,
            *rd,
            *rs1,
            *rs2,
            regs,
            csrs,
            pc,
        ),

        DecodedInsn::FMac {
            fmt,
            rm,
            rd,
            rs1,
            rs2,
            rs3,
            ..
        } => exec_fmac(
            instruction_id,
            *fmt,
            *rm,
            *rd,
            *rs1,
            *rs2,
            *rs3,
            regs,
            csrs,
            pc,
        ),

        DecodedInsn::Atomic {
            funct5,
            aq,
            rl,
            funct3,
            rd,
            rs1,
            rs2,
        } => {
            let addr = regs.read_x(*rs1);
            let val = regs.read_x(*rs2);
            Ok(ExecResult::Atomic {
                funct5: *funct5,
                aq: *aq,
                rl: *rl,
                funct3: *funct3,
                rd: *rd,
                addr,
                val,
                next_pc: pc.wrapping_add(4),
            })
        }
    }
}

// --- ALU helpers ---

fn exec_alu_imm(
    instruction_id: Option<InstructionId>,
    funct3: u8,
    rd: usize,
    rs1: usize,
    imm: i64,
    regs: &ExecuteOperands<'_>,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let val = regs.read_x(rs1);
    let imm_u = imm as u64;
    let result = match instruction_id {
        Some(InstructionId::Addi) => alu::add(val, imm_u),
        Some(InstructionId::Slli) => alu::sll(val, (imm & 63) as u64),
        Some(InstructionId::Slti) => alu::slt(val, imm_u),
        Some(InstructionId::Sltiu) => alu::sltu(val, imm_u),
        Some(InstructionId::Xori) => alu::xor(val, imm_u),
        Some(InstructionId::Srli) => alu::srl(val, (imm & 63) as u64),
        Some(InstructionId::Srai) => alu::sra(val, (imm & 63) as u64),
        Some(InstructionId::Ori) => alu::or(val, imm_u),
        Some(InstructionId::Andi) => alu::and(val, imm_u),
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu_imm32(
    instruction_id: Option<InstructionId>,
    funct3: u8,
    rd: usize,
    rs1: usize,
    imm: i64,
    regs: &ExecuteOperands<'_>,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let val = regs.read_x(rs1);
    let result = match instruction_id {
        Some(InstructionId::Addiw) => alu::addw(val, imm as u64),
        Some(InstructionId::Slliw) => alu::sllw(val, (imm & 31) as u64),
        Some(InstructionId::Srliw) => alu::srlw(val, (imm & 31) as u64),
        Some(InstructionId::Sraiw) => alu::sraw(val, (imm & 31) as u64),
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu(
    instruction_id: Option<InstructionId>,
    funct3: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &ExecuteOperands<'_>,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let a = regs.read_x(rs1);
    let b = regs.read_x(rs2);
    let result = match instruction_id {
        Some(InstructionId::Add) => alu::add(a, b),
        Some(InstructionId::Sub) => alu::sub(a, b),
        Some(InstructionId::Sll) => alu::sll(a, b),
        Some(InstructionId::Slt) => alu::slt(a, b),
        Some(InstructionId::Sltu) => alu::sltu(a, b),
        Some(InstructionId::Xor) => alu::xor(a, b),
        Some(InstructionId::Srl) => alu::srl(a, b),
        Some(InstructionId::Sra) => alu::sra(a, b),
        Some(InstructionId::Or) => alu::or(a, b),
        Some(InstructionId::And) => alu::and(a, b),
        Some(InstructionId::Mul) => alu::mul(a, b),
        Some(InstructionId::Mulh) => alu::mulh(a, b),
        Some(InstructionId::Mulhsu) => alu::mulhsu(a, b),
        Some(InstructionId::Mulhu) => alu::mulhu(a, b),
        Some(InstructionId::Div) => alu::div(a, b),
        Some(InstructionId::Divu) => alu::divu(a, b),
        Some(InstructionId::Rem) => alu::rem(a, b),
        Some(InstructionId::Remu) => alu::remu(a, b),
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu32(
    instruction_id: Option<InstructionId>,
    funct3: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &ExecuteOperands<'_>,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let a = regs.read_x(rs1);
    let b = regs.read_x(rs2);
    let result = match instruction_id {
        Some(InstructionId::Addw) => alu::addw(a, b),
        Some(InstructionId::Subw) => alu::subw(a, b),
        Some(InstructionId::Sllw) => alu::sllw(a, b),
        Some(InstructionId::Srlw) => alu::srlw(a, b),
        Some(InstructionId::Sraw) => alu::sraw(a, b),
        Some(InstructionId::Mulw) => alu::mulw(a, b),
        Some(InstructionId::Divw) => alu::divw(a, b),
        Some(InstructionId::Divuw) => alu::divuw(a, b),
        Some(InstructionId::Remw) => alu::remw(a, b),
        Some(InstructionId::Remuw) => alu::remuw(a, b),
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

// --- FP helpers ---

fn exec_fp_op(
    instruction_id: Option<InstructionId>,
    funct5: u8,
    fmt: u8,
    rm: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &ExecuteOperands<'_>,
    csrs: &CsrFile,
    pc: u64,
) -> Result<ExecResult, VmError> {
    // Resolve dynamic rounding mode
    let rm = if rm == RM_DYN {
        csrs.rounding_mode()
    } else {
        rm
    };
    let next_pc = pc.wrapping_add(4);

    match instruction_id {
        // FADD
        Some(InstructionId::FaddS | InstructionId::FaddD) => {
            let (bits, fflags) = match fmt {
                0 => alu::fp_add_s(regs.read_f32(rs1), regs.read_f32(rs2), rm),
                1 => alu::fp_add_d(regs.read_f64(rs1), regs.read_f64(rs2), rm),
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FSUB
        Some(InstructionId::FsubS | InstructionId::FsubD) => {
            let (bits, fflags) = match fmt {
                0 => alu::fp_sub_s(regs.read_f32(rs1), regs.read_f32(rs2), rm),
                1 => alu::fp_sub_d(regs.read_f64(rs1), regs.read_f64(rs2), rm),
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FMUL
        Some(InstructionId::FmulS | InstructionId::FmulD) => {
            let (bits, fflags) = match fmt {
                0 => alu::fp_mul_s(regs.read_f32(rs1), regs.read_f32(rs2), rm),
                1 => alu::fp_mul_d(regs.read_f64(rs1), regs.read_f64(rs2), rm),
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FDIV
        Some(InstructionId::FdivS | InstructionId::FdivD) => {
            let (bits, fflags) = match fmt {
                0 => alu::fp_div_s(regs.read_f32(rs1), regs.read_f32(rs2), rm),
                1 => alu::fp_div_d(regs.read_f64(rs1), regs.read_f64(rs2), rm),
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FSQRT
        Some(InstructionId::FsqrtS | InstructionId::FsqrtD) => {
            let (bits, fflags) = match fmt {
                0 => alu::fp_sqrt_s(regs.read_f32(rs1), rm),
                1 => alu::fp_sqrt_d(regs.read_f64(rs1), rm),
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FSGNJ family
        Some(
            InstructionId::FsgnjS
            | InstructionId::FsgnjD
            | InstructionId::FsgnjnS
            | InstructionId::FsgnjnD
            | InstructionId::FsgnjxS
            | InstructionId::FsgnjxD,
        ) => {
            let bits = match fmt {
                0 => match rm {
                    0 => alu::fp_sgnj_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    1 => alu::fp_sgnjn_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    2 => alu::fp_sgnjx_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                1 => match rm {
                    0 => alu::fp_sgnj_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    1 => alu::fp_sgnjn_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    2 => alu::fp_sgnjx_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFp { rd, bits, next_pc })
        }
        // FMIN/FMAX
        Some(
            InstructionId::FminS
            | InstructionId::FminD
            | InstructionId::FmaxS
            | InstructionId::FmaxD,
        ) => {
            let (bits, fflags) = match fmt {
                0 => match rm {
                    0 => alu::fp_min_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    1 => alu::fp_max_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                1 => match rm {
                    0 => alu::fp_min_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    1 => alu::fp_max_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FCMP: FLE/FLT/FEQ
        Some(
            InstructionId::FleS
            | InstructionId::FleD
            | InstructionId::FltS
            | InstructionId::FltD
            | InstructionId::FeqS
            | InstructionId::FeqD,
        ) => {
            let (val, fflags) = match fmt {
                0 => match rm {
                    0 => alu::fp_fle_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    1 => alu::fp_flt_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    2 => alu::fp_feq_s(regs.read_f32(rs1), regs.read_f32(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                1 => match rm {
                    0 => alu::fp_fle_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    1 => alu::fp_flt_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    2 => alu::fp_feq_d(regs.read_f64(rs1), regs.read_f64(rs2)),
                    _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                },
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteIntFlags {
                rd,
                val,
                fflags,
                next_pc,
            })
        }
        // FCVT.{W,WU,L,LU}.{S,D} , FP -> int
        Some(
            InstructionId::FcvtWS
            | InstructionId::FcvtWuS
            | InstructionId::FcvtLS
            | InstructionId::FcvtLuS
            | InstructionId::FcvtWD
            | InstructionId::FcvtWuD
            | InstructionId::FcvtLD
            | InstructionId::FcvtLuD,
        ) => {
            let (val, fflags) = match fmt {
                0 => {
                    let v = regs.read_f32(rs1);
                    match rs2 {
                        0 => sat_f32_to_i32(v, rm),
                        1 => sat_f32_to_u32(v, rm),
                        2 => sat_f32_to_i64(v, rm),
                        3 => sat_f32_to_u64(v, rm),
                        _ => return Err(VmError::IllegalInstruction(rs2 as u32)),
                    }
                }
                1 => {
                    let v = regs.read_f64(rs1);
                    match rs2 {
                        0 => sat_f64_to_i32(v, rm),
                        1 => sat_f64_to_u32(v, rm),
                        2 => sat_f64_to_i64(v, rm),
                        3 => sat_f64_to_u64(v, rm),
                        _ => return Err(VmError::IllegalInstruction(rs2 as u32)),
                    }
                }
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteIntFlags {
                rd,
                val,
                fflags,
                next_pc,
            })
        }
        // FCVT.{S,D}.{W,WU,L,LU} , int -> FP
        Some(
            InstructionId::FcvtSW
            | InstructionId::FcvtSWu
            | InstructionId::FcvtSL
            | InstructionId::FcvtSLu
            | InstructionId::FcvtDW
            | InstructionId::FcvtDWu
            | InstructionId::FcvtDL
            | InstructionId::FcvtDLu,
        ) => {
            let src = regs.read_x(rs1);
            let (bits, fflags) = match fmt {
                0 => {
                    // int -> f32: use f64 as exact intermediate, then apply rm.
                    let exact_f64: f64 = match rs2 {
                        0 => (src as i32) as f64, // i32 -> f64 is exact
                        1 => (src as u32) as f64, // u32 -> f64 is exact
                        2 => (src as i64) as f64, // i64 -> f64: exact for |v| <= 2^53
                        3 => src as f64,          // u64 -> f64: exact for v <= 2^53
                        _ => return Err(VmError::IllegalInstruction(rs2 as u32)),
                    };
                    let result = alu::f64_to_f32_with_rm(exact_f64, rm);
                    let fflags = alu::fp_flags_from_exact_s(exact_f64, result);
                    (0xFFFF_FFFF_0000_0000u64 | (result.to_bits() as u64), fflags)
                }
                1 => {
                    let result: f64 = match rs2 {
                        0 => (src as i32) as f64,
                        1 => (src as u32) as f64,
                        2 => (src as i64) as f64,
                        3 => src as f64,
                        _ => return Err(VmError::IllegalInstruction(rs2 as u32)),
                    };
                    // Detect NX for large integer -> f64 conversions that lose bits.
                    let fflags: u8 = match rs2 {
                        2 => u8::from((result as i64) != (src as i64)),
                        3 => u8::from((result as u64) != src),
                        _ => 0,
                    };
                    (result.to_bits(), fflags)
                }
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FCVT.S.D (fmt=0, rs2=1), convert f64 -> f32 with rounding + flags
        Some(InstructionId::FcvtSD) => {
            if fmt != 0 {
                return Err(VmError::IllegalInstruction(funct5 as u32));
            }
            let val_d = regs.read_f64(rs1);
            let (bits, fflags) = alu::fp_cvt_d_to_s(val_d, rm);
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits,
                fflags,
                next_pc,
            })
        }
        // FCVT.D.S (fmt=1, rs2=0), convert f32 -> f64; exact, no flags
        Some(InstructionId::FcvtDS) => {
            if fmt != 1 {
                return Err(VmError::IllegalInstruction(funct5 as u32));
            }
            let val_s = regs.read_f32(rs1);
            let val_d = val_s as f64;
            // f32 -> f64 is always exact; no exception flags (incl. for NaN).
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits: val_d.to_bits(),
                fflags: 0,
                next_pc,
            })
        }
        // FMV.X.W/D and FCLASS
        Some(
            InstructionId::FmvXW
            | InstructionId::FmvXD
            | InstructionId::FclassS
            | InstructionId::FclassD,
        ) => {
            match rm {
                0 => {
                    // FMV.X.{W,D}
                    let val = match fmt {
                        0 => {
                            // FMV.X.W: sign-extend lower 32 bits
                            let bits = regs.read_f_bits(rs1) as u32;
                            bits as i32 as i64 as u64
                        }
                        1 => {
                            // FMV.X.D: raw bits
                            regs.read_f_bits(rs1)
                        }
                        _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                    };
                    Ok(ExecResult::WriteInt { rd, val, next_pc })
                }
                1 => {
                    // FCLASS.{S,D}
                    let val = match fmt {
                        0 => alu::fp_fclass_s(regs.read_f32(rs1)),
                        1 => alu::fp_fclass_d(regs.read_f64(rs1)),
                        _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
                    };
                    Ok(ExecResult::WriteInt { rd, val, next_pc })
                }
                _ => Err(VmError::IllegalInstruction(funct5 as u32)),
            }
        }
        // FMV.{W,D}.X
        Some(InstructionId::FmvWX | InstructionId::FmvDX) => {
            let src = regs.read_x(rs1);
            let bits = match fmt {
                0 => 0xFFFF_FFFF_0000_0000u64 | (src & 0xFFFF_FFFF),
                1 => src,
                _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
            };
            Ok(ExecResult::WriteFp { rd, bits, next_pc })
        }
        _ => Err(VmError::IllegalInstruction(funct5 as u32)),
    }
}

fn exec_fmac(
    instruction_id: Option<InstructionId>,
    fmt: u8,
    rm: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    rs3: usize,
    regs: &ExecuteOperands<'_>,
    csrs: &CsrFile,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let rm = if rm == RM_DYN {
        csrs.rounding_mode()
    } else {
        rm
    };
    let next_pc = pc.wrapping_add(4);

    let (bits, fflags) = match fmt {
        0 => {
            let a = regs.read_f32(rs1);
            let b = regs.read_f32(rs2);
            let c = regs.read_f32(rs3);
            match instruction_id {
                Some(InstructionId::FmaddS) => alu::fp_fmadd_s(a, b, c, rm),
                Some(InstructionId::FmsubS) => alu::fp_fmsub_s(a, b, c, rm),
                Some(InstructionId::FnmsubS) => alu::fp_fnmsub_s(a, b, c, rm),
                Some(InstructionId::FnmaddS) => alu::fp_fnmadd_s(a, b, c, rm),
                _ => return Err(VmError::IllegalInstruction(fmt as u32)),
            }
        }
        1 => {
            let a = regs.read_f64(rs1);
            let b = regs.read_f64(rs2);
            let c = regs.read_f64(rs3);
            match instruction_id {
                Some(InstructionId::FmaddD) => alu::fp_fmadd_d(a, b, c, rm),
                Some(InstructionId::FmsubD) => alu::fp_fmsub_d(a, b, c, rm),
                Some(InstructionId::FnmsubD) => alu::fp_fnmsub_d(a, b, c, rm),
                Some(InstructionId::FnmaddD) => alu::fp_fnmadd_d(a, b, c, rm),
                _ => return Err(VmError::IllegalInstruction(fmt as u32)),
            }
        }
        _ => return Err(VmError::IllegalInstruction(fmt as u32)),
    };

    Ok(ExecResult::WriteFpFlags {
        rd,
        bits,
        fflags,
        next_pc,
    })
}

// --- FP -> integer saturation helpers ---

const NV: u8 = 0x10;
const NX: u8 = 0x01;

fn sat_f32_to_i32(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i32::MAX as i64 as u64, NV);
    }
    let rounded = alu::round_f32(val, rm);
    if rounded >= i32::MAX as f32 {
        return (i32::MAX as i64 as u64, NV);
    }
    if rounded < i32::MIN as f32 {
        return (i32::MIN as i64 as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as i32 as i64 as u64, flags)
}

fn sat_f32_to_u32(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (u32::MAX as u64, NV);
    }
    if val < 0.0f32 {
        return (0, NV);
    }
    let rounded = alu::round_f32(val, rm);
    if rounded >= u32::MAX as f32 {
        return (u32::MAX as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u32 as u64, flags)
}

fn sat_f32_to_i64(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i64::MAX as u64, NV);
    }
    if val >= i64::MAX as f32 {
        return (i64::MAX as u64, NV);
    }
    if val < i64::MIN as f32 {
        return (i64::MIN as u64, NV);
    }
    let rounded = alu::round_f32(val, rm);
    let flags = if val != rounded { NX } else { 0 };
    (rounded as i64 as u64, flags)
}

fn sat_f32_to_u64(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (u64::MAX, NV);
    }
    if val < 0.0f32 {
        return (0, NV);
    }
    // 2^64 as f32
    if val >= 1.844_674_4e19_f32 {
        return (u64::MAX, NV);
    }
    let rounded = alu::round_f32(val, rm);
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u64, flags)
}

// f64->int: round first, then check overflow bounds (fixes the double-rounding issue).

fn sat_f64_to_i32(val: f64, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i32::MAX as i64 as u64, NV);
    }
    let rounded = alu::round_f64(val, rm);
    if rounded >= 2_147_483_648.0f64 || rounded.is_infinite() {
        return (i32::MAX as i64 as u64, NV);
    }
    if rounded < -2_147_483_648.0f64 {
        return (i32::MIN as i64 as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as i32 as i64 as u64, flags)
}

fn sat_f64_to_u32(val: f64, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (u32::MAX as u64, NV);
    }
    if val < 0.0f64 {
        return (0, NV);
    }
    let rounded = alu::round_f64(val, rm);
    if rounded >= 4_294_967_296.0f64 || rounded.is_infinite() {
        return (u32::MAX as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u32 as u64, flags)
}

fn sat_f64_to_i64(val: f64, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i64::MAX as u64, NV);
    }
    let rounded = alu::round_f64(val, rm);
    if rounded >= i64::MAX as f64 || rounded.is_infinite() {
        return (i64::MAX as u64, NV);
    }
    if rounded < i64::MIN as f64 {
        return (i64::MIN as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as i64 as u64, flags)
}

fn sat_f64_to_u64(val: f64, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (u64::MAX, NV);
    }
    if val < 0.0f64 {
        return (0, NV);
    }
    let rounded = alu::round_f64(val, rm);
    // 2^64 as f64
    if rounded >= 1.844_674_407_370_955_2e19f64 || rounded.is_infinite() {
        return (u64::MAX, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u64, flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_integer_operands_do_not_mutate_registers() {
        let mut registers = Registers::new();
        registers.write_x(1, 10);
        registers.write_x(2, 20);
        let operands = ExecuteOperands::new(&registers, (1, 40), (2, 2), (0, 0), (0, 0));
        let csrs = CsrFile::new();
        let insn = DecodedInsn::Alu {
            funct3: 0,
            funct7: 0,
            rd: 3,
            rs1: 1,
            rs2: 2,
        };

        let result = execute(&insn, &operands, &csrs, 0x1000).expect("valid add");

        assert!(matches!(
            result,
            ExecResult::WriteInt {
                rd: 3,
                val: 42,
                next_pc: 0x1004
            }
        ));
        assert_eq!(registers.read_x(1), 10);
        assert_eq!(registers.read_x(2), 20);
    }

    #[test]
    fn forwarded_fp_operands_do_not_mutate_registers() {
        let mut registers = Registers::new();
        registers.write_f64(1, 10.0);
        registers.write_f64(2, 20.0);
        let operands = ExecuteOperands::new(
            &registers,
            (0, 0),
            (0, 0),
            (1, 1.5f64.to_bits()),
            (2, 2.25f64.to_bits()),
        );
        let csrs = CsrFile::new();
        let insn = DecodedInsn::FOp {
            funct5: 0,
            fmt: 1,
            rm: 0,
            rd: 3,
            rs1: 1,
            rs2: 2,
        };

        let result = execute(&insn, &operands, &csrs, 0x2000).expect("valid fadd.d");

        assert!(matches!(
            result,
            ExecResult::WriteFpFlags {
                rd: 3,
                bits,
                next_pc: 0x2004,
                ..
            } if bits == 3.75f64.to_bits()
        ));
        assert_eq!(registers.read_f64(1), 10.0);
        assert_eq!(registers.read_f64(2), 20.0);
    }

    #[test]
    fn x0_stays_zero_when_an_operand_slot_targets_it() {
        let registers = Registers::new();
        let operands =
            ExecuteOperands::new(&registers, (0, u64::MAX), (0, u64::MAX), (0, 0), (0, 0));
        let csrs = CsrFile::new();
        let insn = DecodedInsn::AluImm {
            funct3: 0,
            funct7: 0,
            rd: 1,
            rs1: 0,
            imm: 7,
        };

        let result = execute(&insn, &operands, &csrs, 0).expect("valid addi");

        assert!(matches!(result, ExecResult::WriteInt { val: 7, .. }));
    }
}
