//! Static ELF-64 inspector: decodes headers, sections, symbols, and relocations
//! into plain data. Little-endian RV64 only, matching the in-repo toolchain.

const EM_RISCV: u16 = 243;
const SHT_SYMTAB: u32 = 2;
const SHT_RELA: u32 = 4;
pub const SHT_NOBITS: u32 = 8;
const SHN_UNDEF: u16 = 0;
const SHN_ABS: u16 = 0xfff1;
const SYM_ENTSIZE: usize = 24;
const RELA_ENTSIZE: usize = 24;
const SHDR_SIZE: usize = 64;

pub struct ElfInfo {
    pub file_len: usize,
    pub type_name: String,
    pub is_exec: bool,
    pub machine_name: String,
    pub entry: u64,
    pub segments: Vec<ElfSegment>,
    pub sections: Vec<ElfSection>,
    pub symbols: Vec<ElfSymbol>,
    pub relocations: Vec<ElfReloc>,
}

pub struct ElfSegment {
    pub type_name: String,
    /// "rwx"-style permission string.
    pub flags: String,
    pub offset: u64,
    pub vaddr: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub align: u64,
}

pub struct ElfSection {
    pub name: String,
    pub sh_type: u32,
    pub type_name: String,
    /// "WAX" subset of the section flags.
    pub flags: String,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
}

pub struct ElfSymbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub global: bool,
    /// Name of the section the symbol lives in ("UND"/"ABS" for special ones).
    pub section: String,
}

pub struct ElfReloc {
    /// The section the relocation patches (e.g. ".text").
    pub section: String,
    pub offset: u64,
    pub type_name: String,
    pub symbol: String,
    pub addend: i64,
}

/// Decode an ELF-64 image. Errors carry a human-readable reason so viewers can
/// fall back to a raw hex dump.
pub fn parse_elf(bytes: &[u8]) -> Result<ElfInfo, String> {
    if bytes.len() < 64 {
        return Err("file is smaller than an ELF-64 header".to_owned());
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err("ELF magic bytes are missing".to_owned());
    }
    if bytes[4] != 2 {
        return Err("not a 64-bit ELF image".to_owned());
    }
    if bytes[5] != 1 {
        return Err("not a little-endian ELF image".to_owned());
    }

    let e_type = u16_at(bytes, 16)?;
    let machine = u16_at(bytes, 18)?;
    let entry = u64_at(bytes, 24)?;
    let phoff = u64_at(bytes, 32)? as usize;
    let shoff = u64_at(bytes, 40)? as usize;
    let phentsize = u16_at(bytes, 54)? as usize;
    let phnum = u16_at(bytes, 56)? as usize;
    let shentsize = u16_at(bytes, 58)? as usize;
    let shnum = u16_at(bytes, 60)? as usize;
    let shstrndx = u16_at(bytes, 62)? as usize;

    let segments = parse_segments(bytes, phoff, phentsize, phnum)?;
    let sections = parse_sections(bytes, shoff, shentsize, shnum, shstrndx)?;
    let symbols = parse_symbols(bytes, shoff, shentsize, &sections)?;
    let relocations = parse_relocations(bytes, shoff, shentsize, &sections, &symbols)?;

    Ok(ElfInfo {
        file_len: bytes.len(),
        type_name: type_name(e_type),
        is_exec: e_type == 2,
        machine_name: if machine == EM_RISCV {
            "RISC-V".to_owned()
        } else {
            format!("machine {machine}")
        },
        entry,
        segments,
        sections,
        symbols,
        relocations,
    })
}

fn type_name(e_type: u16) -> String {
    match e_type {
        1 => "REL".to_owned(),
        2 => "EXEC".to_owned(),
        3 => "DYN".to_owned(),
        4 => "CORE".to_owned(),
        other => format!("type {other}"),
    }
}

fn parse_segments(
    bytes: &[u8],
    phoff: usize,
    phentsize: usize,
    phnum: usize,
) -> Result<Vec<ElfSegment>, String> {
    if phnum == 0 {
        return Ok(Vec::new());
    }
    if phentsize < 56 {
        return Err("program header entries are too small".to_owned());
    }
    let mut segments = Vec::with_capacity(phnum);
    for i in 0..phnum {
        let base = phoff
            .checked_add(i * phentsize)
            .ok_or("program header offset overflow")?;
        let p_type = u32_at(bytes, base)?;
        let p_flags = u32_at(bytes, base + 4)?;
        segments.push(ElfSegment {
            type_name: segment_type_name(p_type),
            flags: segment_flags(p_flags),
            offset: u64_at(bytes, base + 8)?,
            vaddr: u64_at(bytes, base + 16)?,
            file_size: u64_at(bytes, base + 32)?,
            mem_size: u64_at(bytes, base + 40)?,
            align: u64_at(bytes, base + 48)?,
        });
    }
    Ok(segments)
}

fn segment_type_name(p_type: u32) -> String {
    match p_type {
        1 => "LOAD".to_owned(),
        6 => "PHDR".to_owned(),
        other => format!("type {other}"),
    }
}

fn segment_flags(p_flags: u32) -> String {
    let mut out = String::with_capacity(3);
    out.push(if p_flags & 4 != 0 { 'r' } else { '-' });
    out.push(if p_flags & 2 != 0 { 'w' } else { '-' });
    out.push(if p_flags & 1 != 0 { 'x' } else { '-' });
    out
}

fn parse_sections(
    bytes: &[u8],
    shoff: usize,
    shentsize: usize,
    shnum: usize,
    shstrndx: usize,
) -> Result<Vec<ElfSection>, String> {
    if shnum == 0 {
        return Ok(Vec::new());
    }
    if shentsize < SHDR_SIZE {
        return Err("section header entries are too small".to_owned());
    }
    let shstr = section_body(bytes, shoff, shentsize, shstrndx)?;
    let mut sections = Vec::with_capacity(shnum);
    for i in 0..shnum {
        let base = shoff
            .checked_add(i * shentsize)
            .ok_or("section header offset overflow")?;
        let name_off = u32_at(bytes, base)? as usize;
        let sh_type = u32_at(bytes, base + 4)?;
        let sh_flags = u64_at(bytes, base + 8)?;
        sections.push(ElfSection {
            name: cstr_at(shstr, name_off),
            sh_type,
            type_name: section_type_name(sh_type),
            flags: section_flags(sh_flags),
            addr: u64_at(bytes, base + 16)?,
            offset: u64_at(bytes, base + 24)?,
            size: u64_at(bytes, base + 32)?,
        });
    }
    Ok(sections)
}

fn section_type_name(sh_type: u32) -> String {
    match sh_type {
        0 => "NULL".to_owned(),
        1 => "PROGBITS".to_owned(),
        SHT_SYMTAB => "SYMTAB".to_owned(),
        3 => "STRTAB".to_owned(),
        SHT_RELA => "RELA".to_owned(),
        SHT_NOBITS => "NOBITS".to_owned(),
        other => format!("type {other}"),
    }
}

fn section_flags(sh_flags: u64) -> String {
    let mut out = String::new();
    if sh_flags & 0x1 != 0 {
        out.push('W');
    }
    if sh_flags & 0x2 != 0 {
        out.push('A');
    }
    if sh_flags & 0x4 != 0 {
        out.push('X');
    }
    out
}

/// The file bytes of section `index`, located via the raw header table.
fn section_body(
    bytes: &[u8],
    shoff: usize,
    shentsize: usize,
    index: usize,
) -> Result<&[u8], String> {
    let base = shoff
        .checked_add(index * shentsize)
        .ok_or("section header offset overflow")?;
    let offset = u64_at(bytes, base + 24)? as usize;
    let size = u64_at(bytes, base + 32)? as usize;
    let end = offset.checked_add(size).ok_or("section body overflow")?;
    bytes
        .get(offset..end)
        .ok_or_else(|| "section body is outside the file".to_owned())
}

fn parse_symbols(
    bytes: &[u8],
    shoff: usize,
    shentsize: usize,
    sections: &[ElfSection],
) -> Result<Vec<ElfSymbol>, String> {
    let Some(symtab_index) = sections.iter().position(|s| s.sh_type == SHT_SYMTAB) else {
        return Ok(Vec::new());
    };
    let base = shoff
        .checked_add(symtab_index * shentsize)
        .ok_or("section header offset overflow")?;
    let strtab_index = u32_at(bytes, base + 40)? as usize;
    let strtab = section_body(bytes, shoff, shentsize, strtab_index)?;
    let symtab = section_body(bytes, shoff, shentsize, symtab_index)?;

    let mut symbols = Vec::new();
    // Entry 0 is the mandatory null symbol; skip it.
    for entry in symtab.chunks_exact(SYM_ENTSIZE).skip(1) {
        let name_off = u32_at(entry, 0)? as usize;
        let info = entry[4];
        let shndx = u16_at(entry, 6)?;
        symbols.push(ElfSymbol {
            name: cstr_at(strtab, name_off),
            value: u64_at(entry, 8)?,
            size: u64_at(entry, 16)?,
            global: info >> 4 == 1,
            section: symbol_section(shndx, sections),
        });
    }
    Ok(symbols)
}

fn symbol_section(shndx: u16, sections: &[ElfSection]) -> String {
    match shndx {
        SHN_UNDEF => "UND".to_owned(),
        SHN_ABS => "ABS".to_owned(),
        index => sections
            .get(index as usize)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("section {index}")),
    }
}

fn parse_relocations(
    bytes: &[u8],
    shoff: usize,
    shentsize: usize,
    sections: &[ElfSection],
    symbols: &[ElfSymbol],
) -> Result<Vec<ElfReloc>, String> {
    let mut relocations = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if section.sh_type != SHT_RELA {
            continue;
        }
        let base = shoff
            .checked_add(index * shentsize)
            .ok_or("section header offset overflow")?;
        let target_index = u32_at(bytes, base + 44)? as usize;
        let target = sections
            .get(target_index)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("section {target_index}"));
        let body = section_body(bytes, shoff, shentsize, index)?;
        for entry in body.chunks_exact(RELA_ENTSIZE) {
            let r_info = u64_at(entry, 8)?;
            // Symbol table indices count the null entry that `symbols` skips.
            let sym_index = (r_info >> 32) as usize;
            let symbol = if sym_index == 0 {
                String::new()
            } else {
                symbols
                    .get(sym_index - 1)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("symbol {sym_index}"))
            };
            relocations.push(ElfReloc {
                section: target.clone(),
                offset: u64_at(entry, 0)?,
                type_name: reloc_type_name((r_info & 0xffff_ffff) as u32),
                symbol,
                addend: u64_at(entry, 16)?.cast_signed(),
            });
        }
    }
    Ok(relocations)
}

fn reloc_type_name(r_type: u32) -> String {
    match r_type {
        1 => "R_RISCV_32".to_owned(),
        2 => "R_RISCV_64".to_owned(),
        17 => "R_RISCV_JAL".to_owned(),
        19 => "R_RISCV_CALL_PLT".to_owned(),
        23 => "R_RISCV_PCREL_HI20".to_owned(),
        24 => "R_RISCV_PCREL_LO12_I".to_owned(),
        other => format!("R_RISCV({other})"),
    }
}

/// NUL-terminated string at `offset` in a string table; lossy on bad bytes.
fn cstr_at(strtab: &[u8], offset: usize) -> String {
    let Some(tail) = strtab.get(offset..) else {
        return String::new();
    };
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    String::from_utf8_lossy(&tail[..end]).into_owned()
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "read past end of file".to_owned())
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "read past end of file".to_owned())
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    bytes
        .get(offset..offset + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| "read past end of file".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use asm_to_binary::assembler::Assembler;
    use asm_to_binary::pseudo::PseudoInstruction;
    use asm_to_binary::real::RealInstruction;
    use asm_to_binary::riscv::rv64i::{Ecall, Jalr};
    use asm_to_binary::rv_instruction::RvInstruction;

    /// A minimal assembled program: _start calls main, main returns 42.
    fn minimal_output() -> asm_to_binary::AssembledOutput {
        let tokens = vec![
            RvInstruction::Directive(".text".to_owned()),
            RvInstruction::Label("_start".to_owned()),
            RvInstruction::Pseudo(PseudoInstruction::Call {
                symbol: "main".to_owned(),
            }),
            RvInstruction::Pseudo(PseudoInstruction::Li { rd: 17, imm: 93 }),
            RvInstruction::Real(RealInstruction::Ecall(Ecall)),
            RvInstruction::Label("main".to_owned()),
            RvInstruction::Pseudo(PseudoInstruction::Li { rd: 10, imm: 42 }),
            RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))),
        ];
        Assembler::assemble(&tokens).expect("assembly failed")
    }

    #[test]
    fn parses_executable_from_the_real_writer() {
        const LOAD_BASE: u64 = 0x8000_0000;
        let elf = minimal_output().to_elf(LOAD_BASE);
        let info = parse_elf(&elf).expect("parse");

        assert_eq!(info.type_name, "EXEC");
        assert!(info.is_exec);
        assert_eq!(info.machine_name, "RISC-V");
        assert!(info.entry >= LOAD_BASE);
        assert!(!info.segments.is_empty());
        assert!(info.sections.iter().any(|s| s.name == ".text"));
        assert!(info.symbols.iter().any(|s| s.name == "_start"));
        let text = info
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .expect("text section");
        assert!(text.size > 0);
        assert!((text.offset + text.size) as usize <= elf.len());
    }

    #[test]
    fn parses_object_from_the_real_writer() {
        let obj = minimal_output().to_object("demo.o");
        let info = parse_elf(&obj).expect("parse");

        assert_eq!(info.type_name, "REL");
        assert!(!info.is_exec);
        assert!(info.sections.iter().any(|s| s.name == ".symtab"));
        assert!(info.symbols.iter().any(|s| s.name == "main"));
    }

    #[test]
    fn rejects_junk_gracefully() {
        assert!(parse_elf(b"not an elf").is_err());
        assert!(parse_elf(&[0x7f, b'E', b'L', b'F']).is_err());
        let mut bad = b"\x7fELF".to_vec();
        bad.resize(64, 0);
        // Class byte says 32-bit.
        bad[4] = 1;
        assert!(parse_elf(&bad).is_err());
    }
}
