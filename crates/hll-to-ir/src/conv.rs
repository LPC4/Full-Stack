//! Small, intent-revealing integer conversions used when building IR values.

/// Convert a size, alignment, length, or index to `i64` for an IR integer.
/// These values are far below `i64::MAX`, so saturation never triggers.
pub(crate) fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Reinterpret a `u64` bit pattern as `i64` (two's complement), preserving bits.
/// Used where unsigned arithmetic or a literal is stored as a signed IR integer.
pub(crate) fn u64_bits_to_i64(value: u64) -> i64 {
    i64::from_ne_bytes(value.to_ne_bytes())
}
