use super::link_layout::LinkLayout;
use super::section::{SectionData, SectionKind};
use std::collections::HashMap;

/// The final output produced by the assembler -- one byte blob per section,
/// plus a complete symbol table ready to hand to a linker or ELF writer.
#[derive(Debug, Default, Clone)]
pub struct AssembledOutput {
    /// Sections in emission order, keyed by kind.
    pub sections: Vec<SectionData>,
    /// All resolved labels: name -> absolute address within the output blob.
    pub symbol_table: HashMap<String, u64>,
    /// Names marked `.globl` (exported).
    pub global_symbols: Vec<String>,
}

impl AssembledOutput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a section by kind, returning its byte content (or an empty slice).
    pub fn section_bytes(&self, kind: &SectionKind) -> &[u8] {
        self.sections
            .iter()
            .find(|s| s.kind.as_ref() == Some(kind))
            .map(|s| s.bytes.as_slice())
            .unwrap_or(&[])
    }

    pub fn text_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::Text)
    }

    pub fn data_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::Data)
    }

    pub fn rodata_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::RoData)
    }

    pub fn bss_bytes(&self) -> &[u8] {
        self.section_bytes(&SectionKind::Bss)
    }

    /// Total encoded size across all sections.
    pub fn total_bytes(&self) -> usize {
        self.sections.iter().map(|s| s.bytes.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// ELF-64 generation (RISC-V little-endian)
// ---------------------------------------------------------------------------

/// ELF-64 constants for RISC-V.
mod elf64 {
    pub const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
    pub const ELFCLASS64: u8 = 2;
    pub const ELFDATA2LSB: u8 = 1; // little-endian
    pub const ET_EXEC: u16 = 2;
    pub const EM_RISCV: u16 = 243;
    pub const EV_CURRENT: u32 = 1;
    pub const PT_LOAD: u32 = 1;
    pub const PF_X: u32 = 1;
    pub const PF_W: u32 = 2;
    pub const PF_R: u32 = 4;
    pub const SHT_PROGBITS: u32 = 1;
    pub const SHT_SYMTAB: u32 = 2;
    pub const SHT_STRTAB: u32 = 3;
    pub const SHT_NOBITS: u32 = 8;
    pub const SHF_ALLOC: u64 = 2;
    pub const SHF_EXECINSTR: u64 = 4;
    pub const SHF_WRITE: u64 = 1;
    pub const STB_LOCAL: u8 = 0;
    pub const STB_GLOBAL: u8 = 1;
    pub const STT_NOTYPE: u8 = 0;
    pub const SHN_ABS: u16 = 0xFFF1;
    pub const ELF64_HDR_SIZE: u16 = 64;
    pub const ELF64_PHDR_SIZE: u16 = 56;
    pub const ELF64_SHDR_SIZE: u16 = 64;
    pub const ELF64_SYM_SIZE: usize = 24;
}

fn push_u16_le(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn push_u32_le(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn push_u64_le(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

impl AssembledOutput {
    /// Produce a minimal ELF-64 executable for RV64IMAFD (little-endian).
    ///
    /// Layout: ELF header · 1 program header · section data · section headers · strtab · symtab
    ///
    /// A single combined PT_LOAD segment covers all sections.  This avoids the
    /// QEMU page-permission conflict that arises when .text (R X) and .bss (R W)
    /// share the same 4 KB page: with one PT_LOAD there is no conflicting mapping.
    /// Since all code uses PC-relative addressing, the single-segment layout is
    /// identical to what the assembler compiled against.
    ///
    /// The entry point is resolved from the symbol table; `_start` is preferred
    /// over `main`, which falls back to the load base address.
    pub fn to_elf(&self, load_base: u64) -> Vec<u8> {
        self.to_elf_with_candidates(load_base, &["_start", "main"])
    }

    /// Like `to_elf` but tries `entry_symbol` first before the default fallbacks.
    /// Use this for freestanding builds whose entry point is not `_start`.
    pub fn to_elf_with_entry(&self, load_base: u64, entry_symbol: &str) -> Vec<u8> {
        self.to_elf_with_candidates(load_base, &[entry_symbol, "_start", "main"])
    }

    fn to_elf_with_candidates(&self, load_base: u64, candidates: &[&str]) -> Vec<u8> {
        use elf64::*;

        // ---- Collect sections with their load addresses ----
        struct ElfSec<'a> {
            sec: &'a SectionData,
            load_addr: u64,
            sh_type: u32,
            sh_flags: u64,
        }

        let mut elf_secs: Vec<ElfSec<'_>> = Vec::new();
        let mut running_addr = load_base;
        // BSS is excluded from the ELF file (zero-filled by the loader);
        // placing it last prevents it from displacing rodata in the virtual address space.
        for pass_bss in [false, true] {
            for sec in &self.sections {
                if let Some(kind) = &sec.kind {
                    if matches!(kind, SectionKind::Bss) != pass_bss {
                        continue;
                    }
                    let sh_type = if matches!(kind, SectionKind::Bss) {
                        SHT_NOBITS
                    } else {
                        SHT_PROGBITS
                    };
                    let sh_flags = if kind.is_executable() {
                        SHF_ALLOC | SHF_EXECINSTR
                    } else if matches!(kind, SectionKind::Data | SectionKind::Bss) {
                        SHF_ALLOC | SHF_WRITE
                    } else {
                        SHF_ALLOC
                    };
                    elf_secs.push(ElfSec {
                        sec,
                        load_addr: running_addr,
                        sh_type,
                        sh_flags,
                    });
                    running_addr += sec.bytes.len() as u64;
                }
            }
        }
        // running_addr is now load_base + total virtual memory size (including BSS)

        // ---- Determine entry point ----
        let entry = candidates
            .iter()
            .find_map(|sym| self.symbol_table.get(*sym).map(|&a| load_base + a))
            .unwrap_or_else(|| elf_secs.first().map(|s| s.load_addr).unwrap_or(load_base));

        // ---- Build section-name string table (.shstrtab) ----
        let mut shstrtab: Vec<u8> = vec![0]; // index 0 = empty string
        let mut shstrtab_indices: Vec<u32> = Vec::new();
        for es in &elf_secs {
            shstrtab_indices.push(shstrtab.len() as u32);
            shstrtab.extend_from_slice(
                es.sec
                    .kind
                    .as_ref()
                    .map(|k| k.name())
                    .unwrap_or("")
                    .as_bytes(),
            );
            shstrtab.push(0);
        }
        // names for .shstrtab, .strtab, .symtab sections
        let shstrtab_name_idx = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");
        let strtab_name_idx = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".strtab\0");
        let symtab_name_idx = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".symtab\0");

        // ---- Build symbol string table (.strtab) and .symtab entries ----
        let mut strtab: Vec<u8> = vec![0]; // STN_UNDEF name = ""
        let mut sym_entries: Vec<u8> = Vec::new();

        let section_index_for_addr = |addr: u64| -> Option<u16> {
            for (i, es) in elf_secs.iter().enumerate() {
                let start = es.load_addr;
                let end = start + es.sec.bytes.len() as u64;
                if es.sec.bytes.is_empty() {
                    if addr == start {
                        return Some((1 + i) as u16);
                    }
                } else if addr >= start && addr < end {
                    return Some((1 + i) as u16);
                }
            }
            None
        };

        // Null symbol first (required by ELF spec)
        sym_entries.extend_from_slice(&[0u8; ELF64_SYM_SIZE]);

        let mut sorted_syms: Vec<(&String, &u64)> = self.symbol_table.iter().collect();
        sorted_syms.sort_by_key(|&(_, &addr)| addr);

        for &(ref name, &addr) in &sorted_syms {
            let name_off = strtab.len() as u32;
            strtab.extend_from_slice(name.as_bytes());
            strtab.push(0);

            let is_global = self.global_symbols.contains(name);
            let st_bind = if is_global { STB_GLOBAL } else { STB_LOCAL };
            let st_type = STT_NOTYPE;
            let st_info = (st_bind << 4) | (st_type & 0xf);
            let shndx = section_index_for_addr(load_base + addr).unwrap_or(SHN_ABS);

            push_u32_le(&mut sym_entries, name_off); // st_name
            sym_entries.push(st_info); // st_info
            sym_entries.push(0); // st_other
            push_u16_le(&mut sym_entries, shndx); // st_shndx
            push_u64_le(&mut sym_entries, load_base + addr); // st_value
            push_u64_le(&mut sym_entries, 0); // st_size
        }

        // ---- Compute file layout ----
        // One PT_LOAD covering everything: no per-section program headers.
        let n_phdrs = 1usize;
        let n_prog_secs = elf_secs.len();
        let n_shdrs = 1 // SHT_NULL
            + n_prog_secs
            + 3; // .shstrtab, .strtab, .symtab

        let ehdr_size = ELF64_HDR_SIZE as u64;
        let phdrs_size = (ELF64_PHDR_SIZE as u64) * (n_phdrs as u64);

        // Page-align the start of section data so p_vaddr ≡ p_offset (mod PAGE_SIZE)
        // holds for any PAGE_SIZE-aligned load_base.
        const PAGE_SIZE: u64 = 0x1000;
        let header_end = ehdr_size + phdrs_size;
        let sec_data_start = (header_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let header_padding = sec_data_start - header_end;

        // Compute individual section file offsets (BSS occupies no file space).
        let mut sec_file_offsets: Vec<u64> = Vec::new();
        let mut file_offset = sec_data_start;
        for es in &elf_secs {
            sec_file_offsets.push(file_offset);
            if !matches!(es.sec.kind, Some(SectionKind::Bss)) {
                file_offset += es.sec.bytes.len() as u64;
            }
        }
        let sec_data_filesz = file_offset - sec_data_start; // bytes actually in file
        let sec_data_memsz = running_addr - load_base; // full virtual footprint incl. BSS

        let shstrtab_offset = file_offset;
        file_offset += shstrtab.len() as u64;
        let strtab_offset = file_offset;
        file_offset += strtab.len() as u64;
        let symtab_offset = file_offset;
        file_offset += sym_entries.len() as u64;
        let shdrs_offset = file_offset;

        // ---- Emit ELF header ----
        let mut buf: Vec<u8> = Vec::new();

        // e_ident[16]
        buf.extend_from_slice(&ELFMAG);
        buf.push(ELFCLASS64);
        buf.push(ELFDATA2LSB);
        buf.push(EV_CURRENT as u8);
        buf.push(0); // ELFOSABI_NONE
        buf.extend_from_slice(&[0u8; 8]);

        push_u16_le(&mut buf, ET_EXEC);
        push_u16_le(&mut buf, EM_RISCV);
        push_u32_le(&mut buf, EV_CURRENT);
        push_u64_le(&mut buf, entry); // e_entry
        push_u64_le(&mut buf, ehdr_size); // e_phoff
        push_u64_le(&mut buf, shdrs_offset); // e_shoff
        push_u32_le(&mut buf, 0x0005); // e_flags: RVC, double-float ABI
        push_u16_le(&mut buf, ELF64_HDR_SIZE);
        push_u16_le(&mut buf, ELF64_PHDR_SIZE);
        push_u16_le(&mut buf, n_phdrs as u16);
        push_u16_le(&mut buf, ELF64_SHDR_SIZE);
        push_u16_le(&mut buf, n_shdrs as u16);
        push_u16_le(&mut buf, (1 + n_prog_secs) as u16); // e_shstrndx

        // ---- Emit a single combined PT_LOAD ----
        // R|W|X covers all sections.  The single segment avoids any page-level
        // permission conflict between .text (R X) and .bss (R W) when they share
        // a page.  QEMU user-mode accepts RWX and will zero-fill the BSS region
        // (the [filesz, memsz) range of the segment).
        push_u32_le(&mut buf, PT_LOAD);
        push_u32_le(&mut buf, PF_R | PF_W | PF_X);
        push_u64_le(&mut buf, sec_data_start); // p_offset
        push_u64_le(&mut buf, load_base); // p_vaddr
        push_u64_le(&mut buf, load_base); // p_paddr
        push_u64_le(&mut buf, sec_data_filesz); // p_filesz (excludes BSS)
        push_u64_le(&mut buf, sec_data_memsz); // p_memsz  (includes BSS)
        push_u64_le(&mut buf, PAGE_SIZE); // p_align

        // Pad from end of program header to page-aligned section data start.
        debug_assert_eq!(buf.len() as u64, header_end);
        buf.extend(std::iter::repeat(0u8).take(header_padding as usize));

        // ---- Emit section data ----
        for (i, es) in elf_secs.iter().enumerate() {
            debug_assert_eq!(buf.len() as u64, sec_file_offsets[i]);
            if !matches!(es.sec.kind, Some(SectionKind::Bss)) {
                buf.extend_from_slice(&es.sec.bytes);
            }
        }

        // ---- Emit .shstrtab, .strtab, .symtab ----
        debug_assert_eq!(buf.len() as u64, shstrtab_offset);
        buf.extend_from_slice(&shstrtab);
        debug_assert_eq!(buf.len() as u64, strtab_offset);
        buf.extend_from_slice(&strtab);
        debug_assert_eq!(buf.len() as u64, symtab_offset);
        buf.extend_from_slice(&sym_entries);

        // ---- Emit section headers ----
        debug_assert_eq!(buf.len() as u64, shdrs_offset);

        // SHT_NULL
        buf.extend_from_slice(&[0u8; 64]);

        // One header per program section
        for (i, es) in elf_secs.iter().enumerate() {
            let filesz = if matches!(es.sec.kind, Some(SectionKind::Bss)) {
                0u64
            } else {
                es.sec.bytes.len() as u64
            };
            push_u32_le(&mut buf, shstrtab_indices[i]); // sh_name
            push_u32_le(&mut buf, es.sh_type); // sh_type
            push_u64_le(&mut buf, es.sh_flags); // sh_flags
            push_u64_le(&mut buf, es.load_addr); // sh_addr
            push_u64_le(&mut buf, sec_file_offsets[i]); // sh_offset
            push_u64_le(&mut buf, filesz); // sh_size
            push_u32_le(&mut buf, 0); // sh_link
            push_u32_le(&mut buf, 0); // sh_info
            push_u64_le(&mut buf, 4); // sh_addralign
            push_u64_le(&mut buf, 0); // sh_entsize
        }

        // .shstrtab header
        push_u32_le(&mut buf, shstrtab_name_idx);
        push_u32_le(&mut buf, SHT_STRTAB);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, shstrtab_offset);
        push_u64_le(&mut buf, shstrtab.len() as u64);
        push_u32_le(&mut buf, 0);
        push_u32_le(&mut buf, 0);
        push_u64_le(&mut buf, 1);
        push_u64_le(&mut buf, 0);

        // .strtab header
        push_u32_le(&mut buf, strtab_name_idx);
        push_u32_le(&mut buf, SHT_STRTAB);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, strtab_offset);
        push_u64_le(&mut buf, strtab.len() as u64);
        push_u32_le(&mut buf, 0);
        push_u32_le(&mut buf, 0);
        push_u64_le(&mut buf, 1);
        push_u64_le(&mut buf, 0);

        // .symtab header
        let strtab_shndx = 1 + n_prog_secs + 1; // index of .strtab
        let n_local_syms = sorted_syms
            .iter()
            .filter(|(n, _)| !self.global_symbols.contains(n))
            .count()
            + 1; // +1 for STN_UNDEF
        push_u32_le(&mut buf, symtab_name_idx);
        push_u32_le(&mut buf, SHT_SYMTAB);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, 0);
        push_u64_le(&mut buf, symtab_offset);
        push_u64_le(&mut buf, sym_entries.len() as u64);
        push_u32_le(&mut buf, strtab_shndx as u32); // sh_link = strtab
        push_u32_le(&mut buf, n_local_syms as u32); // sh_info = first global index
        push_u64_le(&mut buf, 8);
        push_u64_le(&mut buf, ELF64_SYM_SIZE as u64);

        buf
    }

    // ---------------------------------------------------------------------------
    // Flat binary
    // ---------------------------------------------------------------------------

    /// Produce a raw flat binary: all sections packed in load order (non-BSS
    /// first, then BSS as zeros).  No ELF headers are included.  Suitable for
    /// bootloaders that copy the image directly into memory.
    pub fn to_flat_binary(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        for pass_bss in [false, true] {
            for sec in &self.sections {
                if let Some(kind) = &sec.kind {
                    if matches!(kind, SectionKind::Bss) != pass_bss {
                        continue;
                    }
                    buf.extend_from_slice(&sec.bytes);
                }
            }
        }
        buf
    }

    // ---------------------------------------------------------------------------
    // Layout symbol injection
    // ---------------------------------------------------------------------------

    /// Inject linker boundary symbols (`__text_start`, `__bss_end`, etc.) into
    /// the symbol table so kernel startup code can zero BSS, set the stack
    /// pointer, and initialise the heap without hard-coding addresses.
    ///
    /// Symbols are stored as section-relative offsets (load_base is added by
    /// the ELF writer).  If `layout.stack_top > 0` the value stored is
    /// `layout.stack_top - layout.load_base` so that after load-base adjustment
    /// the ELF symbol value equals `layout.stack_top`.
    ///
    /// Only called when `layout.emit_layout_symbols` is true.
    pub fn inject_layout_symbols(&mut self, layout: &LinkLayout) {
        if !layout.emit_layout_symbols {
            return;
        }

        // Walk sections in the same order as to_elf: non-BSS first, BSS last.
        let mut running: u64 = 0;
        for pass_bss in [false, true] {
            for sec in &self.sections {
                if let Some(kind) = &sec.kind {
                    if matches!(kind, SectionKind::Bss) != pass_bss {
                        continue;
                    }
                    let start = running;
                    let end = running + sec.bytes.len() as u64;

                    match kind {
                        SectionKind::Text => {
                            self.symbol_table
                                .entry("__text_start".to_owned())
                                .or_insert(start);
                            self.symbol_table.insert("__text_end".to_owned(), end);
                        }
                        SectionKind::RoData => {
                            self.symbol_table
                                .entry("__rodata_start".to_owned())
                                .or_insert(start);
                            self.symbol_table.insert("__rodata_end".to_owned(), end);
                        }
                        SectionKind::Data => {
                            self.symbol_table
                                .entry("__data_start".to_owned())
                                .or_insert(start);
                            self.symbol_table.insert("__data_end".to_owned(), end);
                        }
                        SectionKind::Bss => {
                            self.symbol_table
                                .entry("__bss_start".to_owned())
                                .or_insert(start);
                            self.symbol_table.insert("__bss_end".to_owned(), end);

                            // Heap starts immediately after BSS, aligned to 16 bytes.
                            let heap_start = (end + 15) & !15;
                            self.symbol_table
                                .insert("__heap_start".to_owned(), heap_start);
                            if layout.heap_size > 0 {
                                self.symbol_table.insert(
                                    "__heap_end".to_owned(),
                                    heap_start + layout.heap_size,
                                );
                            }
                        }
                        _ => {}
                    }
                    running = end;
                }
            }
        }

        // If no BSS section exists, place the heap after the last section.
        if !self.symbol_table.contains_key("__bss_start") {
            let heap_start = (running + 15) & !15;
            self.symbol_table
                .insert("__heap_start".to_owned(), heap_start);
            if layout.heap_size > 0 {
                self.symbol_table
                    .insert("__heap_end".to_owned(), heap_start + layout.heap_size);
            }
        }

        // Stack top: store as an offset from load_base so the ELF writer adds
        // load_base and the resulting symbol equals the intended virtual address.
        if layout.stack_top > 0 {
            let stack_offset = layout.stack_top.saturating_sub(layout.load_base);
            self.symbol_table
                .insert("__stack_top".to_owned(), stack_offset);
        }
    }

    // ---------------------------------------------------------------------------
    // Entry-point marking
    // ---------------------------------------------------------------------------

    /// Mark `entry_symbol` as a global export if it is present in the symbol
    /// table but not yet in `global_symbols`.  This ensures the ELF symtab
    /// advertises the kernel entry so debuggers and QEMU can find it.
    pub fn mark_entry_global(&mut self, entry_symbol: &str) {
        if self.symbol_table.contains_key(entry_symbol)
            && !self.global_symbols.contains(&entry_symbol.to_owned())
        {
            self.global_symbols.push(entry_symbol.to_owned());
        }
    }
}

impl std::fmt::Display for AssembledOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for section in &self.sections {
            if let Some(kind) = &section.kind {
                writeln!(f, "{} ({} bytes):", kind.name(), section.bytes.len())?;
                for chunk in section.bytes.chunks(16) {
                    write!(f, "  ")?;
                    for b in chunk {
                        write!(f, "{b:02x} ")?;
                    }
                    writeln!(f)?;
                }
            }
        }
        writeln!(f, "Symbols ({}):", self.symbol_table.len())?;
        let mut sorted: Vec<_> = self.symbol_table.iter().collect();
        sorted.sort_by_key(|&(_, addr)| addr);
        for (name, addr) in sorted {
            let marker = if self.global_symbols.contains(name) {
                " [global]"
            } else {
                ""
            };
            writeln!(f, "  {addr:#010x}  {name}{marker}")?;
        }
        Ok(())
    }
}
