//! Pipeline inter-stage registers for the 5-stage RV64 pipeline.
//!
//! Each register holds the state produced by one stage and consumed by the next.
//! `None` represents a pipeline bubble (NOP slot).

use crate::cpu::decoder::DecodedInsn;
use crate::cpu::pipeline::execute::ExecResult;
use crate::cpu::pipeline::memory::MemResult;

#[derive(Clone, Debug)]
pub struct IFIDReg {
    pub pc: u64,
    pub raw: u32,
    /// Coarse rs1/rs2 for load-use hazard detection, taken from bits[19:15] and
    /// bits[24:20]. Those positions hold for every standard format.
    pub rs1: usize,
    pub rs2: usize,
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

#[derive(Clone, Debug)]
pub struct IDEXReg {
    pub pc: u64,
    pub insn: DecodedInsn,
    pub mnemonic: &'static str,
    pub rs1: usize,
    pub rs2: usize,
    pub rs1_val: u64,
    pub rs2_val: u64,
    pub frs1: usize,
    pub frs2: usize,
    pub frs1_val: u64,
    pub frs2_val: u64,
    pub rd: usize,
    pub is_fp_dest: bool,
    pub is_load: bool,
    pub is_fp_load: bool,
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

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
    pub actual_next_pc: u64,
    pub actual_taken: bool,
    pub predicted_taken: bool,
    pub predicted_target: u64,
}

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
