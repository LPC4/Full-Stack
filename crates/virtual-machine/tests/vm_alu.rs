use virtual_machine::cpu::alu::*;

// ---------------------------------------------------------------------------
// Integer ALU, RV64I
// ---------------------------------------------------------------------------

#[test]
fn add_wraps() {
    assert_eq!(add(u64::MAX, 1), 0);
}

#[test]
fn sub_wraps() {
    assert_eq!(sub(0, 1), u64::MAX);
}

#[test]
fn slt_signed() {
    // u64::MAX == -1 as i64, which is less than 0
    assert_eq!(slt(u64::MAX, 0), 1);
}

#[test]
fn sltu_unsigned() {
    // u64::MAX > 0 as unsigned
    assert_eq!(sltu(u64::MAX, 0), 0);
}

#[test]
fn sra_sign_extends() {
    // Arithmetic right shift of the sign bit should propagate the sign
    let input = 0x8000_0000_0000_0000u64;
    let result = sra(input, 1);
    assert_eq!(result, 0xC000_0000_0000_0000u64);
}

#[test]
fn addw_sign_extends() {
    // 0xFFFF_FFFF + 1 as i32 = 0, sign-extended to 64 bits = 0
    assert_eq!(addw(0xFFFF_FFFF, 1), 0);
}

// ---------------------------------------------------------------------------
// M extension, divide / remainder edge cases
// ---------------------------------------------------------------------------

#[test]
fn div_by_zero() {
    assert_eq!(divu(42, 0), u64::MAX);
    assert_eq!(div(42i64 as u64, 0), u64::MAX);
}

#[test]
fn rem_by_zero() {
    assert_eq!(remu(42, 0), 42);
    assert_eq!(rem(42i64 as u64, 0), 42);
}

#[test]
fn div_overflow() {
    // i64::MIN / -1 is undefined in two's-complement; RISC-V defines result = i64::MIN
    assert_eq!(div(i64::MIN as u64, u64::MAX /* = -1 as i64 */), i64::MIN as u64);
}

#[test]
fn rem_overflow() {
    // i64::MIN % -1: RISC-V defines result = 0
    assert_eq!(rem(i64::MIN as u64, u64::MAX), 0);
}

// ---------------------------------------------------------------------------
// M extension, multiply high
// ---------------------------------------------------------------------------

#[test]
fn mulh_signed() {
    // (-1) * (-1) = 1; upper 64 bits of the 128-bit product = 0
    assert_eq!(mulh(0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF_FFFF_FFFF), 0);
}

#[test]
fn mulhu_unsigned() {
    // u64::MAX * 2 = 2^65 - 2; upper 64 bits = 1
    assert_eq!(mulhu(u64::MAX, 2), 1);
}

// ---------------------------------------------------------------------------
// FP, single-precision
// ---------------------------------------------------------------------------

#[test]
fn fp_add_s_basic() {
    let (bits, flags) = fp_add_s(1.0f32, 2.0f32, RM_RNE);
    let result = f32::from_bits(bits as u32);
    assert_eq!(result, 3.0f32);
    assert_eq!(flags, 0, "no exception flags expected for 1.0 + 2.0");
}

#[test]
fn fp_div_by_zero_flag() {
    let (_, flags) = fp_div_s(1.0f32, 0.0f32, RM_RNE);
    // DZ flag = 0x08
    assert_ne!(flags & 0x08, 0, "DZ flag must be set when dividing by zero");
}

#[test]
fn fp_sqrt_negative_nv() {
    let (_, flags) = fp_sqrt_s(-1.0f32, RM_RNE);
    // NV flag = 0x10
    assert_ne!(flags & 0x10, 0, "NV flag must be set for sqrt of negative number");
}

#[test]
fn fp_min_nan_propagation() {
    // When one input is NaN, the non-NaN value should be returned
    let (bits, _flags) = fp_min_s(f32::NAN, 1.0f32);
    let result = f32::from_bits(bits as u32);
    assert_eq!(result, 1.0f32, "fp_min with one NaN input should return the non-NaN value");
}

// ---------------------------------------------------------------------------
// FP, double-precision fclass
// ---------------------------------------------------------------------------

#[test]
fn fp_fclass_inf() {
    // Positive infinity should set bit 7
    let class = fp_fclass_d(f64::INFINITY);
    assert_eq!(class, 1 << 7, "positive infinity should be class bit 7");
}
