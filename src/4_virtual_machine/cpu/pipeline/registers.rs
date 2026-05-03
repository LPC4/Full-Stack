//! Pipeline inter-stage registers for the 5-stage RV64 pipeline.
//!
//! Each register holds the state produced by one stage and consumed by the next.
//! `None` represents a pipeline bubble (NOP slot).

use crate::virtual_machine::cpu::decoder::DecodedInsn;
use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
use crate::virtual_machine::cpu::pipeline::memory::MemResult;

/// IF -> ID: result of the Fetch stage.
#[derive(Clone, Debug)]
pub struct IFIDReg {
    pub pc: u64,
    pub raw: u32,
    /// Coarse rs1/rs2 extracted from the raw word for load-use hazard detection.
    /// bits[19:15] and bits[24:20] -- valid for all standard formats.
    pub rs1: usize,
    pub rs2: usize,
    /// Branch predictor's decision for this fetch.
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

/// ID -> EX: result of the Decode stage.
#[derive(Clone, Debug)]
pub struct IDEXReg {
    pub pc: u64,
    pub insn: DecodedInsn,
    pub mnemonic: &'static str,
    // Integer source values read from the register file at decode time.
    pub rs1: usize,
    pub rs2: usize,
    pub rs1_val: u64,
    pub rs2_val: u64,
    // FP source values (for FP instructions).
    pub frs1: usize,
    pub frs2: usize,
    pub frs1_val: u64,
    pub frs2_val: u64,
    // Destination register info.
    pub rd: usize,
    pub is_fp_dest: bool,
    pub is_load: bool,
    pub is_fp_load: bool,
    // Propagated prediction.
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

/// EX -> MEM: result of the Execute stage.
#[derive(Clone, Debug)]
pub struct EXMEMReg {
    pub pc: u64,
    pub mnemonic: &'static str,
    pub exec_result: ExecResult,
    /// Destination register (0 = no integer/FP destination).
    pub rd: usize,
    pub is_fp_dest: bool,
    /// Value available for EX/MEM -> EX forwarding (undefined for loads).
    pub fwd_val: u64,
    pub is_load: bool,
    // Branch resolution.
    pub actual_next_pc: u64,
    pub actual_taken: bool,
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

/// MEM -> WB: result of the Memory stage.
#[derive(Clone, Debug)]
pub struct MEMWBReg {
    pub pc: u64,
    pub mnemonic: &'static str,
    pub rd: usize,
    pub is_fp_dest: bool,
    /// Final value to write back and forward.
    pub fwd_val: u64,
    pub mem_result: MemResult,
}
