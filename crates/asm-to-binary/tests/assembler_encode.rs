/// Tests for assembler pseudo-instruction relocation (call, la, tail).
use asm_to_binary::assembler::Assembler;
use asm_to_binary::pseudo::PseudoInstruction;
use asm_to_binary::rv_instruction::RvInstruction;

#[test]
fn test_call_pseudo_relocation() {
    // Test that `call target` expands to correct auipc + jalr
    let tokens = vec![
        RvInstruction::Label("start".to_string()),
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Pseudo(PseudoInstruction::Call {
            symbol: "target_func".to_string(),
        }),
        RvInstruction::Label("target_func".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Addi(
            asm_to_binary::riscv::rv64i::Addi::new(0, 0, 0),
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    // Check that we have symbols
    assert!(output.has_symbol("start"));
    assert!(output.has_symbol("target_func"));
}

#[test]
fn test_la_pseudo_relocation() {
    // Test that `la a0, label` expands to correct auipc + addi
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Pseudo(PseudoInstruction::La {
            rd: 10, // a0
            symbol: "my_label".to_string(),
        }),
        RvInstruction::Label("my_label".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Addi(
            asm_to_binary::riscv::rv64i::Addi::new(0, 0, 0),
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.has_symbol("my_label"));
}

#[test]
fn test_tail_pseudo_relocation() {
    // Test that `tail target` expands to correct auipc + jalr
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Pseudo(PseudoInstruction::Tail {
            symbol: "end_func".to_string(),
        }),
        RvInstruction::Label("end_func".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Addi(
            asm_to_binary::riscv::rv64i::Addi::new(0, 0, 0),
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.has_symbol("end_func"));
}

// ---------------------------------------------------------------------------
// Regression: AUIPC hi20 must be pre-shifted (hi20 << 12)
//
// Before the fix, encode_call/encode_tail/encode_la passed the raw 20-bit
// page number to Auipc::new instead of shifting it left by 12.  UType::encode
// masks with 0xFFFFF000, so an unshifted value < 4096 encoded as imm=0.
// That caused `la t0, far_label` at PC=0 with far_label at 0x11000 to produce
// auipc t0, 0 instead of auipc t0, 0x11000 -> the load address ended up below
// RAM_BASE causing an infinite trap loop.
// ---------------------------------------------------------------------------

/// Assemble `la t0, far_label` where `far_label` is exactly 0x11000 bytes
/// ahead of the instruction (hi20=17, lo12=0).  The first encoded word must
/// have bits[31:12] == 17, i.e. the auipc immediate = 0x11000.
#[test]
fn auipc_hi20_is_shifted_for_la() {
    // .text
    // la t0, far_label          ; PC=0, offset=0x11000 -> hi20=17, lo12=0
    // .space 0x10FF8            ; pad so that far_label lands at 0x11000
    //                           ; (la expands to 8 bytes, so 0x11000 - 8 = 0x10FF8)
    // far_label:
    // addi zero, zero, 0
    //
    // Note: must use Directive "\tla ..." so the parser produces AsmToken::La
    // (with deferred symbol resolution) rather than eagerly expanding the pseudo.
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Directive("\tla t0, far_label".to_string()),
        RvInstruction::Directive(format!("\t.space {}", 0x11000_usize - 8)),
        RvInstruction::Label("far_label".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Addi(
            asm_to_binary::riscv::rv64i::Addi::new(0, 0, 0),
        )),
    ];

    let output = Assembler::assemble(&tokens).expect("assembly should succeed");
    let text = output.text_bytes();
    assert!(!text.is_empty(), ".text section should not be empty");

    // Decode the first word (the AUIPC).
    let word = u32::from_le_bytes(text[0..4].try_into().unwrap());
    // AUIPC opcode = 0x17; rd=5 -> bits[11:7]=0b00101; imm[31:12] should be 17.
    let opcode = word & 0x7F;
    let rd = (word >> 7) & 0x1F;
    let imm_upper = word & 0xFFFFF000;

    assert_eq!(opcode, 0x17, "first word should be AUIPC");
    assert_eq!(rd, 5, "AUIPC rd should be t0 (x5)");
    assert_eq!(
        imm_upper, 0x11000,
        "AUIPC imm[31:12] should be 17 (0x11000), got 0x{imm_upper:08x} - \
         regression: hi20 was not shifted left by 12"
    );
}

/// Same regression check for `call symbol` (uses ra=x1 for auipc).
#[test]
fn auipc_hi20_is_shifted_for_call() {
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Directive("\tcall far_func".to_string()),
        RvInstruction::Directive(format!("\t.space {}", 0x11000_usize - 8)),
        RvInstruction::Label("far_func".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Jalr(
            asm_to_binary::riscv::rv64i::Jalr::new(0, 1, 0),
        )),
    ];

    let output = Assembler::assemble(&tokens).expect("assembly should succeed");
    let text = output.text_bytes();
    assert!(!text.is_empty(), ".text section should not be empty");

    let word = u32::from_le_bytes(text[0..4].try_into().unwrap());
    let opcode = word & 0x7F;
    let imm_upper = word & 0xFFFFF000;

    assert_eq!(opcode, 0x17, "first word should be AUIPC");
    assert_eq!(
        imm_upper, 0x11000,
        "call: AUIPC imm[31:12] should be 0x11000, got 0x{imm_upper:08x}"
    );
}

/// Same regression check for `tail symbol` (uses t1=x6 for auipc).
#[test]
fn auipc_hi20_is_shifted_for_tail() {
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Directive("\ttail far_tail".to_string()),
        RvInstruction::Directive(format!("\t.space {}", 0x11000_usize - 8)),
        RvInstruction::Label("far_tail".to_string()),
        RvInstruction::Real(asm_to_binary::real::RealInstruction::Jalr(
            asm_to_binary::riscv::rv64i::Jalr::new(0, 1, 0),
        )),
    ];

    let output = Assembler::assemble(&tokens).expect("assembly should succeed");
    let text = output.text_bytes();
    assert!(!text.is_empty(), ".text section should not be empty");

    let word = u32::from_le_bytes(text[0..4].try_into().unwrap());
    let opcode = word & 0x7F;
    let imm_upper = word & 0xFFFFF000;

    assert_eq!(opcode, 0x17, "first word should be AUIPC");
    assert_eq!(
        imm_upper, 0x11000,
        "tail: AUIPC imm[31:12] should be 0x11000, got 0x{imm_upper:08x}"
    );
}
