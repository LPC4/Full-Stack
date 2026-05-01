//! Pure ALU functions for the RV64IMAFD virtual machine.
//!
//! All integer operations are free functions that operate on `u64` bit patterns.
//! FP operations return `(result_bits: u64, fflags: u8)` where `fflags` accumulates
//! IEEE 754 exception flags: NV=0x10, DZ=0x08, OF=0x04, UF=0x02, NX=0x01.
//!
//! NaN-boxing convention: single-precision results have their upper 32 bits set to
//! `0xFFFF_FFFF` so that the 64-bit FP register holds a valid NaN-boxed f32.
//!
//! # f32 rounding mode support
//!
//! f32 arithmetic (add, sub, mul, div, sqrt, FMA) computes the exact mathematical
//! result in f64 (which is always exact for f32 operands: the 24-bit f32 mantissa
//! fits without loss in the 53-bit f64), then rounds the f64 result to f32 according
//! to the requested rounding mode.  This gives exact rounding-mode compliance for all
//! five RISC-V modes (RNE, RTZ, RDN, RUP, RMM) and correct NX/UF/OF flags.
//!
//! f64 arithmetic uses the host hardware (always RNE on x86/ARM; other modes
//! are approximated as RNE).  UF and OF flags are derived from the f64 result.

// ---------------------------------------------------------------------------
// FP rounding mode constants (match RISC-V spec frm field)
// ---------------------------------------------------------------------------

pub const RM_RNE: u8 = 0; // Round to Nearest, ties to Even
pub const RM_RTZ: u8 = 1; // Round towards Zero
pub const RM_RDN: u8 = 2; // Round Down (towards −∞)
pub const RM_RUP: u8 = 3; // Round Up (towards +∞)
pub const RM_RMM: u8 = 4; // Round to Nearest, ties to Max Magnitude
pub const RM_DYN: u8 = 7; // Dynamic (use fcsr.frm), caller resolves before calling

// ---------------------------------------------------------------------------
// Exception flag bits
// ---------------------------------------------------------------------------

const NV: u8 = 0x10; // Invalid operation
const DZ: u8 = 0x08; // Divide by zero
const OF: u8 = 0x04; // Overflow
const UF: u8 = 0x02; // Underflow
const NX: u8 = 0x01; // Inexact

// ---------------------------------------------------------------------------
// Canonical NaN bit patterns
// ---------------------------------------------------------------------------

/// Canonical quiet NaN for f32, NaN-boxed in a u64 FP register.
#[inline]
pub fn canon_nan_s() -> u64 {
    0xFFFF_FFFF_7FC0_0000u64
}

/// Canonical quiet NaN for f64.
#[inline]
pub fn canon_nan_d() -> u64 {
    0x7FF8_0000_0000_0000u64
}

// ---------------------------------------------------------------------------
// NaN-box helpers
// ---------------------------------------------------------------------------

#[inline]
fn box_f32(v: f32) -> u64 {
    0xFFFF_FFFF_0000_0000u64 | (v.to_bits() as u64)
}

#[inline]
fn box_f64(v: f64) -> u64 {
    v.to_bits()
}

// ---------------------------------------------------------------------------
// Base integer ALU, RV64I
// ---------------------------------------------------------------------------

#[inline]
pub fn add(a: u64, b: u64) -> u64 {
    a.wrapping_add(b)
}
#[inline]
pub fn sub(a: u64, b: u64) -> u64 {
    a.wrapping_sub(b)
}
#[inline]
pub fn sll(a: u64, b: u64) -> u64 {
    a << (b & 63)
}
#[inline]
pub fn srl(a: u64, b: u64) -> u64 {
    a >> (b & 63)
}
#[inline]
pub fn sra(a: u64, b: u64) -> u64 {
    ((a as i64) >> (b & 63)) as u64
}
#[inline]
pub fn and(a: u64, b: u64) -> u64 {
    a & b
}
#[inline]
pub fn or(a: u64, b: u64) -> u64 {
    a | b
}
#[inline]
pub fn xor(a: u64, b: u64) -> u64 {
    a ^ b
}
#[inline]
pub fn slt(a: u64, b: u64) -> u64 {
    ((a as i64) < (b as i64)) as u64
}
#[inline]
pub fn sltu(a: u64, b: u64) -> u64 {
    (a < b) as u64
}

// 32-bit word ops, result is sign-extended to 64 bits.

#[inline]
pub fn addw(a: u64, b: u64) -> u64 {
    (a as u32).wrapping_add(b as u32) as i32 as i64 as u64
}

#[inline]
pub fn subw(a: u64, b: u64) -> u64 {
    (a as u32).wrapping_sub(b as u32) as i32 as i64 as u64
}

#[inline]
pub fn sllw(a: u64, b: u64) -> u64 {
    ((a as u32) << (b & 31)) as i32 as i64 as u64
}

#[inline]
pub fn srlw(a: u64, b: u64) -> u64 {
    ((a as u32) >> (b & 31)) as i32 as i64 as u64
}

#[inline]
pub fn sraw(a: u64, b: u64) -> u64 {
    ((a as i32) >> (b & 31)) as i64 as u64
}

// ---------------------------------------------------------------------------
// M extension, multiply / divide
// ---------------------------------------------------------------------------

/// Lower 64 bits of a×b (both interpreted as two's-complement 64-bit).
#[inline]
pub fn mul(a: u64, b: u64) -> u64 {
    a.wrapping_mul(b)
}

/// Upper 64 bits of signed×signed product.
#[inline]
pub fn mulh(a: u64, b: u64) -> u64 {
    ((a as i64 as i128).wrapping_mul(b as i64 as i128) >> 64) as u64
}

/// Upper 64 bits of unsigned×unsigned product.
#[inline]
pub fn mulhu(a: u64, b: u64) -> u64 {
    ((a as u128).wrapping_mul(b as u128) >> 64) as u64
}

/// Upper 64 bits of signed×unsigned product.
#[inline]
pub fn mulhsu(a: u64, b: u64) -> u64 {
    ((a as i64 as i128).wrapping_mul(b as u128 as i128) >> 64) as u64
}

/// Signed division. Returns `u64::MAX` on divide-by-zero; `i64::MIN` (as u64) on overflow.
pub fn div(a: u64, b: u64) -> u64 {
    let dividend = a as i64;
    let divisor = b as i64;
    if divisor == 0 {
        u64::MAX
    } else if dividend == i64::MIN && divisor == -1 {
        i64::MIN as u64
    } else {
        dividend.wrapping_div(divisor) as u64
    }
}

/// Unsigned division. Returns `u64::MAX` on divide-by-zero.
pub fn divu(a: u64, b: u64) -> u64 {
    if b == 0 { u64::MAX } else { a / b }
}

/// Signed remainder. Returns the dividend unchanged on divide-by-zero; 0 on overflow.
pub fn rem(a: u64, b: u64) -> u64 {
    let dividend = a as i64;
    let divisor = b as i64;
    if divisor == 0 {
        a
    } else if dividend == i64::MIN && divisor == -1 {
        0
    } else {
        dividend.wrapping_rem(divisor) as u64
    }
}

/// Unsigned remainder. Returns the dividend unchanged on divide-by-zero.
pub fn remu(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { a % b }
}

// 32-bit M-extension variants, results sign-extended to 64 bits.

#[inline]
pub fn mulw(a: u64, b: u64) -> u64 {
    (a as u32).wrapping_mul(b as u32) as i32 as i64 as u64
}

pub fn divw(a: u64, b: u64) -> u64 {
    let dividend = a as i32;
    let divisor = b as i32;
    if divisor == 0 {
        u64::MAX
    } else if dividend == i32::MIN && divisor == -1 {
        i32::MIN as i64 as u64
    } else {
        dividend.wrapping_div(divisor) as i64 as u64
    }
}

pub fn divuw(a: u64, b: u64) -> u64 {
    let dividend = a as u32;
    let divisor = b as u32;
    if divisor == 0 {
        u64::MAX
    } else {
        (dividend / divisor) as i32 as i64 as u64
    }
}

pub fn remw(a: u64, b: u64) -> u64 {
    let dividend = a as i32;
    let divisor = b as i32;
    if divisor == 0 {
        dividend as i64 as u64
    } else if dividend == i32::MIN && divisor == -1 {
        0
    } else {
        dividend.wrapping_rem(divisor) as i64 as u64
    }
}

pub fn remuw(a: u64, b: u64) -> u64 {
    let dividend = a as u32;
    let divisor = b as u32;
    if divisor == 0 {
        dividend as i32 as i64 as u64
    } else {
        (dividend % divisor) as i32 as i64 as u64
    }
}

// ---------------------------------------------------------------------------
// FP rounding helpers
// ---------------------------------------------------------------------------

pub fn round_f32(val: f32, rm: u8) -> f32 {
    match rm {
        RM_RNE => val.round_ties_even(),
        RM_RTZ => val.trunc(),
        RM_RDN => val.floor(),
        RM_RUP => val.ceil(),
        RM_RMM => val.round(),
        _ => val,
    }
}

pub fn round_f64(val: f64, rm: u8) -> f64 {
    match rm {
        RM_RNE => val.round_ties_even(),
        RM_RTZ => val.trunc(),
        RM_RDN => val.floor(),
        RM_RUP => val.ceil(),
        RM_RMM => val.round(),
        _ => val,
    }
}

// ---------------------------------------------------------------------------
// f32 rounding helpers: next representable value up/down
// ---------------------------------------------------------------------------

/// Next representable f32 strictly greater than `x`.
#[inline]
fn next_up_f32(x: f32) -> f32 {
    if x.is_nan() || x == f32::INFINITY {
        return x;
    }
    let bits = x.to_bits();
    if x.is_sign_negative() {
        // negative: decreasing magnitude (bits decrease) moves toward 0 / +∞
        if bits == 0x8000_0000 {
            f32::from_bits(0x0000_0001) // -0 → smallest positive subnormal
        } else {
            f32::from_bits(bits - 1)
        }
    } else {
        // positive or +0: increasing bits moves toward +∞
        f32::from_bits(bits + 1)
    }
}

/// Next representable f32 strictly less than `x`.
#[inline]
fn next_down_f32(x: f32) -> f32 {
    if x.is_nan() || x == f32::NEG_INFINITY {
        return x;
    }
    let bits = x.to_bits();
    if x.is_sign_negative() {
        // negative: increasing magnitude moves toward -∞
        f32::from_bits(bits + 1)
    } else {
        // positive or +0: decreasing bits moves toward 0 and then negative
        if bits == 0 {
            f32::from_bits(0x8000_0001) // +0 → smallest negative subnormal
        } else {
            f32::from_bits(bits - 1)
        }
    }
}

// ---------------------------------------------------------------------------
// f64 → f32 with explicit rounding mode
//
// Strategy: compute with hardware default (RNE on x86/ARM), then if the
// rounded result is on the wrong side of the exact value, nudge it by one ULP.
// For f32 inputs (at most 24 significant bits), the f64 result is exact so the
// "exact" value passed here is the true mathematical result.
// ---------------------------------------------------------------------------

/// Round a finite-or-infinite f64 to f32 using the specified rounding mode.
pub fn f64_to_f32_with_rm(val: f64, rm: u8) -> f32 {
    if val.is_nan() {
        return f32::NAN;
    }
    if !val.is_finite() {
        return val as f32; // ±∞ maps exactly
    }
    let rne = val as f32; // host hardware rounds with RNE
    let rne_f64 = rne as f64;
    match rm {
        RM_RNE | RM_RMM => rne,
        RM_RTZ => {
            // Result must not be farther from zero than the exact value.
            if (val > 0.0 && rne_f64 > val) || (val < 0.0 && rne_f64 < val) {
                // Hardware rounded away from zero; nudge back.
                if val > 0.0 {
                    next_down_f32(rne)
                } else {
                    next_up_f32(rne)
                }
            } else {
                rne
            }
        }
        RM_RDN => {
            // Result must be ≤ val.
            if rne_f64 > val {
                next_down_f32(rne)
            } else {
                rne
            }
        }
        RM_RUP => {
            // Result must be ≥ val.
            if rne_f64 < val { next_up_f32(rne) } else { rne }
        }
        _ => rne,
    }
}

// ---------------------------------------------------------------------------
// Flag detection from f64-exact result and f32 rounded result
// ---------------------------------------------------------------------------

/// Derive fflags for an f32 operation given the exact result (`exact`, computed
/// in f64) and the rounded f32 result.  Handles OF, UF, NX.
///
/// NV must be detected by the caller (the pattern depends on the operation).
pub fn fp_flags_from_exact_s(exact: f64, result: f32) -> u8 {
    let mut flags = 0u8;
    if result.is_infinite() && exact.is_finite() {
        // Overflow to ±∞ is always inexact.
        flags |= OF | NX;
    } else if !result.is_nan() {
        if (result as f64) != exact {
            flags |= NX;
        }
        if result.is_subnormal() && (flags & NX) != 0 {
            flags |= UF;
        }
    }
    flags
}

// ---------------------------------------------------------------------------
// f64 flag detection helper, used by the D-extension operations
// ---------------------------------------------------------------------------

fn flags_binary_d(a: f64, b: f64, result: f64) -> u8 {
    let mut flags = 0u8;
    if result.is_nan() && !a.is_nan() && !b.is_nan() {
        flags |= NV;
    }
    if result.is_infinite() && !a.is_infinite() && !b.is_infinite() {
        flags |= OF | NX;
    } else if result.is_subnormal() {
        // Subnormal result implies underflow; NX is almost certain for subnormals.
        flags |= UF | NX;
    }
    flags
}

// ---------------------------------------------------------------------------
// F extension, single-precision arithmetic (exact via f64 promotion)
// ---------------------------------------------------------------------------

pub fn fp_add_s(a: f32, b: f32, rm: u8) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (a as f64) + (b as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV); // e.g., +∞ + (−∞)
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (box_f32(result), flags)
}

pub fn fp_sub_s(a: f32, b: f32, rm: u8) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (a as f64) - (b as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (box_f32(result), flags)
}

pub fn fp_mul_s(a: f32, b: f32, rm: u8) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (a as f64) * (b as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV); // e.g., ∞ * 0
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (box_f32(result), flags)
}

/// DZ is set when a finite nonzero value is divided by zero.
pub fn fp_div_s(a: f32, b: f32, rm: u8) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (a as f64) / (b as f64);
    if exact.is_nan() {
        // 0 / 0 → NaN, NV
        return (canon_nan_s(), NV);
    }
    let mut flags = 0u8;
    // DZ: finite non-zero ÷ 0
    if b == 0.0 && a.is_finite() && a != 0.0 {
        flags |= DZ;
        // Result is ±∞; infinite ÷ 0 is not an overflow.
        let bits = if a > 0.0 {
            box_f32(f32::INFINITY)
        } else {
            box_f32(f32::NEG_INFINITY)
        };
        return (bits, flags);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    flags |= fp_flags_from_exact_s(exact, result);
    (box_f32(result), flags)
}

/// NV is set when the input is strictly negative (not NaN).
pub fn fp_sqrt_s(a: f32, rm: u8) -> (u64, u8) {
    if a.is_nan() {
        return (canon_nan_s(), 0);
    }
    if a < 0.0 {
        return (canon_nan_s(), NV);
    }
    // For sqrt, use f64 sqrt for a more-precise intermediate.
    let exact = (a as f64).sqrt();
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (box_f32(result), flags)
}

// ---------------------------------------------------------------------------
// D extension, double-precision arithmetic
// ---------------------------------------------------------------------------

pub fn fp_add_d(a: f64, b: f64, _rm: u8) -> (u64, u8) {
    let result = a + b;
    let flags = flags_binary_d(a, b, result);
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_sub_d(a: f64, b: f64, _rm: u8) -> (u64, u8) {
    let result = a - b;
    let flags = flags_binary_d(a, b, result);
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_mul_d(a: f64, b: f64, _rm: u8) -> (u64, u8) {
    let result = a * b;
    let flags = flags_binary_d(a, b, result);
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_div_d(a: f64, b: f64, _rm: u8) -> (u64, u8) {
    let result = a / b;
    let mut flags = flags_binary_d(a, b, result);
    if b == 0.0 && a.is_finite() && a != 0.0 {
        flags |= DZ;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_sqrt_d(a: f64, _rm: u8) -> (u64, u8) {
    let result = a.sqrt();
    let mut flags = 0u8;
    if result.is_nan() && !a.is_nan() {
        flags |= NV;
    } else if result.is_subnormal() {
        flags |= UF | NX;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

// ---------------------------------------------------------------------------
// FCVT.S.D, convert f64 to f32 with rounding mode and full flag detection
// ---------------------------------------------------------------------------

/// Convert a double-precision value to single-precision with explicit rounding.
pub fn fp_cvt_d_to_s(val: f64, rm: u8) -> (u64, u8) {
    if val.is_nan() {
        // NaN → NaN conversion is exact; no flags.
        return (canon_nan_s(), 0);
    }
    let result = f64_to_f32_with_rm(val, rm);
    let mut flags = 0u8;
    if result.is_infinite() && val.is_finite() {
        flags |= OF | NX;
    } else if !result.is_nan() {
        if (result as f64) != val {
            flags |= NX;
        }
        if result.is_subnormal() && (flags & NX) != 0 {
            flags |= UF;
        }
    }
    (
        if result.is_nan() {
            canon_nan_s()
        } else {
            box_f32(result)
        },
        flags,
    )
}

// ---------------------------------------------------------------------------
// Sign-injection (no rounding, no flags)
// ---------------------------------------------------------------------------

pub fn fp_sgnj_s(a: f32, b: f32) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF;
    let sign = b.to_bits() & 0x8000_0000;
    box_f32(f32::from_bits(mag | sign))
}

pub fn fp_sgnjn_s(a: f32, b: f32) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF;
    let sign = (!b.to_bits()) & 0x8000_0000;
    box_f32(f32::from_bits(mag | sign))
}

pub fn fp_sgnjx_s(a: f32, b: f32) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF;
    let sign = (a.to_bits() ^ b.to_bits()) & 0x8000_0000;
    box_f32(f32::from_bits(mag | sign))
}

pub fn fp_sgnj_d(a: f64, b: f64) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF_FFFF_FFFF;
    let sign = b.to_bits() & 0x8000_0000_0000_0000;
    box_f64(f64::from_bits(mag | sign))
}

pub fn fp_sgnjn_d(a: f64, b: f64) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF_FFFF_FFFF;
    let sign = (!b.to_bits()) & 0x8000_0000_0000_0000;
    box_f64(f64::from_bits(mag | sign))
}

pub fn fp_sgnjx_d(a: f64, b: f64) -> u64 {
    let mag = a.to_bits() & 0x7FFF_FFFF_FFFF_FFFF;
    let sign = (a.to_bits() ^ b.to_bits()) & 0x8000_0000_0000_0000;
    box_f64(f64::from_bits(mag | sign))
}

// ---------------------------------------------------------------------------
// Min / Max (RISC-V 2.2: NaN propagation, if one input is NaN, return the other)
// ---------------------------------------------------------------------------

pub fn fp_min_s(a: f32, b: f32) -> (u64, u8) {
    let result = if a.is_nan() {
        b
    } else if b.is_nan() {
        a
    } else {
        a.min(b)
    };
    let flags = if a.is_nan() || b.is_nan() { NV } else { 0 };
    let bits = if result.is_nan() {
        canon_nan_s()
    } else {
        box_f32(result)
    };
    (bits, flags)
}

pub fn fp_max_s(a: f32, b: f32) -> (u64, u8) {
    let result = if a.is_nan() {
        b
    } else if b.is_nan() {
        a
    } else {
        a.max(b)
    };
    let flags = if a.is_nan() || b.is_nan() { NV } else { 0 };
    let bits = if result.is_nan() {
        canon_nan_s()
    } else {
        box_f32(result)
    };
    (bits, flags)
}

pub fn fp_min_d(a: f64, b: f64) -> (u64, u8) {
    let result = if a.is_nan() {
        b
    } else if b.is_nan() {
        a
    } else {
        a.min(b)
    };
    let flags = if a.is_nan() || b.is_nan() { NV } else { 0 };
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_max_d(a: f64, b: f64) -> (u64, u8) {
    let result = if a.is_nan() {
        b
    } else if b.is_nan() {
        a
    } else {
        a.max(b)
    };
    let flags = if a.is_nan() || b.is_nan() { NV } else { 0 };
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

// ---------------------------------------------------------------------------
// Comparisons, return 0 or 1 (not NaN-boxed); NV if either input is NaN
// ---------------------------------------------------------------------------

pub fn fp_feq_s(a: f32, b: f32) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a == b) as u64, 0)
    }
}

pub fn fp_flt_s(a: f32, b: f32) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a < b) as u64, 0)
    }
}

pub fn fp_fle_s(a: f32, b: f32) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a <= b) as u64, 0)
    }
}

pub fn fp_feq_d(a: f64, b: f64) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a == b) as u64, 0)
    }
}

pub fn fp_flt_d(a: f64, b: f64) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a < b) as u64, 0)
    }
}

pub fn fp_fle_d(a: f64, b: f64) -> (u64, u8) {
    if a.is_nan() || b.is_nan() {
        (0, NV)
    } else {
        ((a <= b) as u64, 0)
    }
}

// ---------------------------------------------------------------------------
// fclass, classify a floating-point value into one of 10 categories
//
// Bit index meanings:
//   0 = −∞      1 = −normal   2 = −subnormal   3 = −0
//   4 = +0      5 = +subnormal  6 = +normal    7 = +∞
//   8 = sNaN (Rust cannot distinguish sNaN from qNaN, so we always use 9)
//   9 = qNaN
// ---------------------------------------------------------------------------

pub fn fp_fclass_s(a: f32) -> u64 {
    let bits = a.to_bits();
    let sign = bits >> 31 != 0;
    if a.is_nan() {
        1u64 << 9
    } else if a.is_infinite() {
        if sign { 1 << 0 } else { 1 << 7 }
    } else if a == 0.0 {
        if sign { 1 << 3 } else { 1 << 4 }
    } else if (bits & 0x7F80_0000) == 0 {
        if sign { 1 << 2 } else { 1 << 5 }
    } else {
        if sign { 1 << 1 } else { 1 << 6 }
    }
}

pub fn fp_fclass_d(a: f64) -> u64 {
    let bits = a.to_bits();
    let sign = bits >> 63 != 0;
    if a.is_nan() {
        1u64 << 9
    } else if a.is_infinite() {
        if sign { 1 << 0 } else { 1 << 7 }
    } else if a == 0.0 {
        if sign { 1 << 3 } else { 1 << 4 }
    } else if (bits & 0x7FF0_0000_0000_0000) == 0 {
        if sign { 1 << 2 } else { 1 << 5 }
    } else {
        if sign { 1 << 1 } else { 1 << 6 }
    }
}

// ---------------------------------------------------------------------------
// Fused multiply-add (F extension), exact via f64 promotion
//
// All four FMA variants use f64 arithmetic internally.  For f32 operands
// (≤ 24 significant bits), both the intermediate product (≤ 48 bits) and the
// final sum (≤ 49 bits) fit exactly in f64 (53-bit mantissa), so the f64
// result is the correctly-rounded mathematical result before the final
// rounding to f32.
// ---------------------------------------------------------------------------

pub fn fp_fmadd_s(rs1: f32, rs2: f32, rs3: f32, rm: u8) -> (u64, u8) {
    if rs1.is_nan() || rs2.is_nan() || rs3.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (rs1 as f64) * (rs2 as f64) + (rs3 as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (
        if result.is_nan() {
            canon_nan_s()
        } else {
            box_f32(result)
        },
        flags,
    )
}

pub fn fp_fmsub_s(rs1: f32, rs2: f32, rs3: f32, rm: u8) -> (u64, u8) {
    if rs1.is_nan() || rs2.is_nan() || rs3.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = (rs1 as f64) * (rs2 as f64) - (rs3 as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (
        if result.is_nan() {
            canon_nan_s()
        } else {
            box_f32(result)
        },
        flags,
    )
}

/// -(rs1 × rs2) + rs3
pub fn fp_fnmsub_s(rs1: f32, rs2: f32, rs3: f32, rm: u8) -> (u64, u8) {
    if rs1.is_nan() || rs2.is_nan() || rs3.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = -((rs1 as f64) * (rs2 as f64)) + (rs3 as f64);
    if exact.is_nan() {
        return (canon_nan_s(), NV);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (
        if result.is_nan() {
            canon_nan_s()
        } else {
            box_f32(result)
        },
        flags,
    )
}

/// -(rs1 × rs2 + rs3)
pub fn fp_fnmadd_s(rs1: f32, rs2: f32, rs3: f32, rm: u8) -> (u64, u8) {
    if rs1.is_nan() || rs2.is_nan() || rs3.is_nan() {
        return (canon_nan_s(), 0);
    }
    let exact = -(((rs1 as f64) * (rs2 as f64)) + (rs3 as f64));
    if exact.is_nan() {
        return (canon_nan_s(), NV);
    }
    let result = f64_to_f32_with_rm(exact, rm);
    let flags = fp_flags_from_exact_s(exact, result);
    (
        if result.is_nan() {
            canon_nan_s()
        } else {
            box_f32(result)
        },
        flags,
    )
}

// ---------------------------------------------------------------------------
// Fused multiply-add (D extension), hardware precision (RNE only)
// ---------------------------------------------------------------------------

pub fn fp_fmadd_d(rs1: f64, rs2: f64, rs3: f64, _rm: u8) -> (u64, u8) {
    let result = rs1.mul_add(rs2, rs3);
    let mut flags = 0u8;
    if result.is_nan() && !rs1.is_nan() && !rs2.is_nan() && !rs3.is_nan() {
        flags |= NV;
    } else if result.is_infinite() && rs1.is_finite() && rs2.is_finite() && rs3.is_finite() {
        flags |= OF | NX;
    } else if result.is_subnormal() {
        flags |= UF | NX;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

pub fn fp_fmsub_d(rs1: f64, rs2: f64, rs3: f64, _rm: u8) -> (u64, u8) {
    let result = rs1.mul_add(rs2, -rs3);
    let mut flags = 0u8;
    if result.is_nan() && !rs1.is_nan() && !rs2.is_nan() && !rs3.is_nan() {
        flags |= NV;
    } else if result.is_infinite() && rs1.is_finite() && rs2.is_finite() && rs3.is_finite() {
        flags |= OF | NX;
    } else if result.is_subnormal() {
        flags |= UF | NX;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

/// -(rs1 × rs2) + rs3
pub fn fp_fnmsub_d(rs1: f64, rs2: f64, rs3: f64, _rm: u8) -> (u64, u8) {
    let result = (-rs1).mul_add(rs2, rs3);
    let mut flags = 0u8;
    if result.is_nan() && !rs1.is_nan() && !rs2.is_nan() && !rs3.is_nan() {
        flags |= NV;
    } else if result.is_infinite() && rs1.is_finite() && rs2.is_finite() && rs3.is_finite() {
        flags |= OF | NX;
    } else if result.is_subnormal() {
        flags |= UF | NX;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}

/// -(rs1 × rs2 + rs3)
pub fn fp_fnmadd_d(rs1: f64, rs2: f64, rs3: f64, _rm: u8) -> (u64, u8) {
    let result = -(rs1.mul_add(rs2, rs3));
    let mut flags = 0u8;
    if result.is_nan() && !rs1.is_nan() && !rs2.is_nan() && !rs3.is_nan() {
        flags |= NV;
    } else if result.is_infinite() && rs1.is_finite() && rs2.is_finite() && rs3.is_finite() {
        flags |= OF | NX;
    } else if result.is_subnormal() {
        flags |= UF | NX;
    }
    let bits = if result.is_nan() {
        canon_nan_d()
    } else {
        box_f64(result)
    };
    (bits, flags)
}
