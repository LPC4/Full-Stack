use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;

fn u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().expect("u16 slice"))
}

#[test]
fn unresolved_call_emits_relocation_and_object_is_et_rel() {
    let tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl _start".to_owned()),
        RvInstruction::Label("_start".to_owned()),
        RvInstruction::Directive("\tcall puts".to_owned()),
    ];

    let out = Assembler::assemble(&tokens).expect("assembly should succeed with unresolved call");
    assert_eq!(out.relocation_count(), 1, "expected one CALL relocation");

    let obj = out.to_object("unresolved_call.o");
    assert_eq!(&obj[0..4], b"\x7fELF", "object should be an ELF file");
    assert_eq!(u16_le(&obj, 16), 1, "e_type should be ET_REL");
    assert!(
        obj.windows(".rela.text\0".len())
            .any(|w| w == b".rela.text\0"),
        "expected .rela.text section in object"
    );
}
