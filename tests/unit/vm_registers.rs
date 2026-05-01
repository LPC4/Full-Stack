use full_stack::virtual_machine::cpu::registers::Registers;

#[test]
fn x0_always_zero() {
    let mut regs = Registers::new();
    regs.write_x(0, 42);
    assert_eq!(regs.read_x(0), 0, "x0 must always read as zero");
}

#[test]
fn x_roundtrip() {
    let mut regs = Registers::new();
    regs.write_x(1, 0xDEAD_BEEF);
    assert_eq!(regs.read_x(1), 0xDEAD_BEEF);
}

#[test]
fn fp_nan_box_write_read_f32() {
    let mut regs = Registers::new();
    regs.write_f32(0, 3.14f32);
    let read_back = regs.read_f32(0);
    assert!(
        (read_back - 3.14f32).abs() < 1e-6,
        "read_f32 should return approximately 3.14, got {read_back}"
    );
    // Upper 32 bits must be 0xFFFF_FFFF (NaN-boxing)
    let bits = regs.read_f_bits(0);
    assert_eq!(
        (bits >> 32) as u32,
        0xFFFF_FFFF,
        "upper 32 bits of NaN-boxed f32 must be 0xFFFF_FFFF"
    );
}

#[test]
fn fp_nan_box_invalid_upper() {
    let mut regs = Registers::new();
    // Write raw bits where upper 32 bits are NOT all 1s, invalid NaN box
    regs.write_f_bits(0, 0x0000_0000_4048_F5C3); // upper = 0, lower = 3.14f32 bits
    let val = regs.read_f32(0);
    assert!(val.is_nan(), "read_f32 with invalid NaN box should return NaN, got {val}");
}

#[test]
fn fp_f64_roundtrip() {
    let mut regs = Registers::new();
    let e = 2.718_281_828_f64;
    regs.write_f64(0, e);
    let read_back = regs.read_f64(0);
    assert_eq!(
        read_back.to_bits(),
        e.to_bits(),
        "f64 round-trip must preserve exact bit pattern"
    );
}

#[test]
fn pc_default_zero() {
    let regs = Registers::new();
    assert_eq!(regs.pc, 0, "PC should be zero after Registers::new()");
}
