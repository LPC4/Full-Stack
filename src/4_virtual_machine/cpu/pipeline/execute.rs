//! Execute stage — computes results without touching memory or registers.

use crate::virtual_machine::cpu::alu::{self, RM_DYN};
use crate::virtual_machine::cpu::csr::CsrFile;
use crate::virtual_machine::cpu::decoder::{DecodedInsn, FMacOp};
use crate::virtual_machine::cpu::registers::Registers;
use crate::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// Public result type
// ---------------------------------------------------------------------------

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
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn execute(
    insn: &DecodedInsn,
    regs: &Registers,
    csrs: &CsrFile,
    pc: u64,
) -> Result<ExecResult, VmError> {
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

        DecodedInsn::Branch {
            funct3,
            rs1,
            rs2,
            imm,
        } => {
            let lhs = regs.read_x(*rs1);
            let rhs = regs.read_x(*rs2);
            let taken = match funct3 {
                0 => lhs == rhs,
                1 => lhs != rhs,
                4 => (lhs as i64) < (rhs as i64),
                5 => (lhs as i64) >= (rhs as i64),
                6 => lhs < rhs,
                7 => lhs >= rhs,
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
            funct7,
            rd,
            rs1,
            imm,
        } => exec_alu_imm(*funct3, *funct7, *rd, *rs1, *imm, regs, pc),

        DecodedInsn::AluImm32 {
            funct3,
            funct7,
            rd,
            rs1,
            imm,
        } => exec_alu_imm32(*funct3, *funct7, *rd, *rs1, *imm, regs, pc),

        DecodedInsn::Alu {
            funct3,
            funct7,
            rd,
            rs1,
            rs2,
        } => exec_alu(*funct3, *funct7, *rd, *rs1, *rs2, regs, pc),

        DecodedInsn::Alu32 {
            funct3,
            funct7,
            rd,
            rs1,
            rs2,
        } => exec_alu32(*funct3, *funct7, *rd, *rs1, *rs2, regs, pc),

        DecodedInsn::Fence { .. } => Ok(ExecResult::Fence {
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::FenceI => Ok(ExecResult::FenceI {
            next_pc: pc.wrapping_add(4),
        }),

        DecodedInsn::Ecall => Ok(ExecResult::Ecall),

        DecodedInsn::Ebreak => Ok(ExecResult::Ebreak),

        DecodedInsn::Csr {
            funct3,
            rd,
            rs1_uimm,
            csr,
        } => {
            let operand = match funct3 {
                1 | 2 | 3 => regs.read_x(*rs1_uimm),
                5 | 6 | 7 => *rs1_uimm as u64,
                _ => return Err(VmError::IllegalInstruction(*funct3 as u32)),
            };
            Ok(ExecResult::Csr {
                funct3: *funct3,
                rd: *rd,
                rs1_uimm: *rs1_uimm,
                csr: *csr,
                operand,
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
        } => exec_fp_op(*funct5, *fmt, *rm, *rd, *rs1, *rs2, regs, csrs, pc),

        DecodedInsn::FMac {
            op,
            fmt,
            rm,
            rd,
            rs1,
            rs2,
            rs3,
        } => exec_fmac(op, *fmt, *rm, *rd, *rs1, *rs2, *rs3, regs, csrs, pc),

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

// ---------------------------------------------------------------------------
// ALU helpers
// ---------------------------------------------------------------------------

fn exec_alu_imm(
    funct3: u8,
    funct7: u8,
    rd: usize,
    rs1: usize,
    imm: i64,
    regs: &Registers,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let val = regs.read_x(rs1);
    let imm_u = imm as u64;
    let result = match funct3 {
        0 => alu::add(val, imm_u),
        1 => alu::sll(val, (imm & 63) as u64),
        2 => alu::slt(val, imm_u),
        3 => alu::sltu(val, imm_u),
        4 => alu::xor(val, imm_u),
        5 => {
            let shamt = (imm & 63) as u64;
            if funct7 & 0x20 != 0 {
                alu::sra(val, shamt)
            } else {
                alu::srl(val, shamt)
            }
        }
        6 => alu::or(val, imm_u),
        7 => alu::and(val, imm_u),
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu_imm32(
    funct3: u8,
    funct7: u8,
    rd: usize,
    rs1: usize,
    imm: i64,
    regs: &Registers,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let val = regs.read_x(rs1);
    let result = match funct3 {
        0 => alu::addw(val, imm as u64),
        1 => alu::sllw(val, (imm & 31) as u64),
        5 => {
            let shamt = (imm & 31) as u64;
            if funct7 & 0x20 != 0 {
                alu::sraw(val, shamt)
            } else {
                alu::srlw(val, shamt)
            }
        }
        _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu(
    funct3: u8,
    funct7: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &Registers,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let a = regs.read_x(rs1);
    let b = regs.read_x(rs2);
    let result = match funct7 {
        0x00 => match funct3 {
            0 => alu::add(a, b),
            1 => alu::sll(a, b),
            2 => alu::slt(a, b),
            3 => alu::sltu(a, b),
            4 => alu::xor(a, b),
            5 => alu::srl(a, b),
            6 => alu::or(a, b),
            7 => alu::and(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        0x20 => match funct3 {
            0 => alu::sub(a, b),
            5 => alu::sra(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        0x01 => match funct3 {
            0 => alu::mul(a, b),
            1 => alu::mulh(a, b),
            2 => alu::mulhsu(a, b),
            3 => alu::mulhu(a, b),
            4 => alu::div(a, b),
            5 => alu::divu(a, b),
            6 => alu::rem(a, b),
            7 => alu::remu(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        _ => return Err(VmError::IllegalInstruction(funct7 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

fn exec_alu32(
    funct3: u8,
    funct7: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &Registers,
    pc: u64,
) -> Result<ExecResult, VmError> {
    let a = regs.read_x(rs1);
    let b = regs.read_x(rs2);
    let result = match funct7 {
        0x00 => match funct3 {
            0 => alu::addw(a, b),
            1 => alu::sllw(a, b),
            5 => alu::srlw(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        0x20 => match funct3 {
            0 => alu::subw(a, b),
            5 => alu::sraw(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        0x01 => match funct3 {
            0 => alu::mulw(a, b),
            4 => alu::divw(a, b),
            5 => alu::divuw(a, b),
            6 => alu::remw(a, b),
            7 => alu::remuw(a, b),
            _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
        },
        _ => return Err(VmError::IllegalInstruction(funct7 as u32)),
    };
    Ok(ExecResult::WriteInt {
        rd,
        val: result,
        next_pc: pc.wrapping_add(4),
    })
}

// ---------------------------------------------------------------------------
// FP helpers
// ---------------------------------------------------------------------------

fn exec_fp_op(
    funct5: u8,
    fmt: u8,
    rm: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    regs: &Registers,
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

    match funct5 {
        // FADD
        0b00000 => {
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
        0b00001 => {
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
        0b00010 => {
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
        0b00011 => {
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
        0b01011 => {
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
        0b00100 => {
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
        0b00101 => {
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
        0b10100 => {
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
        // FCVT.{W,WU,L,LU}.{S,D}  — FP → int
        0b11000 => {
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
        // FCVT.{S,D}.{W,WU,L,LU}  — int → FP
        0b11010 => {
            let src = regs.read_x(rs1);
            let (bits, fflags) = match fmt {
                0 => {
                    // int → f32: use f64 as exact intermediate, then apply rm.
                    let exact_f64: f64 = match rs2 {
                        0 => (src as i32) as f64, // i32 → f64 is exact
                        1 => (src as u32) as f64, // u32 → f64 is exact
                        2 => (src as i64) as f64, // i64 → f64: exact for |v| ≤ 2^53
                        3 => src as f64,          // u64 → f64: exact for v ≤ 2^53
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
                    // Detect NX for large integer → f64 conversions that lose bits.
                    let fflags: u8 = match rs2 {
                        2 => {
                            if (result as i64) != (src as i64) {
                                0x01
                            } else {
                                0
                            }
                        }
                        3 => {
                            if (result as u64) != src {
                                0x01
                            } else {
                                0
                            }
                        }
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
        // FCVT.S.D (fmt=0, rs2=1) — convert f64 → f32 with rounding + flags
        0b01000 => {
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
        // FCVT.D.S (fmt=1, rs2=0) — convert f32 → f64; exact, no flags
        0b01001 => {
            if fmt != 1 {
                return Err(VmError::IllegalInstruction(funct5 as u32));
            }
            let val_s = regs.read_f32(rs1);
            let val_d = val_s as f64;
            // f32 → f64 is always exact; no exception flags (incl. for NaN).
            Ok(ExecResult::WriteFpFlags {
                rd,
                bits: val_d.to_bits(),
                fflags: 0,
                next_pc,
            })
        }
        // FMV.X.W/D and FCLASS
        0b11100 => {
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
        0b11110 => {
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
    op: &FMacOp,
    fmt: u8,
    rm: u8,
    rd: usize,
    rs1: usize,
    rs2: usize,
    rs3: usize,
    regs: &Registers,
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
            match op {
                FMacOp::Fmadd => alu::fp_fmadd_s(a, b, c, rm),
                FMacOp::Fmsub => alu::fp_fmsub_s(a, b, c, rm),
                FMacOp::Fnmsub => alu::fp_fnmsub_s(a, b, c, rm),
                FMacOp::Fnmadd => alu::fp_fnmadd_s(a, b, c, rm),
            }
        }
        1 => {
            let a = regs.read_f64(rs1);
            let b = regs.read_f64(rs2);
            let c = regs.read_f64(rs3);
            match op {
                FMacOp::Fmadd => alu::fp_fmadd_d(a, b, c, rm),
                FMacOp::Fmsub => alu::fp_fmsub_d(a, b, c, rm),
                FMacOp::Fnmsub => alu::fp_fnmsub_d(a, b, c, rm),
                FMacOp::Fnmadd => alu::fp_fnmadd_d(a, b, c, rm),
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

// ---------------------------------------------------------------------------
// FP → integer saturation helpers
// ---------------------------------------------------------------------------

const NV: u8 = 0x10;
const NX: u8 = 0x01;

fn sat_f32_to_i32(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i32::MAX as i64 as u64, NV);
    }
    let rounded = alu::round_f32(val, rm);
    if rounded >= 2_147_483_648.0f32 {
        return (i32::MAX as i64 as u64, NV);
    }
    if rounded < -2_147_483_648.0f32 {
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
    if rounded >= 4_294_967_296.0f32 {
        return (u32::MAX as u64, NV);
    }
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u32 as u64, flags)
}

fn sat_f32_to_i64(val: f32, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        return (i64::MAX as u64, NV);
    }
    if val >= 9_223_372_036_854_775_808.0f32 {
        return (i64::MAX as u64, NV);
    }
    if val < -9_223_372_036_854_775_808.0f32 {
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
    if val >= 1.844_674_407_370_955e19f32 {
        return (u64::MAX, NV);
    }
    let rounded = alu::round_f32(val, rm);
    let flags = if val != rounded { NX } else { 0 };
    (rounded as u64, flags)
}

// f64→int: round first, then check overflow bounds (fixes the double-rounding issue).

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
    if rounded >= 9_223_372_036_854_775_808.0f64 || rounded.is_infinite() {
        return (i64::MAX as u64, NV);
    }
    if rounded < -9_223_372_036_854_775_808.0f64 {
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
