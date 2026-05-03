//! Hazard detection and data forwarding for the 5-stage pipeline.
//!
//! Two kinds of hazards are handled:
//! - **Load-use**: a load in ID/EX whose result is needed by the instruction
//!   being decoded — insert one stall bubble.
//! - **RAW**: a register written in EX/MEM or MEM/WB is read in EX — resolved
//!   through forwarding (no stall).

use crate::virtual_machine::cpu::decoder::DecodedInsn;
use crate::virtual_machine::cpu::pipeline::registers::{EXMEMReg, IDEXReg, MEMWBReg};

// ---------------------------------------------------------------------------
// Register-number extraction helpers
// ---------------------------------------------------------------------------

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

/// True when the destination of a decoded instruction is an FP register.
pub fn insn_is_fp_dest(insn: &DecodedInsn) -> bool {
    matches!(
        insn,
        DecodedInsn::FLoad { .. } | DecodedInsn::FOp { .. } | DecodedInsn::FMac { .. }
    )
}

// ---------------------------------------------------------------------------
// Load-use hazard detection
// ---------------------------------------------------------------------------

/// Returns true if `id_ex` is a load whose result is needed by the instruction
/// currently in the IF/ID register (identified by its raw-word rs1/rs2 fields).
pub fn load_use_hazard(id_ex: &IDEXReg, if_id_rs1: usize, if_id_rs2: usize) -> bool {
    if !(id_ex.is_load || id_ex.is_fp_load) {
        return false;
    }
    if id_ex.rd == 0 {
        return false;
    }
    id_ex.rd == if_id_rs1 || id_ex.rd == if_id_rs2
}

// ---------------------------------------------------------------------------
// Data forwarding
// ---------------------------------------------------------------------------

pub struct ForwardedValues {
    pub rs1: u64,
    pub rs2: u64,
    pub frs1: u64,
    pub frs2: u64,
}

/// Compute the actual rs1/rs2/frs1/frs2 values for the EX stage, applying
/// forwarding from EX/MEM and MEM/WB registers where appropriate.
///
/// Priority: EX/MEM forwarding overrides MEM/WB forwarding (the most recent
/// producer wins).  EX/MEM forwarding is suppressed for load results (whose
/// data is not available until the MEM stage completes); the stall unit ensures
/// no load-use hazard reaches this point for single-cycle latency.
pub fn compute_forwarding(
    ex_mem: Option<&EXMEMReg>,
    mem_wb: Option<&MEMWBReg>,
    id_ex: &IDEXReg,
) -> ForwardedValues {
    let mut rs1 = id_ex.rs1_val;
    let mut rs2 = id_ex.rs2_val;
    let mut frs1 = id_ex.frs1_val;
    let mut frs2 = id_ex.frs2_val;

    // MEM/WB → EX (lower priority)
    if let Some(mw) = mem_wb {
        if mw.rd != 0 {
            if !mw.is_fp_dest {
                if mw.rd == id_ex.rs1 {
                    rs1 = mw.fwd_val;
                }
                if mw.rd == id_ex.rs2 {
                    rs2 = mw.fwd_val;
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
    }

    // EX/MEM → EX (higher priority, load results excluded)
    if let Some(em) = ex_mem {
        if em.rd != 0 && !em.is_load {
            if !em.is_fp_dest {
                if em.rd == id_ex.rs1 {
                    rs1 = em.fwd_val;
                }
                if em.rd == id_ex.rs2 {
                    rs2 = em.fwd_val;
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
    }

    ForwardedValues { rs1, rs2, frs1, frs2 }
}
