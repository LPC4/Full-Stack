pub mod alu;
pub mod csr;
pub mod decoder;
pub mod hazard_unit;
pub mod mmu;
pub mod pipeline;
pub mod predictor;
pub mod registers;
pub mod traps;

pub use pipeline::{CpuPipelineFeed, Pipeline, PipelineStats, StageEntry, TickOutcome};
pub use registers::PrivilegeMode;

use crate::bus::SystemBus;
use crate::error::VmError;

#[derive(Debug)]
pub enum StepOutcome {
    Continue,
    Halted(i64),
}

/// The CPU: owns a 5-stage pipeline and exposes a single tick interface.
pub struct Cpu {
    pipeline: Pipeline,
}

impl Cpu {
    pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
        Self {
            pipeline: Pipeline::new(start_pc, stack_ptr),
        }
    }

    pub fn tick(&mut self, bus: &mut SystemBus) -> Result<TickOutcome, VmError> {
        self.pipeline.tick(bus)
    }

    pub fn last_cycle(&self) -> &CpuPipelineFeed {
        &self.pipeline.last_cycle
    }

    pub fn stats(&self) -> &PipelineStats {
        &self.pipeline.stats
    }

    pub fn set_return_addr(&mut self, ra: u64) {
        self.pipeline.set_return_addr(ra);
    }


    pub fn write_csr_mtvec(&mut self, val: u64) {
        self.pipeline.write_csr_mtvec(val);
    }

    pub fn peek_reg(&self, r: usize) -> u64 {
        self.pipeline.peek_reg(r)
    }

    pub fn peek_fp_reg(&self, r: usize) -> u64 {
        self.pipeline.peek_fp_reg(r)
    }

    pub fn peek_pc(&self) -> u64 {
        self.pipeline.peek_pc()
    }

    pub fn peek_csr_mcause(&self) -> u64 {
        self.pipeline.peek_csr_mcause()
    }

    pub fn peek_csr_mtvec(&self) -> u64 {
        self.pipeline.peek_csr_mtvec()
    }

    pub fn peek_all_xregs(&self) -> [u64; 32] {
        self.pipeline.peek_all_xregs()
    }

    pub fn peek_all_fregs(&self) -> [u64; 32] {
        self.pipeline.peek_all_fregs()
    }

    pub fn peek_csrs(&self) -> csr::CsrSnapshot {
        self.pipeline.peek_csrs()
    }

    pub fn predictor_stats(&self) -> &predictor::PredictorStats {
        self.pipeline.predictor_stats()
    }
}
