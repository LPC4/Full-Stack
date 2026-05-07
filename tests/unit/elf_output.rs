/// Unit tests for AssembledOutput::to_elf.
///
/// Checks ELF structural properties that are required for QEMU / Linux to
/// accept and load the binary correctly.
use full_stack::assembly_language::assembler::Assembler;
use full_stack::assembly_language::pseudo::PseudoInstruction;
use full_stack::assembly_language::real::RealInstruction;
use full_stack::assembly_language::riscv::rv64i::Ecall;
use full_stack::assembly_language::rv_instruction::RvInstruction;

const PAGE_SIZE: u64 = 0x1000;
const LOAD_BASE: u64 = 0x8000_0000;

// ELF-64 field offsets in the header
const E_ENTRY_OFF: usize = 24;
const E_PHOFF_OFF: usize = 32;
const E_PHNUM_OFF: usize = 56;

// Program-header field offsets (each phdr = 56 bytes)
const PHDR_SIZE: usize = 56;
const P_TYPE_OFF: usize = 0;
const P_OFFSET_OFF: usize = 8;
const P_VADDR_OFF: usize = 16;
const P_ALIGN_OFF: usize = 48;

fn u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}
fn u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}
fn u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

/// Build a minimal assembled program: _start calls main, main returns 42.
fn minimal_output() -> full_stack::assembly_language::assembler::output::AssembledOutput {
    let a7: u8 = 17;
    let a0: u8 = 10;
    let ra: u8 = 1;

    let tokens = vec![
        RvInstruction::Directive(".text".to_owned()),
        RvInstruction::Label("_start".to_owned()),
        RvInstruction::Pseudo(PseudoInstruction::Call {
            symbol: "main".to_owned(),
        }),
        RvInstruction::Pseudo(PseudoInstruction::Li { rd: a7, imm: 93 }),
        RvInstruction::Real(RealInstruction::Ecall(Ecall)),
        RvInstruction::Label("main".to_owned()),
        RvInstruction::Pseudo(PseudoInstruction::Li { rd: a0, imm: 42 }),
        RvInstruction::Real(RealInstruction::Jalr(
            full_stack::assembly_language::riscv::rv64i::Jalr::new(0, ra, 0),
        )),
    ];

    Assembler::assemble(&tokens).expect("assembly failed")
}

// ---- ELF magic and class ---------------------------------------------------

#[test]
fn elf_starts_with_magic() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    assert_eq!(&elf[0..4], b"\x7fELF", "ELF magic mismatch");
}

#[test]
fn elf_class_is_64bit() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    assert_eq!(elf[4], 2, "EI_CLASS should be ELFCLASS64 (2)");
}

#[test]
fn elf_data_is_little_endian() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    assert_eq!(elf[5], 1, "EI_DATA should be ELFDATA2LSB (1)");
}

#[test]
fn elf_machine_is_riscv() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    let em = u16_le(&elf, 18);
    assert_eq!(em, 243, "e_machine should be EM_RISCV (243)");
}

// ---- Entry point -----------------------------------------------------------

#[test]
fn elf_entry_resolves_start_symbol() {
    let out = minimal_output();
    let start_off = out.symbol_table["_start"];
    let elf = out.to_elf(LOAD_BASE);
    let entry = u64_le(&elf, E_ENTRY_OFF);
    assert_eq!(entry, LOAD_BASE + start_off, "e_entry should be load_base + _start offset");
}

// ---- PT_LOAD alignment constraint -----------------------------------------

#[test]
fn elf_program_headers_present() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    let phnum = u16_le(&elf, E_PHNUM_OFF);
    assert!(phnum >= 1, "at least one PT_LOAD program header required");
}

#[test]
fn elf_section_data_page_aligned_in_file() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    let phoff = u64_le(&elf, E_PHOFF_OFF) as usize;
    let phnum = u16_le(&elf, E_PHNUM_OFF) as usize;

    // Find the first PT_LOAD (type=1)
    for i in 0..phnum {
        let base = phoff + i * PHDR_SIZE;
        let p_type = u32_le(&elf, base + P_TYPE_OFF);
        if p_type == 1 {
            let p_offset = u64_le(&elf, base + P_OFFSET_OFF);
            assert_eq!(
                p_offset % PAGE_SIZE,
                0,
                "p_offset of first PT_LOAD must be page-aligned; got {p_offset:#x}"
            );
            return;
        }
    }
    panic!("no PT_LOAD segment found");
}

#[test]
fn elf_pt_load_alignment_field_is_page_size() {
    let elf = minimal_output().to_elf(LOAD_BASE);
    let phoff = u64_le(&elf, E_PHOFF_OFF) as usize;
    let phnum = u16_le(&elf, E_PHNUM_OFF) as usize;

    for i in 0..phnum {
        let base = phoff + i * PHDR_SIZE;
        let p_type = u32_le(&elf, base + P_TYPE_OFF);
        if p_type == 1 {
            let p_align = u64_le(&elf, base + P_ALIGN_OFF);
            assert_eq!(
                p_align, PAGE_SIZE,
                "p_align should be PAGE_SIZE ({PAGE_SIZE:#x}), got {p_align:#x}"
            );
            return;
        }
    }
    panic!("no PT_LOAD segment found");
}

#[test]
fn elf_vaddr_offset_congruence() {
    // ELF spec: p_vaddr ≡ p_offset (mod p_align) for every PT_LOAD
    let elf = minimal_output().to_elf(LOAD_BASE);
    let phoff = u64_le(&elf, E_PHOFF_OFF) as usize;
    let phnum = u16_le(&elf, E_PHNUM_OFF) as usize;

    for i in 0..phnum {
        let base = phoff + i * PHDR_SIZE;
        let p_type = u32_le(&elf, base + P_TYPE_OFF);
        if p_type == 1 {
            let p_offset = u64_le(&elf, base + P_OFFSET_OFF);
            let p_vaddr = u64_le(&elf, base + P_VADDR_OFF);
            let p_align = u64_le(&elf, base + P_ALIGN_OFF);
            assert_eq!(
                p_vaddr % p_align,
                p_offset % p_align,
                "PT_LOAD[{i}]: p_vaddr ({p_vaddr:#x}) ≢ p_offset ({p_offset:#x}) mod p_align ({p_align:#x})"
            );
        }
    }
}

// ---- Assembled output helpers ----------------------------------------------

#[test]
fn assembled_output_text_bytes_not_empty() {
    let out = minimal_output();
    assert!(!out.text_bytes().is_empty());
}

#[test]
fn assembled_output_symbol_table_has_start_and_main() {
    let out = minimal_output();
    assert!(out.symbol_table.contains_key("_start"));
    assert!(out.symbol_table.contains_key("main"));
}

#[test]
fn assembled_output_total_bytes_equals_sum() {
    let out = minimal_output();
    let sum: usize = out.sections.iter().map(|s| s.bytes.len()).sum();
    assert_eq!(out.total_bytes(), sum);
}

#[test]
fn elf_non_overlapping_sections() {
    // With page alignment, section data must not overlap the ELF+phdr area.
    let elf = minimal_output().to_elf(LOAD_BASE);
    let phoff = u64_le(&elf, E_PHOFF_OFF) as usize;
    let phnum = u16_le(&elf, E_PHNUM_OFF) as usize;
    let ehdr_plus_phdrs = phoff + phnum * PHDR_SIZE;

    for i in 0..phnum {
        let base = phoff + i * PHDR_SIZE;
        let p_type = u32_le(&elf, base + P_TYPE_OFF);
        if p_type == 1 {
            let p_offset = u64_le(&elf, base + P_OFFSET_OFF) as usize;
            assert!(
                p_offset >= ehdr_plus_phdrs,
                "PT_LOAD[{i}] p_offset ({p_offset:#x}) overlaps ELF headers (end={ehdr_plus_phdrs:#x})"
            );
        }
    }
}
