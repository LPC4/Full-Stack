use asm_to_binary::assembler::Assembler;
use asm_to_binary::object_linker::ObjectLinker;
use asm_to_binary::rv_instruction::RvInstruction;

fn word_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().expect("4-byte word"))
}

fn sign_extend_12(v: u32) -> i64 {
    let raw = (v & 0x0FFF) as i32;
    if raw & 0x800 != 0 {
        (raw - 0x1000) as i64
    } else {
        raw as i64
    }
}

fn word_at_linked_address(assembled: &asm_to_binary::AssembledOutput, addr: u64) -> u32 {
    let mut base = 0u64;
    for section in assembled.sections_iter() {
        let end = base + section.bytes.len() as u64;
        if addr >= base && addr + 4 <= end {
            let off = (addr - base) as usize;
            return word_at(section.bytes, off);
        }
        base = end;
    }
    panic!("no linked section contains address {addr:#x}");
}

#[test]
fn links_two_objects_and_resolves_call_relocation() {
    let user_tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl main".to_owned()),
        RvInstruction::Label("main".to_owned()),
        RvInstruction::Directive("\tcall puts".to_owned()),
        RvInstruction::Directive("\tret".to_owned()),
    ];
    let stdlib_tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl puts".to_owned()),
        RvInstruction::Label("puts".to_owned()),
        RvInstruction::Directive("\tret".to_owned()),
    ];

    let user_obj = Assembler::assemble(&user_tokens).expect("assemble user object");
    let stdlib_obj = Assembler::assemble(&stdlib_tokens).expect("assemble stdlib object");

    assert_eq!(user_obj.relocation_count(), 1, "user object should contain unresolved call relocation");

    let linked = ObjectLinker::link(&[("stdlib", &stdlib_obj), ("user", &user_obj)])
        .expect("link should resolve puts from stdlib");

    assert_eq!(linked.relocation_count(), 0, "linked output should be fully resolved");
    assert!(linked.has_symbol("main"));
    assert!(linked.has_symbol("puts"));

    let main = linked.symbol_address("main").expect("main symbol");
    let puts = linked.symbol_address("puts").expect("puts symbol");
    let auipc = word_at_linked_address(&linked, main);
    let jalr = word_at_linked_address(&linked, main + 4);

    let hi = (auipc & 0xFFFF_F000) as i32 as i64;
    let lo = sign_extend_12(jalr >> 20);
    let resolved_target = (main as i64) + hi + lo;
    assert_eq!(
        resolved_target,
        puts as i64,
        "linked call pair should resolve to `puts`"
    );
}

#[test]
fn linking_fails_for_undefined_external_symbol() {
    let tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl main".to_owned()),
        RvInstruction::Label("main".to_owned()),
        RvInstruction::Directive("\tcall missing_symbol".to_owned()),
    ];

    let obj = Assembler::assemble(&tokens).expect("assemble object with unresolved external");
    let err = ObjectLinker::link(&[("user", &obj)]).expect_err("link should fail on undefined symbol");
    assert!(
        err.to_string().contains("missing_symbol"),
        "error should mention unresolved symbol name"
    );
}

#[test]
fn links_unresolved_jal_between_objects() {
    let user_tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl main".to_owned()),
        RvInstruction::Label("main".to_owned()),
        RvInstruction::Directive("\tj target_ext".to_owned()),
    ];
    let lib_tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Directive(".globl target_ext".to_owned()),
        RvInstruction::Label("target_ext".to_owned()),
        RvInstruction::Directive("\tret".to_owned()),
    ];

    let user_obj = Assembler::assemble(&user_tokens).expect("assemble user object");
    let lib_obj = Assembler::assemble(&lib_tokens).expect("assemble library object");

    let linked = ObjectLinker::link(&[("lib", &lib_obj), ("user", &user_obj)]).expect("link objects");

    let main = linked.symbol_address("main").expect("main symbol") as usize;
    let target = linked.symbol_address("target_ext").expect("target symbol") as i64;
    let jal = word_at_linked_address(&linked, main as u64);
    let imm20 = (((jal >> 31) & 0x1) << 20) as i32;
    let imm19_12 = (((jal >> 12) & 0xFF) << 12) as i32;
    let imm11 = (((jal >> 20) & 0x1) << 11) as i32;
    let imm10_1 = (((jal >> 21) & 0x3FF) << 1) as i32;
    let mut offset = imm20 | imm19_12 | imm11 | imm10_1;
    if (offset & (1 << 20)) != 0 {
        offset -= 1 << 21;
    }
    let resolved = (main as i64) + (offset as i64);
    assert_eq!(resolved, target, "JAL relocation should resolve to external target");
}



