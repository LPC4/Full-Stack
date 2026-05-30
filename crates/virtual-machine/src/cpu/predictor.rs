//! Branch predictor -- 2-bit bimodal predictor with a Branch Target Buffer (BTB).
//!
//! The PHT (Pattern History Table) holds 2-bit saturating counters indexed by
//! PC bits.  The BTB maps known branch PCs to their most-recently-seen target.
//! Predict-not-taken is the default when a PC is not yet in the BTB.

use std::collections::HashMap;

const PHT_SIZE: usize = 256;

pub struct BranchPredictor {
    /// 2-bit saturating counters: 0-1 = predict not-taken, 2-3 = predict taken.
    pht: [u8; PHT_SIZE],
    /// Maps branch PC -> last seen target address.
    btb: HashMap<u64, u64>,
    pub stats: PredictorStats,
}

#[derive(Default, Debug, Clone)]
pub struct PredictorStats {
    pub total: u64,
    pub correct: u64,
    pub mispredicted: u64,
}

impl Default for BranchPredictor {
    fn default() -> Self {
        Self {
            pht: [1u8; PHT_SIZE], // start weakly-not-taken
            btb: HashMap::new(),
            stats: PredictorStats::default(),
        }
    }
}

impl BranchPredictor {
    pub fn new() -> Self {
        Self::default()
    }

    fn pht_idx(pc: u64) -> usize {
        ((pc >> 2) as usize) & (PHT_SIZE - 1)
    }

    /// Predict whether the branch at `pc` will be taken and what its target will be.
    /// Returns `(predicted_taken, predicted_target)`.
    pub fn predict(&self, pc: u64) -> (bool, u64) {
        let taken = self.pht[Self::pht_idx(pc)] >= 2;
        let target = self.btb.get(&pc).copied().unwrap_or(pc.wrapping_add(4));
        (taken, target)
    }

    /// Update the predictor with the actual outcome of a branch at `pc`.
    /// Call this once per branch instruction as it exits the EX stage.
    pub fn update(&mut self, pc: u64, was_taken: bool, actual_target: u64, predicted_taken: bool) {
        let idx = Self::pht_idx(pc);

        // Update saturating counter
        if was_taken {
            if self.pht[idx] < 3 {
                self.pht[idx] += 1;
            }
            self.btb.insert(pc, actual_target);
        } else if self.pht[idx] > 0 {
            self.pht[idx] -= 1;
        }

        // Accumulate stats
        self.stats.total += 1;
        if was_taken == predicted_taken {
            self.stats.correct += 1;
        } else {
            self.stats.mispredicted += 1;
        }
    }

    pub fn stats(&self) -> &PredictorStats {
        &self.stats
    }

    /// Clear all branch-history and target state (PHT + BTB), preserving stats.
    /// Used when the address space changes so stale entries from another process
    /// at the same virtual PCs cannot mispredict the resumed process.
    pub fn clear(&mut self) {
        self.pht = [1u8; PHT_SIZE];
        self.btb.clear();
    }
}
