/// Multi-pass assembler: `Vec<RvInstruction>` -> `AssembledOutput`.
///
/// # Passes
///
/// | Pass | File | Responsibility |
/// |------|------|----------------|
/// | 0 :  Parse  | `parser.rs`   | `RvInstruction` -> `Vec<AsmToken>` (typed, no raw strings) |
/// | 1 :  Layout | `layout.rs`   | Walk tokens, compute every label's section-relative address |
/// | 2 :  Encode | `encode.rs`   | Emit bytes, resolve branch/jump offsets via symbol table |
pub(crate) mod directive;
pub(crate) mod encode;
pub(crate) mod layout;
pub mod link_layout;
pub(crate) mod output;
pub(crate) mod parser;
pub(crate) mod reg_parse;
pub(crate) mod section;
pub(crate) mod symbol_table;
pub(crate) mod token;

pub use link_layout::LinkLayout;

use crate::rv_instruction::RvInstruction;
use output::AssembledOutput;

/// Error produced by any pass.
#[derive(Debug, Clone)]
pub struct AssemblerError {
    pub message: String,
}

impl std::fmt::Display for AssemblerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "assembler error: {}", self.message)
    }
}

impl AssemblerError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

pub struct Assembler;

impl Assembler {
    /// Assemble a `RvInstruction` token stream into machine code.
    ///
    /// # Errors
    /// Returns an error if a label is undefined/duplicated, or if a branch
    /// offset falls outside the encodable range.
    pub fn assemble(tokens: &[RvInstruction]) -> Result<AssembledOutput, AssemblerError> {
        // Pass 0: parse raw strings into fully-typed AsmTokens.
        let asm_tokens = parser::parse(tokens);

        // Pass 1: compute label addresses.
        let layout = layout::compute_layout(&asm_tokens)?;

        // Pass 2: encode to bytes, resolving all symbol references.
        encode::encode(&asm_tokens, &layout)
    }
}

#[cfg(test)]
mod tests {
    use super::Assembler;
    use super::directive::Directive;
    use super::encode::encode;
    use super::layout::compute_layout;
    use super::parser::parse;
    use super::reg_parse::{parse_float_reg, parse_int_reg};
    use super::section::SectionKind;
    use super::token::{AsmToken, BranchKind};
    use crate::real::RealInstruction;
    use crate::riscv::rv64i::Addi;
    use crate::rv_instruction::RvInstruction;
    use crate::utils::reg_name;

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

    // ---- directive::Directive parsing ----

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

    // ---- reg_parse ----

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
        assert_eq!(parse_int_reg("fp"), Some(8));
        assert_eq!(parse_int_reg("s0"), Some(8));
    }

    #[test]
    fn unknown_reg_returns_none() {
        assert_eq!(parse_int_reg("notareg"), None);
        assert_eq!(parse_float_reg(""), None);
    }

    // ---- parser::parse (Pass 0) ----

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
        let tok = parse_one("\t.asciz \"hello\"");
        assert!(
            matches!(&tok, AsmToken::DataAsciz(s) if s == "hello"),
            "unexpected token: {tok:?}"
        );
    }

    // ---- layout::compute_layout (Pass 1) ----

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
            AsmToken::Align(2),
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
            AsmToken::Balign(8),
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

    // ---- encode::encode (Pass 2) ----

    fn text_section_tokens_with_forward_branch() -> Vec<AsmToken> {
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

        assert_eq!(output.symbol_address("end").unwrap(), 12);
        let text = output.text_bytes();
        let word = u32::from_le_bytes(text[4..8].try_into().unwrap());
        assert_eq!(word & 0x7F, 0x63, "wrong opcode for branch");
        assert_eq!((word >> 12) & 0x7, 1, "wrong funct3, expected BNE");
    }

    #[test]
    fn encode_backward_branch_resolves() {
        let tokens = vec![
            AsmToken::Section(SectionKind::Text),
            AsmToken::Label("top".to_owned()),
            nop_token(),
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
        assert_eq!(word & 0x7F, 0x63, "wrong opcode for branch");
        assert_eq!((word >> 12) & 0x7, 0, "wrong funct3, expected BEQ");
    }

    #[test]
    fn encode_jal_resolves() {
        let tokens = vec![
            AsmToken::Section(SectionKind::Text),
            AsmToken::Jal {
                rd: 1,
                target: "target".to_owned(),
            },
            nop_token(),
            nop_token(),
            AsmToken::Label("target".to_owned()),
            nop_token(),
        ];
        let layout = compute_layout(&tokens).unwrap();
        let output = encode(&tokens, &layout).unwrap();

        let text = output.text_bytes();
        let word = u32::from_le_bytes(text[0..4].try_into().unwrap());
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
        assert_eq!(output.rodata_bytes(), b"hi\0");
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
        assert!(output.is_symbol_global("main"), "main should be exported");
    }

    #[test]
    fn elf_export_uses_absolute_entry_point() {
        let tokens = vec![
            RvInstruction::Directive(".text".to_owned()),
            RvInstruction::Directive(".globl _start".to_owned()),
            RvInstruction::Directive(".space 64".to_owned()),
            RvInstruction::Label("_start".to_owned()),
            RvInstruction::Real(RealInstruction::Addi(Addi::new(0, 0, 0))),
        ];
        let output = Assembler::assemble(&tokens).expect("assembly should succeed");
        let elf = output.to_elf(0x8000_0000);

        assert_eq!(&elf[..4], b"\x7fELF");
        assert_eq!(elf[4], 2, "expected ELFCLASS64");
        assert_eq!(elf[5], 1, "expected little-endian ELF");
        assert_eq!(
            u16::from_le_bytes(elf[16..18].try_into().unwrap()),
            2,
            "expected ET_EXEC"
        );
        assert_eq!(
            u16::from_le_bytes(elf[18..20].try_into().unwrap()),
            243,
            "expected EM_RISCV"
        );
        assert_eq!(
            u64::from_le_bytes(elf[24..32].try_into().unwrap()),
            0x8000_0040
        );
    }
}
