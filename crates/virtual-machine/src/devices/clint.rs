//! CLINT - Core-Local Interruptor (timer + software interrupts).
//! Memory-mapped at 0x0200_0000.

use crate::error::VmError;
use crate::memory::MemoryAccess;

pub struct Clint {
    pub mtime: u64,
    mtimecmp: u64,
    msip: u32,
}

impl Clint {
    pub fn new() -> Self {
        Self {
            mtime: 0,
            mtimecmp: u64::MAX,
            msip: 0,
        }
    }

    pub fn timer_irq_pending(&self) -> bool {
        self.mtime >= self.mtimecmp
    }

    pub fn software_irq_pending(&self) -> bool {
        self.msip != 0
    }

    pub fn set_mtime(&mut self, value: u64) {
        self.mtime = value;
    }

    pub fn tick(&mut self) {
        self.mtime = self.mtime.wrapping_add(1);
    }
}

const MSIP_BASE: u64 = 0x0000;
const MTIMECMP_BASE: u64 = 0x4000;
const MTIME_BASE: u64 = 0xBFF8;

impl MemoryAccess for Clint {
    // Byte and halfword accesses are illegal -> fault.
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        Err(VmError::LoadAccessFault(addr))
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        match addr {
            a if a == MTIME_BASE => Ok(self.mtime as u32),
            a if a == MTIME_BASE + 4 => Ok((self.mtime >> 32) as u32),
            a if a == MTIMECMP_BASE => Ok(self.mtimecmp as u32),
            a if a == MTIMECMP_BASE + 4 => Ok((self.mtimecmp >> 32) as u32),
            a if a == MSIP_BASE => Ok(self.msip),
            _ => Err(VmError::LoadAccessFault(addr)),
        }
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let lo = self.read_word(addr)? as u64;
        let hi = self.read_word(addr + 4)? as u64;
        Ok(lo | (hi << 32))
    }

    fn write_byte(&mut self, _addr: u64, _data: u8) -> Result<(), VmError> {
        Err(VmError::StoreAccessFault(_addr))
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        match addr {
            a if a == MTIME_BASE => {
                self.mtime = (self.mtime & 0xFFFF_FFFF_0000_0000) | data as u64;
                Ok(())
            }
            a if a == MTIME_BASE + 4 => {
                self.mtime = (self.mtime & 0x0000_0000_FFFF_FFFF) | ((data as u64) << 32);
                Ok(())
            }
            a if a == MTIMECMP_BASE => {
                self.mtimecmp = (self.mtimecmp & 0xFFFF_FFFF_0000_0000) | data as u64;
                Ok(())
            }
            a if a == MTIMECMP_BASE + 4 => {
                self.mtimecmp = (self.mtimecmp & 0x0000_0000_FFFF_FFFF) | ((data as u64) << 32);
                Ok(())
            }
            a if a == MSIP_BASE => {
                self.msip = data & 1;
                Ok(())
            }
            _ => Err(VmError::StoreAccessFault(addr)),
        }
    }

    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        self.write_word(addr, data as u32)?;
        self.write_word(addr + 4, (data >> 32) as u32)
    }
}
