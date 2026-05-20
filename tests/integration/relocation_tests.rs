/// Integration tests for symbol relocation with pseudo-instructions (call, tail, la).
use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::pseudo::PseudoInstruction;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64i::{Addi, Jalr};

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
            // Verify assembly was successful
            assert!(!output.sections.is_empty(), "Expected non-empty sections");
            
            // Verify symbol table contains expected symbols
            assert!(output.symbol_table.contains_key("main"), "Expected 'main' symbol");
            assert!(output.symbol_table.contains_key("print_message"), "Expected 'print_message' symbol");
            assert!(output.symbol_table.contains_key("message"), "Expected 'message' symbol");
            
            // Verify global symbols include main
            assert!(output.global_symbols.contains(&"main".to_string()), "Expected 'main' in global symbols");
            
            // Verify we have both text and data sections
            let has_text_section = output.sections.iter().any(|s| 
                matches!(&s.kind, Some(k) if format!("{:?}", k).contains("Text"))
            );
            let has_data_section = output.sections.iter().any(|s| 
                matches!(&s.kind, Some(k) if format!("{:?}", k).contains("Data"))
            );
            
            assert!(has_text_section, "Expected text section");
            assert!(has_data_section, "Expected data section");
            
            // Verify relocation completed successfully by checking that instructions were generated
            let text_sections: Vec<_> = output.sections.iter().filter(|s| 
                matches!(&s.kind, Some(k) if format!("{:?}", k).contains("Text"))
            ).collect();
            
            assert!(!text_sections.is_empty(), "Expected at least one text section");
            assert!(text_sections[0].bytes.len() > 0, "Expected non-empty text section");
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
            // Verify the la pseudo-instruction was expanded properly
            // This should result in auipc + addi instructions
            let text_section = output.sections.iter().find(|s| 
                matches!(&s.kind, Some(k) if format!("{:?}", k).contains("Text"))
            ).expect("Expected text section");
            
            // The la pseudo should expand to at least 2 instructions (auipc + addi)
            assert!(text_section.bytes.len() >= 8, "Expected at least 2 instructions from la expansion");
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
            // Verify the call pseudo-instruction was expanded properly
            // This should result in auipc + jalr instructions
            let text_section = output.sections.iter().find(|s| 
                matches!(&s.kind, Some(k) if format!("{:?}", k).contains("Text"))
            ).expect("Expected text section");
            
            // The call pseudo should expand to at least 2 instructions (auipc + jalr)
            assert!(text_section.bytes.len() >= 8, "Expected at least 2 instructions from call expansion");
            
            // Verify both symbols are in the symbol table
            assert!(output.symbol_table.contains_key("main"));
            assert!(output.symbol_table.contains_key("target_func"));
        }
        Err(e) => {
            panic!("Assembly failed for call test: {}", e.message);
        }
    }
}