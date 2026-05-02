//! Plain-data snapshot types that the debug panels render from.
//! No VM references live here — everything is Clone.

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
    /// PC from the *previous* snapshot — used to highlight changes.
    pub prev_pc: u64,
    /// Integer registers from the previous snapshot.
    pub prev_xregs: [u64; 32],
}

// ---------------------------------------------------------------------------
// Pipeline history
// ---------------------------------------------------------------------------

/// Tracks the PCs of the last N instructions so the pipeline diagram can show
/// where each instruction is in the 5-stage pipeline (Fetch→Writeback).
///
/// Because this is a scalar in-order core, after step N:
///   writeback  = history[N-1]
///   memory     = history[N-2]
///   execute    = history[N-3]
///   decode     = history[N-4]
///   fetch      = history[N-5] (current PC, not yet executed)
#[derive(Clone, Debug, Default)]
pub struct PipelineHistory {
    /// Ring buffer of the last `DEPTH` committed PCs (index 0 = most recent).
    history: Vec<u64>,
}

impl PipelineHistory {
    pub const DEPTH: usize = 5;

    /// Record that an instruction at `pc` just entered the pipeline.
    pub fn push(&mut self, pc: u64) {
        self.history.insert(0, pc);
        self.history.truncate(Self::DEPTH);
    }

    /// Returns the PC in the given stage (0 = writeback … 4 = fetch), or None
    /// if fewer than `stage + 1` instructions have been committed.
    pub fn stage(&self, stage: usize) -> Option<u64> {
        self.history.get(stage).copied()
    }

    pub fn stages(&self) -> [Option<u64>; 5] {
        std::array::from_fn(|i| self.history.get(i).copied())
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
