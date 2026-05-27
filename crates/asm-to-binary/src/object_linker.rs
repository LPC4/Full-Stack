use crate::assembler::output::{AssembledOutput, RelocationKind};
use crate::assembler::section::{SectionData, SectionKind};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LinkerError {
    pub message: String,
}

impl LinkerError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for LinkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "linker error: {}", self.message)
    }
}

impl std::error::Error for LinkerError {}

/// Links independently assembled objects into one fully linked `AssembledOutput`.
///
/// Sections of the same kind from different modules are merged into a single
/// output section.  This keeps the ELF output lean (one `.text`, one `.data`,
/// etc.) and ensures boundary symbols injected afterwards span the correct
/// merged ranges.
pub struct ObjectLinker;

impl ObjectLinker {
    pub fn link(modules: &[(&str, &AssembledOutput)]) -> Result<AssembledOutput, LinkerError> {
        // Collect contributions per section kind
        //
        // For each module we walk its sections in ELF load order (non-BSS
        // first, BSS last)
        struct Contrib<'a> {
            module_idx: usize,
            sec: &'a SectionData,
        }

        // The canonical output order: Text -> RoData -> Data -> Custom(...) -> Bss.
        let all_kinds = collect_ordered_kinds(modules);
        let mut kind_contribs: Vec<(&SectionKind, Vec<Contrib<'_>>)> =
            all_kinds.iter().map(|k| (k, Vec::new())).collect();

        for (mi, (_name, module)) in modules.iter().enumerate() {
            let ordered = ordered_sections(module);
            for sec in ordered {
                let Some(kind) = &sec.kind else {
                    continue;
                };
                let contrib = Contrib {
                    module_idx: mi,
                    sec,
                };
                for (k, contribs) in &mut kind_contribs {
                    if *k == kind {
                        contribs.push(contrib);
                        break;
                    }
                }
            }
        }

        // Merge bytes and compute output bases
        //
        // Within each kind, contributions are concatenated in module order.
        // `mod_start_in_output[kind][mi]` gives the byte offset within the
        // merged section where module `mi`'s contribution begins.
        struct MergedSec {
            kind: SectionKind,
            bytes: Vec<u8>,
            mod_starts: HashMap<usize, u64>,
        }

        let mut merged_secs: Vec<MergedSec> = Vec::new();
        // Map: (kind_name, module_idx) -> absolute output base
        let mut contrib_base: HashMap<(String, usize), u64> = HashMap::new();
        let mut output_bases_by_kind: HashMap<String, u64> = HashMap::new();
        let mut running_abs: u64 = 0;

        for (kind, contribs) in &kind_contribs {
            if contribs.is_empty() {
                continue;
            }
            let mut merged_bytes: Vec<u8> = Vec::new();
            let mut mod_starts: HashMap<usize, u64> = HashMap::new();
            let kind_name = kind.name().to_owned();

            output_bases_by_kind.insert(kind_name.clone(), running_abs);

            for c in contribs {
                let start_in_merged = merged_bytes.len() as u64;
                mod_starts.insert(c.module_idx, start_in_merged);
                contrib_base.insert(
                    (kind_name.clone(), c.module_idx),
                    running_abs + start_in_merged,
                );
                merged_bytes.extend_from_slice(&c.sec.bytes);
            }

            if !matches!(kind, SectionKind::Bss) {
                running_abs += merged_bytes.len() as u64;
            }

            merged_secs.push(MergedSec {
                kind: (*kind).clone(),
                bytes: merged_bytes,
                mod_starts,
            });
        }

        // BSS segments are placed AFTER all non-BSS in the address space.
        let mut bss_running = running_abs;
        for ms in &mut merged_secs {
            if matches!(ms.kind, SectionKind::Bss) {
                let kind_name = ms.kind.name().to_owned();
                output_bases_by_kind.insert(kind_name.clone(), bss_running);
                for (mi, start) in &ms.mod_starts {
                    contrib_base.insert((kind_name.clone(), *mi), bss_running + start);
                }
                bss_running += ms.bytes.len() as u64;
            }
        }

        // Build the global symbol table
        let mut all_symbols: HashMap<String, u64> = HashMap::new();
        let mut defined_globals: HashMap<String, u64> = HashMap::new();
        let mut global_names: Vec<String> = Vec::new();
        // Non-global (local) symbols are keyed by (module_idx, name) so that
        // identically-named locals from different object files don't collide.
        let mut local_symbol_addrs: HashMap<(usize, String), u64> = HashMap::new();

        for (mi, (module_name, module)) in modules.iter().enumerate() {
            let mod_sec_map = build_module_section_map(module);

            for (name, &module_addr) in &module.symbol_table {
                let Some((sec_name, sec_start)) =
                    find_symbol_section_name_via_map(&mod_sec_map, module_addr)
                else {
                    continue;
                };

                let Some(&abs_base) = contrib_base.get(&(sec_name.to_owned(), mi)) else {
                    continue;
                };
                let out_addr = abs_base + (module_addr.saturating_sub(sec_start));

                if module.global_symbols.contains(name) {
                    all_symbols.entry(name.clone()).or_insert(out_addr);

                    if let Some(prev) = defined_globals.get(name) {
                        if *prev != out_addr {
                            return Err(LinkerError::new(format!(
                                "duplicate global symbol `{name}` while linking module `{module_name}`"
                            )));
                        }
                    } else {
                        defined_globals.insert(name.clone(), out_addr);
                        global_names.push(name.clone());
                    }
                } else {
                    // Local symbol: store per-module to avoid cross-module collisions.
                    local_symbol_addrs.insert((mi, name.clone()), out_addr);
                }
            }
        }

        // Apply relocations
        for (mi, (module_name, module)) in modules.iter().enumerate() {
            let mod_sec_map = build_module_section_map(module);

            for reloc in module.relocations_iter() {
                // The relocation offset is module-relative. Find which section
                // contains it in the module's address space.
                let Some((sec_name, sec_start)) =
                    find_symbol_section_name_via_map(&mod_sec_map, reloc.offset)
                else {
                    return Err(LinkerError::new(format!(
                        "relocation at offset {} in module `{module_name}` is outside any known section",
                        reloc.offset
                    )));
                };

                let Some(&abs_base) = contrib_base.get(&(sec_name.to_owned(), mi)) else {
                    return Err(LinkerError::new(format!(
                        "relocation section `{sec_name}` from module `{module_name}` is missing in output",
                    )));
                };

                // Look up local symbols first (per-module), then fall back to globals.
                let target_addr = local_symbol_addrs
                    .get(&(mi, reloc.symbol.clone()))
                    .copied()
                    .or_else(|| all_symbols.get(&reloc.symbol).copied())
                    .ok_or_else(|| {
                        LinkerError::new(format!(
                            "undefined external symbol `{}` referenced by module `{module_name}`",
                            reloc.symbol
                        ))
                    })?;

                let site_in_contribution = reloc.offset.saturating_sub(sec_start);
                let site_abs = abs_base + site_in_contribution;

                let merged_sec = merged_secs
                    .iter_mut()
                    .find(|ms| ms.kind.name() == sec_name)
                    .ok_or_else(|| {
                        LinkerError::new(format!(
                            "merged section `{sec_name}` not found while applying relocation from `{module_name}`",
                        ))
                    })?;

                let site_in_merged =
                    merged_sec.mod_starts.get(&mi).copied().unwrap_or(0) + site_in_contribution;

                match reloc.kind {
                    RelocationKind::CallPlt => {
                        patch_call_pair(
                            &mut merged_sec.bytes,
                            site_in_merged as usize,
                            site_abs,
                            target_addr,
                            reloc.addend,
                        )?;
                    }
                    RelocationKind::Jal => {
                        patch_jal(
                            &mut merged_sec.bytes,
                            site_in_merged as usize,
                            site_abs,
                            target_addr,
                            reloc.addend,
                        )?;
                    }
                    RelocationKind::La => {
                        patch_la(
                            &mut merged_sec.bytes,
                            site_in_merged as usize,
                            site_abs,
                            target_addr,
                            reloc.addend,
                        )?;
                    }
                }
            }
        }

        // Assemble the final output
        let sections: Vec<SectionData> = merged_secs
            .into_iter()
            .map(|ms| SectionData {
                kind: Some(ms.kind),
                bytes: ms.bytes,
                symbols: Vec::new(),
                globals: Vec::new(),
            })
            .collect();

        Ok(AssembledOutput {
            sections,
            symbol_table: all_symbols,
            global_symbols: global_names,
            relocations: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect all distinct section kinds present across all modules, in canonical
/// ELF load order: Text, RoData, Data, Custom(k), Bss.
fn collect_ordered_kinds(modules: &[(&str, &AssembledOutput)]) -> Vec<SectionKind> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    let preferred = [SectionKind::Text, SectionKind::RoData, SectionKind::Data];

    for kind in &preferred {
        for (_name, module) in modules {
            for sec in &module.sections {
                if sec.kind.as_ref() == Some(kind) && seen.insert(kind.clone()) {
                    out.push(kind.clone());
                }
            }
        }
    }

    // Custom sections in their first-appearance order
    for (_name, module) in modules {
        for sec in &module.sections {
            if let Some(kind) = &sec.kind {
                if !preferred.contains(kind) && !matches!(kind, SectionKind::Bss) {
                    if seen.insert(kind.clone()) {
                        out.push(kind.clone());
                    }
                }
            }
        }
    }

    // BSS always last
    if modules.iter().any(|(_, m)| {
        m.sections
            .iter()
            .any(|s| matches!(&s.kind, Some(SectionKind::Bss)))
    }) {
        out.push(SectionKind::Bss);
    }

    out
}

/// Return the module's sections in ELF load order: non-BSS first, BSS last.
fn ordered_sections(module: &AssembledOutput) -> Vec<&SectionData> {
    let mut out = Vec::new();
    for pass_bss in [false, true] {
        for sec in &module.sections {
            let Some(kind) = &sec.kind else {
                continue;
            };
            if matches!(kind, SectionKind::Bss) == pass_bss {
                out.push(sec);
            }
        }
    }
    out
}

/// Pre-compute a section map for a module: [(name, start_addr, end_addr)] in
/// the same order as `ordered_sections`.
fn build_module_section_map(module: &AssembledOutput) -> Vec<(&str, u64, u64)> {
    let mut map = Vec::new();
    let mut running = 0u64;
    for sec in ordered_sections(module) {
        let Some(kind) = &sec.kind else {
            continue;
        };
        let start = running;
        let end = start + sec.bytes.len() as u64;
        map.push((kind.name(), start, end));
        running = end;
    }
    map
}

/// Find which section (name, start) contains `addr` in the module's address space.
fn find_symbol_section_name_via_map<'a>(
    map: &'a [(&str, u64, u64)],
    addr: u64,
) -> Option<(&'a str, u64)> {
    for &(name, start, end) in map {
        let in_range = if start == end {
            addr == start
        } else {
            addr >= start && addr < end
        };
        if in_range {
            return Some((name, start));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// PC-relative splitting
// ---------------------------------------------------------------------------

fn pcrel_split(offset: i64) -> (i32, i32) {
    let lo12 = ((offset & 0xFFF) as i32).wrapping_sub(if offset & 0x800 != 0 { 0x1000 } else { 0 });
    let hi20 = ((offset - lo12 as i64) >> 12) as i32;
    (hi20, lo12)
}

fn read_u32_at(buf: &[u8], off: usize) -> Result<u32, LinkerError> {
    let end = off + 4;
    let Some(bytes) = buf.get(off..end) else {
        return Err(LinkerError::new(format!(
            "relocation read at offset {off} exceeds section size {}",
            buf.len()
        )));
    };
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| LinkerError::new("invalid word slice while reading relocation site"))?;
    Ok(u32::from_le_bytes(arr))
}

fn write_u32_at(buf: &mut [u8], off: usize, word: u32) -> Result<(), LinkerError> {
    let end = off + 4;
    let Some(dst) = buf.get_mut(off..end) else {
        return Err(LinkerError::new(format!(
            "relocation write at offset {off} exceeds section size {}",
            buf.len()
        )));
    };
    dst.copy_from_slice(&word.to_le_bytes());
    Ok(())
}

fn patch_call_pair(
    section_bytes: &mut [u8],
    site: usize,
    site_abs: u64,
    target_abs: u64,
    addend: i64,
) -> Result<(), LinkerError> {
    let auipc_word = read_u32_at(section_bytes, site)?;
    let jalr_word = read_u32_at(section_bytes, site + 4)?;

    let offset = (target_abs as i64)
        .wrapping_add(addend)
        .wrapping_sub(site_abs as i64);
    let (hi20, lo12) = pcrel_split(offset);

    let auipc_patched = (auipc_word & 0x0000_0FFF) | (((hi20 << 12) as u32) & 0xFFFF_F000);
    let jalr_patched = (jalr_word & !(0xFFF << 20)) | (((lo12 as u32) & 0xFFF) << 20);

    write_u32_at(section_bytes, site, auipc_patched)?;
    write_u32_at(section_bytes, site + 4, jalr_patched)?;
    Ok(())
}

fn patch_jal(
    section_bytes: &mut [u8],
    site: usize,
    site_abs: u64,
    target_abs: u64,
    addend: i64,
) -> Result<(), LinkerError> {
    let jal_word = read_u32_at(section_bytes, site)?;
    let offset = (target_abs as i64)
        .wrapping_add(addend)
        .wrapping_sub(site_abs as i64);

    if (offset & 1) != 0 {
        return Err(LinkerError::new(format!(
            "JAL relocation offset {offset} is not 2-byte aligned"
        )));
    }
    if !(-1_048_576..=1_048_574).contains(&offset) {
        return Err(LinkerError::new(format!(
            "JAL relocation offset {offset} out of range"
        )));
    }

    let imm = offset as i32;
    let bit20 = (((imm >> 20) & 0x1) as u32) << 31;
    let bits10_1 = (((imm >> 1) & 0x3FF) as u32) << 21;
    let bit11 = (((imm >> 11) & 0x1) as u32) << 20;
    let bits19_12 = (((imm >> 12) & 0xFF) as u32) << 12;
    let imm_bits = bit20 | bits19_12 | bit11 | bits10_1;

    let patched = (jal_word & 0x0000_0FFF) | imm_bits;
    write_u32_at(section_bytes, site, patched)
}

fn patch_la(
    section_bytes: &mut [u8],
    site: usize,
    site_abs: u64,
    target_abs: u64,
    addend: i64,
) -> Result<(), LinkerError> {
    let auipc_word = read_u32_at(section_bytes, site)?;
    let addi_word = read_u32_at(section_bytes, site + 4)?;

    let offset = (target_abs as i64)
        .wrapping_add(addend)
        .wrapping_sub(site_abs as i64);
    let (hi20, lo12) = pcrel_split(offset);

    let auipc_patched = (auipc_word & 0x0000_0FFF) | (((hi20 << 12) as u32) & 0xFFFF_F000);
    let addi_patched = (addi_word & !0xFFF0_0000) | (((lo12 as u32) & 0xFFF) << 20);

    write_u32_at(section_bytes, site, auipc_patched)?;
    write_u32_at(section_bytes, site + 4, addi_patched)
}
