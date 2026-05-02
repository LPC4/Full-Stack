//! System bus, routes physical addresses to ROM, RAM, and MMIO devices.

use crate::virtual_machine::devices::{clint::Clint, plic::Plic, uart::Uart};
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::{MemoryAccess, ram::Ram, rom::Rom};

/// Physical address map
pub const ROM_BASE: u64 = 0x0000_0000;
pub const ROM_SIZE: u64 = 256 * 1024 * 1024; // 256 MiB
pub const ROM_END: u64 = ROM_BASE + ROM_SIZE - 1;
pub const UART_BASE: u64 = 0x1000_0000;
pub const UART_SIZE: u64 = 0x1000;
pub const UART_END: u64 = UART_BASE + UART_SIZE - 1;
pub const CLINT_BASE: u64 = 0x0200_0000;
pub const CLINT_SIZE: u64 = 0x10000;
pub const CLINT_END: u64 = CLINT_BASE + CLINT_SIZE - 1;
pub const PLIC_BASE: u64 = 0x0C00_0000;
pub const PLIC_SIZE: u64 = 0x20_0000;
pub const PLIC_END: u64 = PLIC_BASE + PLIC_SIZE - 1;
pub const RAM_BASE: u64 = 0x8000_0000;
pub const RAM_SIZE_DEFAULT: usize = 128 * 1024 * 1024; // 128 MiB

/// Fixed address where the heap bump-pointer is stored (one u64 word in RAM).
/// The actual heap region begins at `HEAP_PTR_ADDR + 8`.
pub const HEAP_PTR_ADDR: u64 = RAM_BASE + 32 * 1024 * 1024 - 8; // 0x81FF_FFF8

pub struct SystemBus {
    rom: Rom,
    ram: Ram,
    uart: Uart,
    clint: Clint,
    plic: Plic,
}

impl SystemBus {
    pub fn new(rom_data: Vec<u8>) -> Self {
        let rom = Rom::new(ROM_BASE, rom_data);
        let ram = Ram::new(RAM_BASE, RAM_SIZE_DEFAULT);
        let uart = Uart::new();
        let clint = Clint::new();
        let plic = Plic::new();
        Self {
            rom,
            ram,
            uart,
            clint,
            plic,
        }
    }

    /// Map an absolute address to the correct device and return its local offset.
    fn route(&mut self, addr: u64) -> Option<(&mut dyn MemoryAccess, u64)> {
        match addr {
            a if a >= ROM_BASE && a <= ROM_END => Some((&mut self.rom, addr)),
            a if a >= UART_BASE && a <= UART_END => Some((&mut self.uart, addr - UART_BASE)),
            a if a >= CLINT_BASE && a <= CLINT_END => Some((&mut self.clint, addr - CLINT_BASE)),
            a if a >= PLIC_BASE && a <= PLIC_END => Some((&mut self.plic, addr - PLIC_BASE)),
            _ => {
                let ram_end = RAM_BASE + self.ram.size() - 1;
                if addr >= RAM_BASE && addr <= ram_end {
                    Some((&mut self.ram, addr))
                } else {
                    None
                }
            }
        }
    }

    // Expose RAM size for bounds check (also needed by route)
    pub fn ram_size(&self) -> usize {
        self.ram.size() as usize
    }
    pub fn rom_size(&self) -> usize {
        self.rom.size() as usize
    }

    // Direct access to devices for interrupt handling
    pub fn uart_mut(&mut self) -> &mut Uart {
        &mut self.uart
    }
    pub fn clint_mut(&mut self) -> &mut Clint {
        &mut self.clint
    }
    pub fn plic_mut(&mut self) -> &mut Plic {
        &mut self.plic
    }
}

impl MemoryAccess for SystemBus {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_byte(local)
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_halfword(local)
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_word(local)
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_doubleword(local)
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_byte(local, data)
    }

    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_halfword(local, data)
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_word(local, data)
    }

    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_doubleword(local, data)
    }
}
