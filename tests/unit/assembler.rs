use full_stack::assembly_language::assembler::directive::Directive;
use full_stack::assembly_language::assembler::encode::encode;
use full_stack::assembly_language::assembler::layout::compute_layout;
use full_stack::assembly_language::assembler::parser::parse;
use full_stack::assembly_language::assembler::reg_parse::{parse_float_reg, parse_int_reg};
use full_stack::assembly_language::assembler::section::SectionKind;
use full_stack::assembly_language::assembler::token::{AsmToken, BranchKind};
use full_stack::assembly_language::real::RealInstruction;
use full_stack::assembly_language::riscv::rv64i::Addi;
use full_stack::assembly_language::rv_instruction::RvInstruction;
use full_stack::assembly_language::utils::reg_name;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dir(s: &str) -> RvInstruction {
    RvInstruction::Directive(s.to_owned())
}

fn nop_token() -> AsmToken {
    AsmToken::Real(RealInstruction::Addi(Addi::new(0, 0, 0)))
}

fn parse_one(raw: &str) -> AsmToken {
    let tokens = parse(&[dir(raw)]);
    assert_eq!(tokens.len(), 1, "expected exactly one token from `{raw}`");
    tokens.into_iter().next().unwrap()
}

// ---------------------------------------------------------------------------
// directive::Directive parsing
// ---------------------------------------------------------------------------

#[test]
fn directive_parse_section_explicit() {
    assert_eq!(
        Directive::parse(".section .text"),
        Some(Directive::Section(".text".to_owned()))
    );
}

#[test]
fn directive_parse_section_shorthand() {
    assert_eq!(
        Directive::parse(".text"),
        Some(Directive::Section(".text".to_owned()))
    );
    assert_eq!(
        Directive::parse(".data"),
        Some(Directive::Section(".data".to_owned()))
    );
    assert_eq!(
        Directive::parse(".rodata"),
        Some(Directive::Section(".rodata".to_owned()))
    );
    assert_eq!(
        Directive::parse(".bss"),
        Some(Directive::Section(".bss".to_owned()))
    );
}

#[test]
fn directive_parse_globl() {
    assert_eq!(
        Directive::parse(".globl main"),
        Some(Directive::Globl("main".to_owned()))
    );
    assert_eq!(
        Directive::parse(".global _start"),
        Some(Directive::Globl("_start".to_owned()))
    );
}

#[test]
fn directive_parse_asciz_with_escapes() {
    assert_eq!(
        Directive::parse(r#".asciz "hello\n""#),
        Some(Directive::Asciz("hello\n".to_owned()))
    );
}

#[test]
fn directive_parse_numeric_data() {
    assert_eq!(Directive::parse(".byte 42"), Some(Directive::Byte(42)));
    assert_eq!(Directive::parse(".word 0"), Some(Directive::Word(0)));
    assert_eq!(Directive::parse(".space 16"), Some(Directive::Space(16)));
}

#[test]
fn directive_parse_unknown_is_not_none() {
    let d = Directive::parse(".some_future_directive");
    assert!(
        matches!(d, Some(Directive::Unknown(_))),
        "unknown directive should not return None"
    );
}

// ---------------------------------------------------------------------------
// reg_parse
// ---------------------------------------------------------------------------

#[test]
fn int_reg_round_trips_all() {
    for r in 0u8..=31 {
        let name = reg_name(r, false);
        assert_eq!(
            parse_int_reg(&name),
            Some(r),
            "int reg {r} ({name}) did not round-trip"
        );
    }
}

#[test]
fn float_reg_round_trips_all() {
    for r in 0u8..=31 {
        let name = reg_name(r, true);
        assert_eq!(
            parse_float_reg(&name),
            Some(r),
            "float reg {r} ({name}) did not round-trip"
        );
    }
}

#[test]
fn int_reg_aliases() {
    assert_eq!(parse_int_reg("zero"), Some(0));
    assert_eq!(parse_int_reg("x0"), Some(0));
    assert_eq!(parse_int_reg("ra"), Some(1));
    assert_eq!(parse_int_reg("x1"), Some(1));
    assert_eq!(parse_int_reg("sp"), Some(2));
    assert_eq!(parse_int_reg("fp"), Some(8));  // alias for s0
    assert_eq!(parse_int_reg("s0"), Some(8));
}

#[test]
fn unknown_reg_returns_none() {
    assert_eq!(parse_int_reg("notareg"), None);
    assert_eq!(parse_float_reg(""), None);
}

// ---------------------------------------------------------------------------
// parser::parse  (Pass 0)
// ---------------------------------------------------------------------------

#[test]
fn parser_section_directive() {
    let tok = parse_one(".section .text");
    assert!(matches!(tok, AsmToken::Section(SectionKind::Text)));
}

#[test]
fn parser_data_label() {
    let tok = parse_one("str_0:");
    assert!(matches!(tok, AsmToken::Label(n) if n == "str_0"));
}

#[test]
fn parser_branch_bne() {
    let tok = parse_one("\tbne a0, a1, .Lelse");
    assert!(
        matches!(
            &tok,
            AsmToken::Branch { kind: BranchKind::Bne, rs1: 10, rs2: 11, target }
            if target == ".Lelse"
        ),
        "unexpected token: {tok:?}"
    );
}

#[test]
fn parser_branch_beq() {
    let tok = parse_one("\tbeq s0, zero, .Lexit");
    assert!(
        matches!(
            &tok,
            AsmToken::Branch { kind: BranchKind::Beq, rs1: 8, rs2: 0, target }
            if target == ".Lexit"
        ),
        "unexpected token: {tok:?}"
    );
}

#[test]
fn parser_j_pseudo() {
    let tok = parse_one("\tj .Ltop");
    assert!(
        matches!(&tok, AsmToken::Jal { rd: 0, target } if target == ".Ltop"),
        "unexpected token: {tok:?}"
    );
}

#[test]
fn parser_jal_with_rd() {
    let tok = parse_one("\tjal ra, my_func");
    assert!(
        matches!(&tok, AsmToken::Jal { rd: 1, target } if target == "my_func"),
        "unexpected token: {tok:?}"
    );
}

#[test]
fn parser_real_instruction_passthrough() {
    let tokens = parse(&[RvInstruction::Real(RealInstruction::Addi(Addi::new(
        10, 0, 42,
    )))]);
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0], AsmToken::Real(_)));
}

#[test]
fn parser_label_passthrough() {
    let tokens = parse(&[RvInstruction::Label("main".to_owned())]);
    assert!(matches!(&tokens[0], AsmToken::Label(n) if n == "main"));
}

#[test]
fn parser_asciz_directive() {
    let tok = parse_one(r#"	.asciz "hello""#);
    assert!(
        matches!(&tok, AsmToken::DataAsciz(s) if s == "hello"),
        "unexpected token: {tok:?}"
    );
}

// ---------------------------------------------------------------------------
// layout::compute_layout  (Pass 1)
// ---------------------------------------------------------------------------

#[test]
fn layout_label_after_two_nops() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        nop_token(),
        nop_token(),
        AsmToken::Label("target".to_owned()),
        nop_token(),
    ];
    let layout = compute_layout(&tokens).unwrap();
    assert_eq!(layout.symbols.resolve("target"), Some(8));
}

#[test]
fn layout_align_pads_offset() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::DataU8(0xAB),
        AsmToken::Align(2), // align to 4 bytes: offset 1 → 4
        AsmToken::Label("after".to_owned()),
    ];
    let layout = compute_layout(&tokens).unwrap();
    assert_eq!(layout.symbols.resolve("after"), Some(4));
}

#[test]
fn layout_balign_pads_offset() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::DataU8(0x01),
        AsmToken::DataU8(0x02),
        AsmToken::DataU8(0x03),
        AsmToken::Balign(8), // offset 3 → 8
        AsmToken::Label("aligned".to_owned()),
    ];
    let layout = compute_layout(&tokens).unwrap();
    assert_eq!(layout.symbols.resolve("aligned"), Some(8));
}

#[test]
fn layout_multiple_sections() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        nop_token(),
        AsmToken::Section(SectionKind::Data),
        AsmToken::Label("data_start".to_owned()),
        AsmToken::DataU32(0xDEADBEEF),
    ];
    let layout = compute_layout(&tokens).unwrap();
    // data_start is at offset 0 within the data section
    assert_eq!(layout.symbols.resolve("data_start"), Some(0));
    assert!(layout.section_order.contains(&SectionKind::Text));
    assert!(layout.section_order.contains(&SectionKind::Data));
}

#[test]
fn layout_duplicate_label_is_error() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::Label("foo".to_owned()),
        nop_token(),
        AsmToken::Label("foo".to_owned()),
    ];
    assert!(compute_layout(&tokens).is_err());
}

// ---------------------------------------------------------------------------
// encode::encode  (Pass 2)
// ---------------------------------------------------------------------------

fn text_section_tokens_with_forward_branch() -> Vec<AsmToken> {
    // .text
    // start:  nop   @ 0x00
    //         bne a0, a1, end  @ 0x04
    //         nop   @ 0x08
    // end:    nop   @ 0x0c
    vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::Label("start".to_owned()),
        nop_token(),
        AsmToken::Branch {
            kind: BranchKind::Bne,
            rs1: 10,
            rs2: 11,
            target: "end".to_owned(),
        },
        nop_token(),
        AsmToken::Label("end".to_owned()),
        nop_token(),
    ]
}

#[test]
fn encode_forward_branch_resolves() {
    let tokens = text_section_tokens_with_forward_branch();
    let layout = compute_layout(&tokens).unwrap();
    let output = encode(&tokens, &layout).unwrap();

    assert_eq!(*output.symbol_table.get("end").unwrap(), 12);

    let text = output.text_bytes();
    let word = u32::from_le_bytes(text[4..8].try_into().unwrap());
    // BNE: opcode = 0x63, funct3 = 1
    assert_eq!(word & 0x7F, 0x63, "wrong opcode for branch");
    assert_eq!((word >> 12) & 0x7, 1, "wrong funct3 — expected BNE");
}

#[test]
fn encode_backward_branch_resolves() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::Label("top".to_owned()),
        nop_token(),
        // beq x0, x0, top  (offset = -4)
        AsmToken::Branch {
            kind: BranchKind::Beq,
            rs1: 0,
            rs2: 0,
            target: "top".to_owned(),
        },
    ];
    let layout = compute_layout(&tokens).unwrap();
    let output = encode(&tokens, &layout).unwrap();

    let text = output.text_bytes();
    let word = u32::from_le_bytes(text[4..8].try_into().unwrap());
    // BEQ: opcode = 0x63, funct3 = 0
    assert_eq!(word & 0x7F, 0x63, "wrong opcode for branch");
    assert_eq!((word >> 12) & 0x7, 0, "wrong funct3 — expected BEQ");
}

#[test]
fn encode_jal_resolves() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        // jal ra, target  @ 0x00
        AsmToken::Jal { rd: 1, target: "target".to_owned() },
        nop_token(),          // @ 0x04
        nop_token(),          // @ 0x08
        AsmToken::Label("target".to_owned()),
        nop_token(),          // @ 0x0c
    ];
    let layout = compute_layout(&tokens).unwrap();
    let output = encode(&tokens, &layout).unwrap();

    let text = output.text_bytes();
    let word = u32::from_le_bytes(text[0..4].try_into().unwrap());
    // JAL opcode = 0x6F
    assert_eq!(word & 0x7F, 0x6F, "wrong opcode for JAL");
}

#[test]
fn encode_undefined_label_is_error() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::Branch {
            kind: BranchKind::Beq,
            rs1: 0,
            rs2: 0,
            target: "nowhere".to_owned(),
        },
    ];
    let layout = compute_layout(&tokens).unwrap();
    assert!(encode(&tokens, &layout).is_err());
}

#[test]
fn encode_data_section_bytes() {
    let tokens = vec![
        AsmToken::Section(SectionKind::RoData),
        AsmToken::Label("str_0".to_owned()),
        AsmToken::DataAsciz("hi".to_owned()),
    ];
    let layout = compute_layout(&tokens).unwrap();
    let output = encode(&tokens, &layout).unwrap();

    let rodata = output.rodata_bytes();
    assert_eq!(rodata, b"hi\0");
}

#[test]
fn encode_globl_is_exported() {
    let tokens = vec![
        AsmToken::Section(SectionKind::Text),
        AsmToken::Globl("main".to_owned()),
        AsmToken::Label("main".to_owned()),
        nop_token(),
    ];
    let layout = compute_layout(&tokens).unwrap();
    let output = encode(&tokens, &layout).unwrap();

    assert!(
        output.global_symbols.contains(&"main".to_owned()),
        "main should be exported"
    );
}
