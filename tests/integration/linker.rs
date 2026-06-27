use asm_to_binary::{AssembledOutput, ObjectLinker};
use full_stack::compilation_pipeline::CompilationPipeline;
use virtual_machine::bus::ELF_LOAD_BASE;

fn sample_source() -> &'static str {
    r#"
helper: (n: i32) -> i32 {
    i: i32 = n
    while i > 0 {
        i = i - 1
    }
    return i
}

export main: () -> i32 {
    return helper(3)
}
"#
}

fn compile_object(source: &str) -> AssembledOutput {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let result = pipeline
        .compile(source)
        .unwrap_or_else(|e| panic!("failed to compile sample source: {e}"));
    let (_asm, tokens) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    pipeline
        .assemble(&tokens)
        .unwrap_or_else(|e| panic!("failed to assemble sample source: {e}"))
}

fn compile_sample() -> (AssembledOutput, Vec<u8>) {
    let assembled = compile_object(sample_source());
    let elf = assembled.to_elf(ELF_LOAD_BASE);
    (assembled, elf)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn read_c_string(bytes: &[u8], offset: u32) -> String {
    let start = offset as usize;
    let end = bytes[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|len| start + len)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

#[derive(Debug)]
struct SectionHeader {
    name: String,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_entsize: u64,
}

#[derive(Debug)]
struct SymbolEntry {
    name: String,
    bind: u8,
    section_index: u16,
    value: u64,
}

fn parse_sections(elf: &[u8]) -> Vec<SectionHeader> {
    let shoff = read_u64(elf, 40) as usize;
    let shentsize = read_u16(elf, 58) as usize;
    let shnum = read_u16(elf, 60) as usize;
    let shstrndx = read_u16(elf, 62) as usize;

    let mut raw = Vec::with_capacity(shnum);
    for index in 0..shnum {
        let base = shoff + index * shentsize;
        raw.push((
            index,
            read_u32(elf, base),
            read_u64(elf, base + 16),
            read_u64(elf, base + 24),
            read_u64(elf, base + 32),
            read_u64(elf, base + 56),
        ));
    }

    let shstrtab = &elf[raw[shstrndx].3 as usize..(raw[shstrndx].3 + raw[shstrndx].4) as usize];

    raw.into_iter()
        .map(|(_index, name_off, sh_addr, sh_offset, sh_size, sh_entsize)| SectionHeader {
            name: read_c_string(shstrtab, name_off),
            sh_addr,
            sh_offset,
            sh_size,
            sh_entsize,
        })
        .collect()
}

fn parse_symbols(elf: &[u8], sections: &[SectionHeader]) -> Vec<SymbolEntry> {
    let symtab = sections
        .iter()
        .find(|section| section.name == ".symtab")
        .unwrap_or_else(|| panic!("missing .symtab in exported ELF"));
    let strtab = sections
        .iter()
        .find(|section| section.name == ".strtab")
        .unwrap_or_else(|| panic!("missing .strtab in exported ELF"));

    let symtab_bytes = &elf[symtab.sh_offset as usize..(symtab.sh_offset + symtab.sh_size) as usize];
    let strtab_bytes = &elf[strtab.sh_offset as usize..(strtab.sh_offset + strtab.sh_size) as usize];
    let entsize = symtab.sh_entsize as usize;

    symtab_bytes
        .chunks_exact(entsize)
        .map(|entry| SymbolEntry {
            name: read_c_string(strtab_bytes, read_u32(entry, 0)),
            bind: entry[4] >> 4,
            section_index: read_u16(entry, 6),
            value: read_u64(entry, 8),
        })
        .collect()
}

fn section_index_for_addr(sections: &[SectionHeader], addr: u64) -> usize {
    sections
        .iter()
        .enumerate()
        .find(|(_, section)| {
            if section.sh_size == 0 {
                section.sh_addr == addr
            } else {
                addr >= section.sh_addr && addr < section.sh_addr + section.sh_size
            }
        })
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("no section contains address {addr:#x}"))
}

fn find_local_label(assembled: &AssembledOutput) -> String {
    assembled
        .symbols_iter()
        .map(|(name, _)| name)
        .find(|name| !assembled.is_symbol_global(name) && name.contains("__"))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("expected at least one local control-flow label"))
}

#[test]
fn elf_program_headers_keep_page_alignment() {
    let (_assembled, elf) = compile_sample();
    let phoff = read_u64(&elf, 32) as usize;
    let phentsize = read_u16(&elf, 54) as usize;
    let phnum = read_u16(&elf, 56) as usize;

    assert!(phnum > 0, "expected at least one PT_LOAD segment");

    for i in 0..phnum {
        let base = phoff + i * phentsize;
        let p_type = read_u32(&elf, base);
        if p_type != 1 {
            continue;
        }
        let p_offset = read_u64(&elf, base + 8);
        let p_vaddr = read_u64(&elf, base + 16);
        let p_filesz = read_u64(&elf, base + 32);
        let p_memsz = read_u64(&elf, base + 40);

        assert_eq!(p_offset % 0x1000, p_vaddr % 0x1000, "PT_LOAD segment {i} is not page-aligned");
        assert!(p_filesz <= p_memsz, "PT_LOAD segment {i} has filesz > memsz");
    }
}

#[test]
fn elf_symbol_table_marks_globals_and_locals() {
    let (assembled, elf) = compile_sample();
    let sections = parse_sections(&elf);
    let symbols = parse_symbols(&elf, &sections);

    let text_index = sections
        .iter()
        .position(|section| section.name == ".text")
        .expect("missing .text section in ELF");

    let main = symbols
        .iter()
        .find(|symbol| symbol.name == "main")
        .expect("missing main symbol in ELF symbol table");
    assert_eq!(main.bind, 1, "main should be exported as a global symbol");
    assert_eq!(main.section_index as usize, text_index, "main should live in .text");

    let local_name = find_local_label(&assembled);
    let local = symbols
        .iter()
        .find(|symbol| symbol.name == local_name)
        .unwrap_or_else(|| panic!("missing local label `{local_name}` in ELF symbol table"));
    assert_eq!(local.bind, 0, "local labels should remain local in the ELF symbol table");
    assert_eq!(
        local.section_index as usize,
        section_index_for_addr(&sections, local.value),
        "local label `{local_name}` should reference the section that contains its address"
    );
}

#[test]
fn hll_exports_control_object_global_visibility() {
    let assembled = compile_object(
        r#"
secret: () -> i32 { return 41 }
private_data: i64 = 1

export api: () -> i32 {
    return secret()
}

export shared: i64 = 7
"#,
    );

    assert!(assembled.has_symbol("secret"), "private function label should exist");
    assert!(
        !assembled.is_symbol_global("secret"),
        "unexported function should stay local"
    );
    assert!(
        !assembled.is_symbol_global("private_data"),
        "unexported global should stay local"
    );
    assert!(assembled.is_symbol_global("api"), "exported function should be global");
    assert!(
        assembled.is_symbol_global("shared"),
        "exported global should be global"
    );
}

#[test]
fn object_linker_does_not_resolve_external_to_private_symbol() {
    let defining = compile_object(
        r#"
hidden: () -> i32 { return 9 }
"#,
    );
    let referencing = compile_object(
        r#"
external hidden: () -> i32

main: () -> i32 {
    return hidden()
}
"#,
    );

    let err = ObjectLinker::link(&[("defining", &defining), ("referencing", &referencing)])
        .expect_err("private symbol should not satisfy an external relocation");
    assert!(
        err.message.contains("undefined external symbol `hidden`"),
        "unexpected linker error: {err}"
    );
}

