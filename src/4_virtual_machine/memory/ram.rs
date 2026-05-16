//! Main memory (DRAM). Occupies the top half of the address space
//! (0x8000_0000 .. 0xFFFF_FFFF) by default.

use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::{MemoryAccess, PeekByteRaw};

pub struct Ram {
    data: Vec<u8>,
    base: u64,
    size: u64,
}

impl Ram {
    /// Create RAM of `size_bytes` starting at `base`.
    pub fn new(base: u64, size_bytes: usize) -> Self {
        Self {
            data: vec![0u8; size_bytes],
            base,
            size: size_bytes as u64,
        }
    }

    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.base + self.size
    }

    fn index(&self, addr: u64) -> Option<usize> {
        if self.contains(addr) {
            Some((addr - self.base) as usize)
        } else {
            None
        }
    }

    /// Return the size of RAM in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Peek at a byte without mutation (for debugging).
    pub fn peek_byte(&self, addr: u64) -> Option<u8> {
        self.index(addr).map(|idx| self.data[idx])
    }
}

impl PeekByteRaw for Ram {
    fn peek_byte_raw(&self, addr: u64) -> Option<u8> {
        self.peek_byte(addr)
    }
}

impl MemoryAccess for Ram {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        match self.index(addr) {
            Some(idx) => Ok(self.data[idx]),
            None => Err(VmError::BusError(addr)),
        }
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let b0 = self.read_byte(addr)? as u16;
        let b1 = self.read_byte(addr + 1)? as u16;
        Ok(b0 | (b1 << 8))
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let b0 = self.read_byte(addr)? as u32;
        let b1 = self.read_byte(addr + 1)? as u32;
        let b2 = self.read_byte(addr + 2)? as u32;
        let b3 = self.read_byte(addr + 3)? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let lo = self.read_word(addr)? as u64;
        let hi = self.read_word(addr + 4)? as u64;
        Ok(lo | (hi << 32))
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        match self.index(addr) {
            Some(idx) => {
                self.data[idx] = data;
                Ok(())
            }
            None => Err(VmError::BusError(addr)),
        }
    }

    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        self.write_byte(addr, data as u8)?;
        self.write_byte(addr + 1, (data >> 8) as u8)?;
        Ok(())
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        self.write_byte(addr, data as u8)?;
        self.write_byte(addr + 1, (data >> 8) as u8)?;
        self.write_byte(addr + 2, (data >> 16) as u8)?;
        self.write_byte(addr + 3, (data >> 24) as u8)?;
        Ok(())
    }

    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        self.write_word(addr, data as u32)?;
        self.write_word(addr + 4, (data >> 32) as u32)?;
        Ok(())
    }
}
