//! Plain-data snapshot types that the debug panels render from.
//! No VM references live here, everything is Clone.

use crate::virtual_machine::cpu::csr::CsrSnapshot;
use crate::virtual_machine::memory::cache::CacheStats;

// ---------------------------------------------------------------------------
// CPU snapshot
// ---------------------------------------------------------------------------

/// All CPU register state captured after a step.
#[derive(Clone, Debug, Default)]
pub struct CpuSnapshot {
    pub pc: u64,
    pub xregs: [u64; 32],
    pub fregs: [u64; 32],
    pub csrs: CsrSnapshot,
    pub prev_pc: u64,
    pub prev_xregs: [u64; 32],
}

// ---------------------------------------------------------------------------
// Pipeline history
// ---------------------------------------------------------------------------

/// An instruction occupying one pipeline stage.
#[derive(Clone, Debug)]
pub struct PipelineEntry {
    pub pc: u64,
    pub mnemonic: String,
}

/// What a single stage slot holds in a given cycle.
#[derive(Clone, Debug)]
pub enum SlotState {
    /// Pipeline has not filled this stage yet (early cycles).
    Empty,
    /// A real instruction is in this stage.
    Normal(PipelineEntry),
    /// A bubble injected because of a load-use stall.
    StallBubble,
    /// A bubble injected because of a branch mispredict flush.
    FlushBubble,
}

impl Default for SlotState {
    fn default() -> Self { Self::Empty }
}

/// Full 5-stage pipeline state captured for one clock cycle.
#[derive(Clone, Debug)]
pub struct PipelineCycleSnapshot {
    pub cycle: u64,
    /// `stages[0]` = IF, `stages[1]` = ID, `stages[2]` = EX,
    /// `stages[3]` = MEM, `stages[4]` = WB.
    pub stages: [SlotState; 5],
    /// A load-use stall occurred this cycle (IF held, bubble in ID).
    pub stalled: bool,
    /// A branch mispredict flush occurred (IF and ID squashed).
    pub flushed: bool,
}

/// Rolling history of pipeline cycle snapshots used to render the waterfall.
///
/// `cycles[0]` = most recent cycle, `cycles[1]` = one cycle ago, ...
#[derive(Clone, Debug, Default)]
pub struct PipelineHistory {
    pub cycles: Vec<PipelineCycleSnapshot>,
    pub total_cycles: u64,
    pub stall_cycles: u64,
    pub flush_cycles: u64,
    pub branches_seen: u64,
    pub branches_mispredicted: u64,
}

impl PipelineHistory {
    pub const DEPTH: usize = 20;

    /// Push a new cycle snapshot (most recent first).
    pub fn push(&mut self, snap: PipelineCycleSnapshot) {
        self.cycles.insert(0, snap);
        self.cycles.truncate(Self::DEPTH);
    }

    /// Entry for stage `stage` (0=IF ... 4=WB) at waterfall row `row` (0=latest).
    pub fn slot(&self, stage: usize, row: usize) -> Option<&SlotState> {
        self.cycles.get(row).map(|c| &c.stages[stage])
    }

    /// Display cycle number for waterfall row `row`.
    pub fn cycle_for_row(&self, row: usize) -> Option<u64> {
        self.cycles.get(row).map(|c| c.cycle)
    }
}

// ---------------------------------------------------------------------------
// Top-level snapshot
// ---------------------------------------------------------------------------

/// Everything the debug panels need, captured after each step.
#[derive(Clone, Debug, Default)]
pub struct DebugSnapshot {
    pub cpu: CpuSnapshot,
    pub pipeline: PipelineHistory,
    /// `(label, address)` pairs for the memory view's preset jump buttons.
    pub section_presets: Vec<(&'static str, u64)>,
    pub l1_stats: CacheStats,
    pub l2_stats: CacheStats,
    pub l3_stats: CacheStats,
}
