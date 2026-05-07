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
    /// Layout: ELF header · program headers · section data · section headers · strtab · symtab
    ///
    /// The entry point is resolved from the symbol table; `_start` is preferred
    /// over `main`, which falls back to the load base address.
    pub fn to_elf(&self, load_base: u64) -> Vec<u8> {
        use elf64::*;

        // ---- Collect sections with their load addresses ----
        struct ElfSec<'a> {
            sec: &'a super::section::SectionData,
            load_addr: u64,
            sh_type: u32,
            sh_flags: u64,
        }

        let mut elf_secs: Vec<ElfSec<'_>> = Vec::new();
        let mut running_addr = load_base;
        for sec in &self.sections {
            if let Some(kind) = &sec.kind {
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

        // ---- Build _start trampoline if only main is present ----
        // qemu-riscv64 (Linux user-mode) jumps to the ELF entry point.  If the
        // entry is main, returning from it crashes because ra is 0 at startup.
        // We append a 12-byte _start stub that calls main then does exit_group.
        //
        // Encoding:
        //   jal  ra, <main_addr - stub_addr>   ; PC-relative call to main
        //   addi a7, x0, 94                    ; exit_group syscall number
        //   ecall                              ; terminate with a0 as exit code
        let start_stub: Option<(Vec<u8>, u64)> = if !self.symbol_table.contains_key("_start") {
            if let Some(&main_off) = self.symbol_table.get("main") {
                let stub_addr = running_addr; // placed after all existing sections
                let main_addr = load_base + main_off;
                let offset = (main_addr as i64) - (stub_addr as i64);
                // JAL ra, offset  (rd=1, opcode=0x6f)
                let off = offset as u32;
                let imm20 = (off >> 20) & 1;
                let imm10_1 = (off >> 1) & 0x3ff;
                let imm11 = (off >> 11) & 1;
                let imm19_12 = (off >> 12) & 0xff;
                let jal = (imm20 << 31)
                    | (imm10_1 << 21)
                    | (imm11 << 20)
                    | (imm19_12 << 12)
                    | (1 << 7)
                    | 0x6f_u32;
                // ADDI a7, x0, 94  (exit_group)
                let addi: u32 = (94 << 20) | (17 << 7) | 0x13;
                // ECALL
                let ecall: u32 = 0x0000_0073;
                let mut stub = Vec::with_capacity(12);
                stub.extend_from_slice(&jal.to_le_bytes());
                stub.extend_from_slice(&addi.to_le_bytes());
                stub.extend_from_slice(&ecall.to_le_bytes());
                Some((stub, stub_addr))
            } else {
                None
            }
        } else {
            None
        };

        // ---- Determine entry point ----
        let entry = if let Some(&sym_addr) = self.symbol_table.get("_start") {
            load_base + sym_addr
        } else if let Some((_, stub_addr)) = &start_stub {
            *stub_addr
        } else if let Some(&sym_addr) = self.symbol_table.get("main") {
            load_base + sym_addr
        } else {
            elf_secs.first().map(|s| s.load_addr).unwrap_or(load_base)
        };

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
            let st_type = STT_NOTYPE; // could detect FUNC from .text range
            let st_info = (st_bind << 4) | (st_type & 0xf);

            push_u32_le(&mut sym_entries, name_off); // st_name
            sym_entries.push(st_info); // st_info
            sym_entries.push(0); // st_other
            push_u16_le(&mut sym_entries, 1); // st_shndx = section 1 (approx)
            push_u64_le(&mut sym_entries, load_base + addr); // st_value
            push_u64_le(&mut sym_entries, 0); // st_size
        }

        // ---- Compute file layout ----
        let n_prog_secs = elf_secs.len();
        // The _start stub gets its own PT_LOAD but no section header.
        let n_phdrs = n_prog_secs + if start_stub.is_some() { 1 } else { 0 };
        let n_shdrs = 1 // SHT_NULL
            + n_prog_secs
            + 3; // .shstrtab, .strtab, .symtab

        let ehdr_size = ELF64_HDR_SIZE as u64;
        let phdrs_size = (ELF64_PHDR_SIZE as u64) * (n_phdrs as u64);
        let mut sec_file_offsets: Vec<u64> = Vec::new();
        let mut file_offset = ehdr_size + phdrs_size;

        for es in &elf_secs {
            sec_file_offsets.push(file_offset);
            if !matches!(es.sec.kind, Some(SectionKind::Bss)) {
                file_offset += es.sec.bytes.len() as u64;
            }
        }

        // Reserve space for the stub (if present) after all other section data.
        let stub_file_offset = file_offset;
        if let Some((ref stub_bytes, _)) = start_stub {
            file_offset += stub_bytes.len() as u64;
        }

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
        buf.push(EV_CURRENT as u8); // EI_VERSION
        buf.push(0); // ELFOSABI_NONE
        buf.extend_from_slice(&[0u8; 8]); // EI_ABIVERSION + padding

        push_u16_le(&mut buf, ET_EXEC);
        push_u16_le(&mut buf, EM_RISCV);
        push_u32_le(&mut buf, EV_CURRENT);
        push_u64_le(&mut buf, entry); // e_entry
        push_u64_le(&mut buf, ehdr_size); // e_phoff
        push_u64_le(&mut buf, shdrs_offset); // e_shoff
        push_u32_le(&mut buf, 0x0005); // e_flags: RV64 soft-float ABI
        push_u16_le(&mut buf, ELF64_HDR_SIZE);
        push_u16_le(&mut buf, ELF64_PHDR_SIZE);
        push_u16_le(&mut buf, n_phdrs as u16);
        push_u16_le(&mut buf, ELF64_SHDR_SIZE);
        push_u16_le(&mut buf, n_shdrs as u16);
        push_u16_le(&mut buf, (1 + n_prog_secs) as u16); // e_shstrndx = index of .shstrtab

        // ---- Emit program headers (one PT_LOAD per section) ----
        for (i, es) in elf_secs.iter().enumerate() {
            let flags = if es.sec.kind.as_ref().map_or(false, |k| k.is_executable()) {
                PF_R | PF_X
            } else if matches!(
                es.sec.kind,
                Some(SectionKind::Data) | Some(SectionKind::Bss)
            ) {
                PF_R | PF_W
            } else {
                PF_R
            };
            let filesz = if matches!(es.sec.kind, Some(SectionKind::Bss)) {
                0u64
            } else {
                es.sec.bytes.len() as u64
            };
            push_u32_le(&mut buf, PT_LOAD);
            push_u32_le(&mut buf, flags);
            push_u64_le(&mut buf, sec_file_offsets[i]); // p_offset
            push_u64_le(&mut buf, es.load_addr); // p_vaddr
            push_u64_le(&mut buf, es.load_addr); // p_paddr
            push_u64_le(&mut buf, filesz); // p_filesz
            push_u64_le(&mut buf, es.sec.bytes.len() as u64); // p_memsz
            // p_align = 1: no alignment constraint.  Segments are placed at
            // file offsets that are NOT page-aligned (ehdr+phdrs precede them),
            // so p_align = 0x1000 would violate p_vaddr ≡ p_offset (mod align).
            push_u64_le(&mut buf, 1);
        }

        // Program header for the _start stub (executable, no section header).
        if let Some((ref stub_bytes, stub_vaddr)) = start_stub {
            push_u32_le(&mut buf, PT_LOAD);
            push_u32_le(&mut buf, PF_R | PF_X);
            push_u64_le(&mut buf, stub_file_offset);
            push_u64_le(&mut buf, stub_vaddr);
            push_u64_le(&mut buf, stub_vaddr);
            push_u64_le(&mut buf, stub_bytes.len() as u64);
            push_u64_le(&mut buf, stub_bytes.len() as u64);
            push_u64_le(&mut buf, 1);
        }

        // ---- Emit section data ----
        for (i, es) in elf_secs.iter().enumerate() {
            debug_assert_eq!(buf.len() as u64, sec_file_offsets[i]);
            if !matches!(es.sec.kind, Some(SectionKind::Bss)) {
                buf.extend_from_slice(&es.sec.bytes);
            }
        }

        // Emit _start stub bytes (if present).
        debug_assert_eq!(buf.len() as u64, stub_file_offset);
        if let Some((ref stub_bytes, _)) = start_stub {
            buf.extend_from_slice(stub_bytes);
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
        push_u64_le(&mut buf, 0); // sh_flags
        push_u64_le(&mut buf, 0); // sh_addr
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
