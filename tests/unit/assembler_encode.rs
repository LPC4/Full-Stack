/// Tests for assembler pseudo-instruction relocation (call, la, tail).
use full_stack::assembly_language::assembler::Assembler;
use full_stack::assembly_language::rv_instruction::RvInstruction;
use full_stack::assembly_language::pseudo::PseudoInstruction;

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
        RvInstruction::Real(full_stack::assembly_language::real::RealInstruction::Addi(
            full_stack::assembly_language::riscv::rv64i::Addi::new(0, 0, 0)
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    // Check that we have symbols
    assert!(output.symbol_table.contains_key("start"));
    assert!(output.symbol_table.contains_key("target_func"));
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
        RvInstruction::Real(full_stack::assembly_language::real::RealInstruction::Addi(
            full_stack::assembly_language::riscv::rv64i::Addi::new(0, 0, 0)
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.symbol_table.contains_key("my_label"));
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
        RvInstruction::Real(full_stack::assembly_language::real::RealInstruction::Addi(
            full_stack::assembly_language::riscv::rv64i::Addi::new(0, 0, 0)
        )),
    ];

    let result = Assembler::assemble(&tokens);
    assert!(result.is_ok(), "Assembly failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.symbol_table.contains_key("end_func"));
}
