use asm_to_binary::riscv::rv64i::{
    Add, Addi, Addw, And, Auipc, Ecall, Lui, Or, Sll, Sllw, Slt, Sltu, Sra, Sraw, Srl, Srlw, Sub,
    Subw, Xor,
};
use asm_to_binary::riscv::rv64m::{
    Div, Divu, Divuw, Divw, Mul, Mulh, Mulhsu, Mulhu, Mulw, Rem, Remu, Remuw, Remw,
};
use asm_to_binary::traits::Instruction as _;
use riscv_model::RETAINED_ENCODING_VECTORS;

#[test]
fn encoders_match_retained_riscv_opcode_vectors() {
    let encoded = [
        Lui::new(1, 0x1234_5000).encode(),
        Addi::new(1, 2, -1).encode(),
        Add::new(1, 2, 3).encode(),
        Sub::new(1, 2, 3).encode(),
        Mul::new(1, 2, 3).encode(),
        Ecall.encode(),
    ];

    assert_eq!(encoded.len(), RETAINED_ENCODING_VECTORS.len());
    for (actual, vector) in encoded.into_iter().zip(RETAINED_ENCODING_VECTORS) {
        assert_eq!(actual, vector.word, "{} encoding", vector.id.mnemonic());
    }
}

#[test]
fn shared_upper_immediate_encoder_matches_legacy_formula() {
    let immediates = [i32::MIN, -4097, -4096, -1, 0, 1, 4095, 4096, i32::MAX];
    for rd in 0..32 {
        for immediate in immediates {
            let fields = (immediate.cast_unsigned() & 0xffff_f000) | (u32::from(rd) << 7);
            assert_eq!(Lui::new(rd, immediate).encode(), fields | 0x37);
            assert_eq!(Auipc::new(rd, immediate).encode(), fields | 0x17);
        }
    }
}

#[test]
#[should_panic(expected = "register x32 is out of range")]
fn upper_immediate_constructor_rejects_invalid_register() {
    let _ = Lui::new(32, 0);
}

macro_rules! assert_register_register_encoding {
    ($name:ident, $opcode:literal, $funct3:literal, $funct7:literal) => {
        for rd in [0, 1, 30, 31] {
            for rs1 in [0, 1, 30, 31] {
                for rs2 in [0, 1, 30, 31] {
                    let expected = ($funct7 << 25)
                        | (u32::from(rs2) << 20)
                        | (u32::from(rs1) << 15)
                        | ($funct3 << 12)
                        | (u32::from(rd) << 7)
                        | $opcode;
                    assert_eq!($name::new(rd, rs1, rs2).encode(), expected);
                }
            }
        }
    };
}

#[test]
fn shared_register_register_encoder_matches_legacy_formulas() {
    assert_register_register_encoding!(Add, 0x33, 0, 0x00);
    assert_register_register_encoding!(Sub, 0x33, 0, 0x20);
    assert_register_register_encoding!(Sll, 0x33, 1, 0x00);
    assert_register_register_encoding!(Slt, 0x33, 2, 0x00);
    assert_register_register_encoding!(Sltu, 0x33, 3, 0x00);
    assert_register_register_encoding!(Xor, 0x33, 4, 0x00);
    assert_register_register_encoding!(Srl, 0x33, 5, 0x00);
    assert_register_register_encoding!(Sra, 0x33, 5, 0x20);
    assert_register_register_encoding!(Or, 0x33, 6, 0x00);
    assert_register_register_encoding!(And, 0x33, 7, 0x00);
    assert_register_register_encoding!(Addw, 0x3b, 0, 0x00);
    assert_register_register_encoding!(Subw, 0x3b, 0, 0x20);
    assert_register_register_encoding!(Sllw, 0x3b, 1, 0x00);
    assert_register_register_encoding!(Srlw, 0x3b, 5, 0x00);
    assert_register_register_encoding!(Sraw, 0x3b, 5, 0x20);
    assert_register_register_encoding!(Mul, 0x33, 0, 0x01);
    assert_register_register_encoding!(Mulh, 0x33, 1, 0x01);
    assert_register_register_encoding!(Mulhsu, 0x33, 2, 0x01);
    assert_register_register_encoding!(Mulhu, 0x33, 3, 0x01);
    assert_register_register_encoding!(Div, 0x33, 4, 0x01);
    assert_register_register_encoding!(Divu, 0x33, 5, 0x01);
    assert_register_register_encoding!(Rem, 0x33, 6, 0x01);
    assert_register_register_encoding!(Remu, 0x33, 7, 0x01);
    assert_register_register_encoding!(Mulw, 0x3b, 0, 0x01);
    assert_register_register_encoding!(Divw, 0x3b, 4, 0x01);
    assert_register_register_encoding!(Divuw, 0x3b, 5, 0x01);
    assert_register_register_encoding!(Remw, 0x3b, 6, 0x01);
    assert_register_register_encoding!(Remuw, 0x3b, 7, 0x01);
}

#[test]
fn register_register_constructor_rejects_invalid_registers() {
    assert!(std::panic::catch_unwind(|| Add::new(32, 0, 0)).is_err());
    assert!(std::panic::catch_unwind(|| Add::new(0, 32, 0)).is_err());
    assert!(std::panic::catch_unwind(|| Add::new(0, 0, 32)).is_err());
}
