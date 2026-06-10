//! Read-Only Memory - holds boot code and fixed firmware.
//! Mapped at the bottom of the address space (`0x0000_0000` .. `0x0FFF_FFFF`).

use crate::error::VmError;
use crate::memory::MemoryAccess;

/// Simple ROM: loaded once, never modified.
pub struct Rom {
    data: Vec<u8>,
    base: u64,
    size: u64,
}

impl Rom {
    /// Create a ROM from a byte vector. `base` is the start address in the system
    /// memory map (normally `0x0000_0000`).
    pub fn new(base: u64, data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self { data, base, size }
    }

    /// Quick check whether an address falls inside this ROM.
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.base + self.size
    }

    /// Convert a physical address to an index into the data array.
    /// Returns `None` if out of bounds.
    fn index(&self, addr: u64) -> Option<usize> {
        if self.contains(addr) {
            Some((addr - self.base) as usize)
        } else {
            None
        }
    }

    /// Return the size of ROM in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Peek at a ROM byte without mutation.
    pub fn peek_byte(&self, addr: u64) -> Option<u8> {
        self.index(addr).map(|idx| self.data[idx])
    }
}

impl MemoryAccess for Rom {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        match self.index(addr) {
            Some(idx) => Ok(self.data[idx]),
            None => Err(VmError::BusError(addr)),
        }
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let b0 = self.read_byte(addr)? as u16;
        let b1 = self.read_byte(addr + 1)? as u16;
        Ok(b0 | (b1 << 8)) // little-endian
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

    fn write_byte(&mut self, _addr: u64, _data: u8) -> Result<(), VmError> {
        Err(VmError::WriteToRom)
    }
    fn write_halfword(&mut self, _addr: u64, _data: u16) -> Result<(), VmError> {
        Err(VmError::WriteToRom)
    }
    fn write_word(&mut self, _addr: u64, _data: u32) -> Result<(), VmError> {
        Err(VmError::WriteToRom)
    }
    fn write_doubleword(&mut self, _addr: u64, _data: u64) -> Result<(), VmError> {
        Err(VmError::WriteToRom)
    }
}
