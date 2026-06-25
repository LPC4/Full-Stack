/// Integration tests for symbol relocation with pseudo-instructions.
use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::pseudo::PseudoInstruction;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64i::{Addi, Jalr};
#[expect(unused_imports, reason = "kept with the relocation fixture imports for readability")]
use asm_to_binary::AssembledOutput;

#[test]
fn test_relocation_with_call_tail_and_la_pseudos() {
    // Create a simple program that uses all three pseudo-instructions
    let tokens = vec![
        // .text section
        RvInstruction::Directive("\t.text".to_string()),
        
        // Main entry point
        RvInstruction::Label("main".to_string()),
        RvInstruction::Directive("\t.globl main".to_string()),
        
        // Load address of message using la pseudo
        RvInstruction::Pseudo(PseudoInstruction::La {
            rd: 10, // a0 - first argument register
            symbol: "message".to_string(),
        }),
        
        // Call print function using call pseudo
        RvInstruction::Pseudo(PseudoInstruction::Call {
            symbol: "print_message".to_string(),
        }),
        
        // Return 0
        RvInstruction::Real(RealInstruction::Addi(Addi::new(10, 0, 0))), // a0 = 0
        RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))), // ret (jalr x0, ra)
        
        // Print message function
        RvInstruction::Label("print_message".to_string()),
        // Just return for this test (in real code would do I/O)
        RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))), // ret
        
        // Data section with message
        RvInstruction::Directive("\t.data".to_string()),
        RvInstruction::Label("message".to_string()),
        RvInstruction::Directive("\t.asciz \"Hello from relocated symbols!\"".to_string()),
    ];

    match Assembler::assemble(&tokens) {
        Ok(output) => {
            assert!(output.has_sections(), "Expected non-empty sections");

            assert!(output.has_symbol("main"), "Expected 'main' symbol");
            assert!(output.has_symbol("print_message"), "Expected 'print_message' symbol");
            assert!(output.has_symbol("message"), "Expected 'message' symbol");

            assert!(output.is_symbol_global("main"), "Expected 'main' in global symbols");

            assert!(!output.text_bytes().is_empty(), "Expected text section");
            assert!(!output.data_bytes().is_empty(), "Expected data section");

            assert!(!output.text_bytes().is_empty(), "Expected non-empty text section");
        }
        Err(e) => {
            panic!("Assembly failed: {}", e.message);
        }
    }
}

#[test]
fn test_la_pseudo_expansion() {
    // Test specifically the 'la' pseudo-instruction expansion
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Label("main".to_string()),
        RvInstruction::Directive("\t.globl main".to_string()),
        
        // Load address using la pseudo
        RvInstruction::Pseudo(PseudoInstruction::La {
            rd: 10, // a0
            symbol: "data_label".to_string(),
        }),
        
        RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))), // ret
        
        RvInstruction::Directive("\t.data".to_string()),
        RvInstruction::Label("data_label".to_string()),
        RvInstruction::Directive("\t.word 42".to_string()),
    ];

    match Assembler::assemble(&tokens) {
        Ok(output) => {
            // la expands to auipc + addi = at least 2 instructions
            assert!(output.text_bytes().len() >= 8, "Expected at least 2 instructions from la expansion");
        }
        Err(e) => {
            panic!("Assembly failed for la test: {}", e.message);
        }
    }
}

#[test]
fn test_call_pseudo_expansion() {
    // Test specifically the 'call' pseudo-instruction expansion
    let tokens = vec![
        RvInstruction::Directive("\t.text".to_string()),
        RvInstruction::Label("main".to_string()),
        RvInstruction::Directive("\t.globl main".to_string()),
        
        // Call another function using call pseudo
        RvInstruction::Pseudo(PseudoInstruction::Call {
            symbol: "target_func".to_string(),
        }),
        
        RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))), // ret
        
        RvInstruction::Label("target_func".to_string()),
        RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))), // ret
    ];

    match Assembler::assemble(&tokens) {
        Ok(output) => {
            // call expands to auipc + jalr = at least 2 instructions
            assert!(output.text_bytes().len() >= 8, "Expected at least 2 instructions from call expansion");
            assert!(output.has_symbol("main"));
            assert!(output.has_symbol("target_func"));
        }
        Err(e) => {
            panic!("Assembly failed for call test: {}", e.message);
        }
    }
}
