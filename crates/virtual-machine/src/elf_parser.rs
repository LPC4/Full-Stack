//! ELF (Executable and Linkable Format) parser for RV64.
//!
//! This module parses ELF-64 images and extracts load segments and entry point
//! information for loading into the virtual machine's memory.

use crate::error::VmError;

/// Parsed ELF image with load segments and entry point
#[derive(Debug)]
pub struct ParsedElf {
    pub entry_point: u64,
    pub load_segments: Vec<ElfLoadSegment>,
}

/// A loadable segment from an ELF PT_LOAD program header
#[derive(Debug)]
pub struct ElfLoadSegment {
    pub offset: u64,
    pub vaddr: u64,
    pub file_size: u64,
    pub mem_size: u64,
}

impl ParsedElf {
    /// Parse an ELF-64 image from raw bytes
    ///
    /// # Returns
    /// Parsed ELF structure or error if the format is invalid
    pub fn parse(bytes: &[u8]) -> Result<Self, VmError> {
        const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
        const ELFCLASS64: u8 = 2;
        const ELFDATA2LSB: u8 = 1;
        const PT_LOAD: u32 = 1;

        if bytes.len() < 64 {
            return Err(VmError::Other("ELF image is too small".to_string()));
        }
        if &bytes[0..4] != ELF_MAGIC {
            return Err(VmError::Other("ELF magic header missing".to_string()));
        }
        if bytes[4] != ELFCLASS64 {
            return Err(VmError::Other("ELF image is not 64-bit".to_string()));
        }
        if bytes[5] != ELFDATA2LSB {
            return Err(VmError::Other("ELF image is not little-endian".to_string()));
        }

        let entry_point = read_u64(bytes, 24)?;
        let phoff = read_u64(bytes, 32)?;
        let phentsize = read_u16(bytes, 54)? as u64;
        let phnum = read_u16(bytes, 56)? as u64;

        if phentsize < 56 {
            return Err(VmError::Other(
                "ELF program header size is invalid".to_string(),
            ));
        }

        let mut load_segments = Vec::new();
        for i in 0..phnum {
            let base = phoff
                .checked_add(i * phentsize)
                .ok_or_else(|| VmError::Other("ELF program header overflow".to_string()))?;
            let ph = base as usize;
            let end = ph
                .checked_add(phentsize as usize)
                .ok_or_else(|| VmError::Other("ELF program header slice overflow".to_string()))?;
            let header = bytes
                .get(ph..end)
                .ok_or_else(|| VmError::Other("ELF program header outside file".to_string()))?;

            let p_type = read_u32(header, 0)?;
            if p_type != PT_LOAD {
                continue;
            }

            load_segments.push(ElfLoadSegment {
                offset: read_u64(header, 8)?,
                vaddr: read_u64(header, 16)?,
                file_size: read_u64(header, 32)?,
                mem_size: read_u64(header, 40)?,
            });
        }

        if load_segments.is_empty() {
            return Err(VmError::Other(
                "ELF contains no PT_LOAD segments".to_string(),
            ));
        }

        Ok(Self {
            entry_point,
            load_segments,
        })
    }
}

/// Read a 16-bit little-endian value from bytes at the given offset
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, VmError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

/// Read a 32-bit little-endian value from bytes at the given offset
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, VmError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Read a 64-bit little-endian value from bytes at the given offset
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, VmError> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| VmError::Other("ELF read overflow".to_string()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| VmError::Other("ELF read out of bounds".to_string()))?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

/// Align a value up to the nearest multiple of alignment
pub fn align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    (value + alignment - 1) & !(alignment - 1)
}
