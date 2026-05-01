//! Memory subsystem: trait, bus, and sub‑module organisation.

pub mod cache;
pub mod ram;
pub mod rom;

use crate::virtual_machine::error::VmError;

/// Every component in the memory hierarchy must implement this.
pub trait MemoryAccess {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError>;
    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let lo = self.read_byte(addr)? as u16;
        let hi = self.read_byte(addr + 1)? as u16;
        Ok(lo | (hi << 8))
    }
    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let lo = self.read_halfword(addr)? as u32;
        let hi = self.read_halfword(addr + 2)? as u32;
        Ok(lo | (hi << 16))
    }
    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let lo = self.read_word(addr)? as u64;
        let hi = self.read_word(addr + 4)? as u64;
        Ok(lo | (hi << 32))
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError>;
    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        self.write_byte(addr, data as u8)?;
        self.write_byte(addr + 1, (data >> 8) as u8)
    }
    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        self.write_halfword(addr, data as u16)?;
        self.write_halfword(addr + 2, (data >> 16) as u16)
    }
    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        self.write_word(addr, data as u32)?;
        self.write_word(addr + 4, (data >> 32) as u32)
    }
}

/// A simple bus that routes addresses to ROM or RAM.
pub struct Bus {
    rom: rom::Rom,
    ram: ram::Ram,
}

impl Bus {
    pub fn new(rom: rom::Rom, ram: ram::Ram) -> Self {
        Self { rom, ram }
    }
}

impl MemoryAccess for Bus {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        if self.rom.contains(addr) {
            self.rom.read_byte(addr)
        } else if self.ram.contains(addr) {
            self.ram.read_byte(addr)
        } else {
            Err(VmError::BusError(addr))
        }
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        if self.ram.contains(addr) {
            self.ram.write_byte(addr, data)
        } else {
            Err(VmError::BusError(addr))
        }
    }
}
