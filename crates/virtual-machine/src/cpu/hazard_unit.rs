//! Hazard handling for the 5-stage pipeline: a load-use hazard stalls one
//! bubble; RAW hazards resolve by forwarding from EX/MEM and MEM/WB.

use crate::cpu::decoder::DecodedInsn;
use crate::cpu::pipeline::registers::{EXMEMReg, IDEXReg, MEMWBReg};

// --- Register-number extraction helpers ---

/// Destination register (integer or FP) of a decoded instruction; 0 = none.
pub fn insn_rd(insn: &DecodedInsn) -> usize {
    match insn {
        DecodedInsn::Lui { rd, .. }
        | DecodedInsn::Auipc { rd, .. }
        | DecodedInsn::Jal { rd, .. }
        | DecodedInsn::Jalr { rd, .. }
        | DecodedInsn::Load { rd, .. }
        | DecodedInsn::AluImm { rd, .. }
        | DecodedInsn::AluImm32 { rd, .. }
        | DecodedInsn::Alu { rd, .. }
        | DecodedInsn::Alu32 { rd, .. }
        | DecodedInsn::Csr { rd, .. }
        | DecodedInsn::FLoad { rd, .. }
        | DecodedInsn::FOp { rd, .. }
        | DecodedInsn::FMac { rd, .. }
        | DecodedInsn::Atomic { rd, .. } => *rd,
        _ => 0,
    }
}

/// First integer source register of a decoded instruction; 0 = none.
pub fn insn_rs1(insn: &DecodedInsn) -> usize {
    match insn {
        DecodedInsn::Jalr { rs1, .. }
        | DecodedInsn::Branch { rs1, .. }
        | DecodedInsn::Load { rs1, .. }
        | DecodedInsn::Store { rs1, .. }
        | DecodedInsn::AluImm { rs1, .. }
        | DecodedInsn::AluImm32 { rs1, .. }
        | DecodedInsn::Alu { rs1, .. }
        | DecodedInsn::Alu32 { rs1, .. }
        | DecodedInsn::FLoad { rs1, .. }
        | DecodedInsn::FStore { rs1, .. }
        | DecodedInsn::FOp { rs1, .. }
        | DecodedInsn::FMac { rs1, .. }
        | DecodedInsn::Atomic { rs1, .. } => *rs1,
        // CSR immediate variants (funct3 5-7) carry a literal here; execute ignores the
        // bogus forward and save/restore undoes it, so returning unconditionally is safe.
        DecodedInsn::Csr { rs1_uimm, .. } => *rs1_uimm,
        _ => 0,
    }
}

/// Second integer source register of a decoded instruction; 0 = none.
pub fn insn_rs2(insn: &DecodedInsn) -> usize {
    match insn {
        DecodedInsn::Branch { rs2, .. }
        | DecodedInsn::Store { rs2, .. }
        | DecodedInsn::Alu { rs2, .. }
        | DecodedInsn::Alu32 { rs2, .. }
        | DecodedInsn::FStore { rs2, .. }
        | DecodedInsn::FOp { rs2, .. }
        | DecodedInsn::FMac { rs2, .. }
        | DecodedInsn::Atomic { rs2, .. } => *rs2,
        _ => 0,
    }
}

/// True when the instruction is a (potentially FP) memory load.
pub fn insn_is_load(insn: &DecodedInsn) -> bool {
    matches!(insn, DecodedInsn::Load { .. } | DecodedInsn::FLoad { .. })
}

/// True when the instruction is an atomic (acts as load for hazard purposes).
pub fn insn_is_atomic(insn: &DecodedInsn) -> bool {
    matches!(insn, DecodedInsn::Atomic { .. })
}

/// True when the destination is an FP register. FP-to-int converts, compares, and
/// `fmv.x`/`fclass` (funct5 below) write integer rd; FP-forwarding those starves the consumer.
pub fn insn_is_fp_dest(insn: &DecodedInsn) -> bool {
    match insn {
        DecodedInsn::FLoad { .. } | DecodedInsn::FMac { .. } => true,
        DecodedInsn::FOp { funct5, .. } => !matches!(*funct5, 0b11000 | 0b10100 | 0b11100),
        _ => false,
    }
}

// --- Load-use hazard detection ---

/// Returns true if `id_ex` is a load whose result is needed by the instruction
/// currently in the IF/ID register (identified by its raw-word rs1/rs2 fields).
pub fn load_use_hazard(id_ex: &IDEXReg, if_id_rs1: usize, if_id_rs2: usize) -> bool {
    if !(id_ex.is_load || id_ex.is_fp_load) {
        return false;
    }
    // x0 has no real result to wait on, but f0 is a real FP register.
    if id_ex.rd == 0 && !id_ex.is_fp_load {
        return false;
    }
    id_ex.rd == if_id_rs1 || id_ex.rd == if_id_rs2
}

// --- Data forwarding ---

pub struct ForwardedValues {
    pub rs1: u64,
    pub rs2: u64,
    pub frs1: u64,
    pub frs2: u64,
}

/// The rs1/rs2/frs1/frs2 values for EX with forwarding applied: EX/MEM (newest producer)
/// beats MEM/WB, and load results skip EX/MEM since their data only exists after MEM.
pub fn compute_forwarding(
    ex_mem: Option<&EXMEMReg>,
    mem_wb: Option<&MEMWBReg>,
    id_ex: &IDEXReg,
) -> ForwardedValues {
    let mut rs1 = id_ex.rs1_val;
    let mut rs2 = id_ex.rs2_val;
    let mut frs1 = id_ex.frs1_val;
    let mut frs2 = id_ex.frs2_val;

    // MEM/WB -> EX (lower priority). Only integer dests skip rd == 0: x0 is
    // hardwired zero, but f0 is a real register whose results must forward.
    if let Some(mw) = mem_wb {
        if !mw.is_fp_dest {
            if mw.rd != 0 {
                if mw.rd == id_ex.rs1 {
                    rs1 = mw.fwd_val;
                }
                if mw.rd == id_ex.rs2 {
                    rs2 = mw.fwd_val;
                }
            }
        } else {
            if mw.rd == id_ex.frs1 {
                frs1 = mw.fwd_val;
            }
            if mw.rd == id_ex.frs2 {
                frs2 = mw.fwd_val;
            }
        }
    }

    // EX/MEM -> EX (higher priority, load results excluded).
    if let Some(em) = ex_mem
        && !em.is_load
    {
        if !em.is_fp_dest {
            if em.rd != 0 {
                if em.rd == id_ex.rs1 {
                    rs1 = em.fwd_val;
                }
                if em.rd == id_ex.rs2 {
                    rs2 = em.fwd_val;
                }
            }
        } else {
            if em.rd == id_ex.frs1 {
                frs1 = em.fwd_val;
            }
            if em.rd == id_ex.frs2 {
                frs2 = em.fwd_val;
            }
        }
    }

    ForwardedValues {
        rs1,
        rs2,
        frs1,
        frs2,
    }
}
