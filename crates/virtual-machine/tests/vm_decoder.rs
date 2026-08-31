use asm_to_binary::riscv::rv64i::*;
use asm_to_binary::riscv::rv64m::*;
use asm_to_binary::traits::Instruction as _;
use riscv_model::{InstructionFormat, REGISTER_REGISTER_SPECS, RETAINED_ENCODING_VECTORS};
use virtual_machine::cpu::decoder::{DecodedInsn, decode};
use virtual_machine::error::VmError;

#[test]
fn decoder_matches_retained_riscv_opcode_vectors() {
    for vector in RETAINED_ENCODING_VECTORS {
        let decoded = decode(vector.word).expect("retained encoding should decode");
        assert_eq!(decoded.instruction_id(), Some(vector.id));
    }
}

#[test]
fn shared_upper_immediate_decoder_matches_legacy_fields() {
    let immediates = [
        0x0000_0000,
        0x0000_1000,
        0x7fff_f000,
        0x8000_0000,
        0xffff_f000,
    ];
    for (opcode, expected_id) in [
        (0x37, riscv_model::InstructionId::Lui),
        (0x17, riscv_model::InstructionId::Auipc),
    ] {
        for rd in 0..32 {
            for immediate in immediates {
                let word = immediate | (rd << 7) | opcode;
                let decoded = decode(word).expect("upper-immediate word should decode");
                assert_eq!(decoded.instruction_id(), Some(expected_id));
                match decoded {
                    DecodedInsn::Lui { rd: actual_rd, imm }
                    | DecodedInsn::Auipc { rd: actual_rd, imm } => {
                        assert_eq!(actual_rd, rd as usize);
                        assert_eq!(imm, i64::from(immediate.cast_signed()));
                    }
                    other => panic!("expected upper-immediate instruction, got {other:?}"),
                }
            }
        }
    }
}

#[test]
fn shared_register_register_decoder_matches_legacy_fields() {
    for spec in REGISTER_REGISTER_SPECS {
        for rd in [0_u8, 1, 30, 31] {
            for rs1 in [0_u8, 1, 30, 31] {
                for rs2 in [0_u8, 1, 30, 31] {
                    let word = spec.match_bits
                        | (u32::from(rs2) << 20)
                        | (u32::from(rs1) << 15)
                        | (u32::from(rd) << 7);
                    let decoded = decode(word).expect("register-register word should decode");
                    assert_eq!(decoded.instruction_id(), Some(spec.id));
                    match (spec.format, decoded) {
                        (
                            InstructionFormat::RegisterRegister,
                            DecodedInsn::Alu {
                                rd: actual_rd,
                                rs1: actual_rs1,
                                rs2: actual_rs2,
                                ..
                            },
                        )
                        | (
                            InstructionFormat::RegisterRegisterWord,
                            DecodedInsn::Alu32 {
                                rd: actual_rd,
                                rs1: actual_rs1,
                                rs2: actual_rs2,
                                ..
                            },
                        ) => {
                            assert_eq!(
                                (actual_rd, actual_rs1, actual_rs2),
                                (usize::from(rd), usize::from(rs1), usize::from(rs2))
                            );
                        }
                        (format, other) => {
                            panic!(
                                "expected {format:?} register-register instruction, got {other:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_register_register_decoder_rejects_reserved_selectors() {
    let word = 0xfe00_0033;
    assert!(matches!(decode(word), Err(VmError::IllegalInstruction(actual)) if actual == word));
}

#[test]
fn decode_lui() {
    // Lui::new(rd, imm), imm is the full 32-bit value whose upper 20 bits are used.
    // Lui::new(5, 0x12345 << 12) would put 0x12345 in upper bits.
    // But the u_inst! macro takes imm as i32 and masks lower 12 bits during encode.
    // Pass the already-shifted value so upper 20 bits = 0x12345.
    let encoded = Lui::new(5, 0x1234_5000_i32).encode();
    let decoded = decode(encoded).expect("should decode LUI");
    if let DecodedInsn::Lui { rd, imm } = decoded {
        assert_eq!(rd, 5);
        // The decoded imm is the full sign-extended 32-bit upper-immediate.
        // 0x12345000 as i32 = 0x12345000 (positive, fits in i32).
        let expected_imm = i64::from(0x1234_5000_i32);
        assert_eq!(
            imm, expected_imm,
            "LUI immediate mismatch: got {imm:#x}, expected {expected_imm:#x}"
        );
    } else {
        panic!("expected DecodedInsn::Lui, got {decoded:?}");
    }
}

#[test]
fn decode_addi() {
    let encoded = Addi::new(1, 2, -42).encode();
    let decoded = decode(encoded).expect("should decode ADDI");
    if let DecodedInsn::AluImm {
        rd,
        rs1,
        imm,
        funct3,
        ..
    } = decoded
    {
        assert_eq!(rd, 1);
        assert_eq!(rs1, 2);
        assert_eq!(imm, -42);
        assert_eq!(funct3, 0);
    } else {
        panic!("expected DecodedInsn::AluImm, got {decoded:?}");
    }
}

#[test]
fn decode_add() {
    let encoded = Add::new(3, 1, 2).encode();
    let decoded = decode(encoded).expect("should decode ADD");
    if let DecodedInsn::Alu {
        rd,
        rs1,
        rs2,
        funct3,
        funct7,
    } = decoded
    {
        assert_eq!(rd, 3);
        assert_eq!(rs1, 1);
        assert_eq!(rs2, 2);
        assert_eq!(funct3, 0);
        assert_eq!(funct7, 0);
    } else {
        panic!("expected DecodedInsn::Alu, got {decoded:?}");
    }
}

#[test]
fn decode_sub() {
    let encoded = Sub::new(3, 1, 2).encode();
    let decoded = decode(encoded).expect("should decode SUB");
    if let DecodedInsn::Alu { funct7, .. } = decoded {
        assert_eq!(funct7, 0x20, "SUB funct7 must be 0x20");
    } else {
        panic!("expected DecodedInsn::Alu, got {decoded:?}");
    }
}

#[test]
fn decode_mul() {
    let encoded = Mul::new(3, 1, 2).encode();
    let decoded = decode(encoded).expect("should decode MUL");
    if let DecodedInsn::Alu { funct7, funct3, .. } = decoded {
        assert_eq!(funct7, 0x01, "MUL funct7 must be 0x01");
        assert_eq!(funct3, 0);
    } else {
        panic!("expected DecodedInsn::Alu, got {decoded:?}");
    }
}

#[test]
fn decode_lw() {
    // Lw::new(rd, base, offset)
    let encoded = Lw::new(5, 6, 100).encode();
    let decoded = decode(encoded).expect("should decode LW");
    if let DecodedInsn::Load {
        funct3,
        rd,
        rs1,
        imm,
    } = decoded
    {
        assert_eq!(funct3, 2, "LW funct3 must be 2");
        assert_eq!(rd, 5);
        assert_eq!(rs1, 6);
        assert_eq!(imm, 100);
    } else {
        panic!("expected DecodedInsn::Load, got {decoded:?}");
    }
}

#[test]
fn decode_sw() {
    // Sw::new(base, src, offset)
    let encoded = Sw::new(6, 7, -8).encode();
    let decoded = decode(encoded).expect("should decode SW");
    if let DecodedInsn::Store {
        funct3,
        rs1,
        rs2,
        imm,
    } = decoded
    {
        assert_eq!(funct3, 2, "SW funct3 must be 2");
        assert_eq!(rs1, 6);
        assert_eq!(rs2, 7);
        assert_eq!(imm, -8);
    } else {
        panic!("expected DecodedInsn::Store, got {decoded:?}");
    }
}

#[test]
fn decode_beq() {
    // Beq::new(rs1, rs2, offset)
    let encoded = Beq::new(1, 2, 8).encode();
    let decoded = decode(encoded).expect("should decode BEQ");
    if let DecodedInsn::Branch { funct3, imm, .. } = decoded {
        assert_eq!(funct3, 0, "BEQ funct3 must be 0");
        assert_eq!(imm, 8, "BEQ immediate mismatch");
    } else {
        panic!("expected DecodedInsn::Branch, got {decoded:?}");
    }
}

#[test]
fn decode_ecall() {
    let encoded = Ecall::new().encode();
    let decoded = decode(encoded).expect("should decode ECALL");
    assert!(
        matches!(decoded, DecodedInsn::Ecall),
        "expected Ecall variant, got {decoded:?}"
    );
}

#[test]
fn decode_illegal() {
    // Opcode 0 is not a valid RISC-V opcode
    let result = decode(0x0000_0000);
    assert!(
        matches!(result, Err(VmError::IllegalInstruction(0))),
        "opcode 0 should be illegal, got {result:?}"
    );
}
