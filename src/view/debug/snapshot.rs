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
    /// All 32 integer registers.
    pub xregs: [u64; 32],
    /// All 32 FP registers (raw bits, NaN-boxed for f32).
    pub fregs: [u64; 32],
    pub csrs: CsrSnapshot,
    /// PC from the *previous* snapshot, used to highlight changes.
    pub prev_pc: u64,
    /// Integer registers from the previous snapshot.
    pub prev_xregs: [u64; 32],
}

// ---------------------------------------------------------------------------
// Pipeline history
// ---------------------------------------------------------------------------

/// One instruction's snapshot inside the pipeline waterfall.
#[derive(Clone, Debug, Default)]
pub struct PipelineEntry {
    /// Address of the instruction.
    pub pc: u64,
    /// Short mnemonic, e.g. "addi", "lw", "beq".
    pub mnemonic: String,
}

/// Rolling log of recently-fetched instructions used to render the pipeline
/// waterfall diagram.
///
/// Layout convention (index 0 = most recently pushed = currently in IF):
///
/// ```text
///   history[S + R]  →  stage S (0=IF … 4=WB) at row R (0=current cycle)
/// ```
///
/// Because in a perfect in-order pipeline an instruction advances one stage
/// per cycle, the entry that is in IF this cycle (index 0) will be in WB four
/// cycles later (index 4 at row 0 → that entry is now row 0 but shifted).
#[derive(Clone, Debug, Default)]
pub struct PipelineHistory {
    /// Most-recently-fetched instruction first (index 0).
    pub log: Vec<PipelineEntry>,
    /// Monotonically-increasing cycle counter (incremented on every push).
    pub step: u64,
}

impl PipelineHistory {
    pub const DEPTH: usize = 20;

    /// Record that an instruction at `pc` with the given mnemonic just entered IF.
    pub fn push(&mut self, entry: PipelineEntry) {
        self.log.insert(0, entry);
        self.log.truncate(Self::DEPTH);
        self.step += 1;
    }

    /// Entry for stage `stage` (0=IF … 4=WB) at waterfall row `row` (0=current).
    /// Returns `None` if the pipeline has not been filled that far yet.
    pub fn waterfall(&self, stage: usize, row: usize) -> Option<&PipelineEntry> {
        self.log.get(stage + row)
    }

    /// Display cycle number for waterfall row `row`.
    pub fn cycle_for_row(&self, row: usize) -> Option<u64> {
        if self.step > row as u64 { Some(self.step - row as u64) } else { None }
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
    /// Populated from the linker symbol table (text/rodata/data/bss starts).
    pub section_presets: Vec<(&'static str, u64)>,
    /// Cache statistics from L1, L2, and L3 caches
    pub l1_stats: CacheStats,
    pub l2_stats: CacheStats,
    pub l3_stats: CacheStats,
}
