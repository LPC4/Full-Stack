//! Integer and floating-point register file for the RV64 CPU.

/// Canonical NaN-boxed NaN stored in every FP register at reset.
///
/// RV64 NaN-boxing: an f32 value is valid only when bits [63:32] are all 1s.
/// `0xFFFF_FFFF_7FC0_0000` encodes positive quiet NaN in the lower word with
/// the required all-ones upper word.
const CANONICAL_NAN_BOXED: u64 = 0xFFFF_FFFF_7FC0_0000;

/// The upper 32 bits that must all be 1 for a NaN-boxed f32 to be valid.
const NAN_BOX_UPPER: u64 = 0xFFFF_FFFF_0000_0000;

/// RISC-V privilege modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

#[derive(Clone)]
pub struct Registers {
    /// Integer registers x0-x31. x[0] is hardwired to zero.
    x: [u64; 32],
    /// FP registers f0-f31, stored as raw bits with NaN-boxing for f32.
    f: [u64; 32],
    pub pc: u64,
    /// Current privilege mode
    pub priv_mode: PrivilegeMode,
}

impl Registers {
    pub fn new() -> Self {
        Self {
            x: [0u64; 32],
            f: [CANONICAL_NAN_BOXED; 32],
            pc: 0,
            priv_mode: PrivilegeMode::Machine,
        }
    }

    pub fn read_x(&self, reg: usize) -> u64 {
        // x0 is hardwired to zero regardless of what is stored.
        if reg == 0 { 0 } else { self.x[reg] }
    }

    pub fn write_x(&mut self, reg: usize, val: u64) {
        // Writes to x0 are silently dropped.
        if reg != 0 {
            self.x[reg] = val;
        }
    }

    pub fn read_f_bits(&self, reg: usize) -> u64 {
        self.f[reg]
    }

    pub fn write_f_bits(&mut self, reg: usize, val: u64) {
        self.f[reg] = val;
    }

    /// Reads the register as an f32, applying NaN-boxing validation.
    ///
    /// If bits [63:32] are not all 1s the stored value is not a valid
    /// NaN-boxed f32, so the canonical NaN is returned (RISC-V spec §11.3).
    pub fn read_f32(&self, reg: usize) -> f32 {
        let bits = self.f[reg];
        if bits & NAN_BOX_UPPER != NAN_BOX_UPPER {
            return f32::NAN;
        }
        f32::from_bits(bits as u32)
    }

    /// Writes an f32, NaN-boxing it by setting bits [63:32] to all 1s.
    pub fn write_f32(&mut self, reg: usize, val: f32) {
        self.f[reg] = NAN_BOX_UPPER | u64::from(val.to_bits());
    }

    pub fn read_f64(&self, reg: usize) -> f64 {
        f64::from_bits(self.f[reg])
    }

    pub fn write_f64(&mut self, reg: usize, val: f64) {
        self.f[reg] = val.to_bits();
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}
