//! System bus, routes physical addresses to ROM, RAM, and MMIO devices.

use crate::devices::{
    clint::Clint, framebuffer::Framebuffer, keyboard::Keyboard, plic::Plic, uart::Uart,
};
use crate::error::VmError;
use crate::memory::{
    MemoryAccess, PeekByteRaw as _,
    cache::{Cache, CacheParams},
    ram::Ram,
    rom::Rom,
};

pub const ROM_BASE: u64 = 0x0000_0000;
pub const ROM_SIZE: u64 = 256 * 1024 * 1024; // 256 MiB
pub const ROM_END: u64 = ROM_BASE + ROM_SIZE - 1;
pub const UART_BASE: u64 = 0x1000_0000;
pub const UART_SIZE: u64 = 0x1000;
pub const UART_END: u64 = UART_BASE + UART_SIZE - 1;
/// SYSCON power-off device. Writing any 8-byte value halts the VM with the exit code.
pub const SYSCON_BASE: u64 = 0x1001_0000;
pub const SYSCON_SIZE: u64 = 0x1000;
pub const SYSCON_END: u64 = SYSCON_BASE + SYSCON_SIZE - 1;
/// Linear framebuffer MMIO device. See `devices::framebuffer`.
/// The span covers the pixel buffer plus a one-page control block (FILL, ...).
pub const FB_BASE: u64 = 0x1002_0000;
pub const FB_SIZE: u64 = crate::devices::framebuffer::FB_TOTAL_BYTES as u64;
pub const FB_END: u64 = FB_BASE + FB_SIZE - 1;
/// Keyboard input MMIO device. See `devices::keyboard`.
/// Placed above `FB_END` (the framebuffer spans ~311 KiB from `FB_BASE`).
pub const KBD_BASE: u64 = 0x1007_0000;
pub const KBD_SIZE: u64 = crate::devices::keyboard::KBD_TOTAL_BYTES as u64;
pub const KBD_END: u64 = KBD_BASE + KBD_SIZE - 1;
// The framebuffer device span is large; guard against it overlapping the keyboard.
const _: () = assert!(KBD_BASE > FB_END);
pub const CLINT_BASE: u64 = 0x0200_0000;
pub const CLINT_SIZE: u64 = 0x10000;
pub const CLINT_END: u64 = CLINT_BASE + CLINT_SIZE - 1;
pub const PLIC_BASE: u64 = 0x0C00_0000;
// Include context threshold/claim region at offsets 0x200000/0x200004.
pub const PLIC_SIZE: u64 = 0x0100_0000;
pub const PLIC_END: u64 = PLIC_BASE + PLIC_SIZE - 1;
pub const RAM_BASE: u64 = 0x8000_0000;
pub const RAM_SIZE_DEFAULT: usize = 128 * 1024 * 1024; // 128 MiB
pub const ELF_LOAD_BASE: u64 = 0x0001_0000;
pub const HEAP_PTR_ADDR: u64 = RAM_BASE + 32 * 1024 * 1024 - 8; // 0x81FF_FFF8

pub struct SystemBus {
    rom: Rom,
    l1_cache: Cache<Cache<Cache<Ram>>>, // L1 -> L2 -> L3 -> RAM
    uart: Uart,
    clint: Clint,
    plic: Plic,
    framebuffer: Framebuffer,
    keyboard: Keyboard,
    syscon_exit: Option<i64>,
}

impl SystemBus {
    pub fn new(rom_data: Vec<u8>) -> Self {
        let rom = Rom::new(ROM_BASE, rom_data);

        // Create three-level cache hierarchy
        // L3: 8MB, 64-byte blocks, 16-way set associative
        let ram = Ram::new(RAM_BASE, RAM_SIZE_DEFAULT);
        let l3_params = CacheParams {
            size: 8 * 1024 * 1024, // 8MB
            block_size: 64,
            associativity: 16,
            write_back: true,
            read_only: false,
        };
        let l3_cache = Cache::new(l3_params, ram);

        // L2: 256KB, 64-byte blocks, 8-way set associative
        let l2_params = CacheParams {
            size: 256 * 1024, // 256KB
            block_size: 64,
            associativity: 8,
            write_back: true,
            read_only: false,
        };
        let l2_cache = Cache::new(l2_params, l3_cache);

        // L1: 4KB, 64-byte blocks, 2-way set associative
        let l1_params = CacheParams {
            size: 4096, // 4KB
            block_size: 64,
            associativity: 2,
            write_back: true,
            read_only: false,
        };
        let l1_cache = Cache::new(l1_params, l2_cache);

        let uart = Uart::new();
        let clint = Clint::new();
        let plic = Plic::new();
        let framebuffer = Framebuffer::new();
        let keyboard = Keyboard::new();
        Self {
            rom,
            l1_cache,
            uart,
            clint,
            plic,
            framebuffer,
            keyboard,
            syscon_exit: None,
        }
    }

    /// Route memory accesses through L1 cache for RAM, direct for MMIO.
    ///
    /// IMPORTANT: UART, CLINT, and PLIC must be checked BEFORE ROM because their
    /// IMPORTANT: physical addresses fall within the ROM range (0x0000_0000-0x0FFF_FFFF).
    /// IMPORTANT: SYSCON writes are intercepted in the `MemoryAccess` impl below, not here.
    #[inline]
    fn route(&mut self, addr: u64) -> Option<(&mut dyn MemoryAccess, u64)> {
        match addr {
            a if a >= UART_BASE && a <= UART_END => Some((&mut self.uart, addr - UART_BASE)),
            a if a >= CLINT_BASE && a <= CLINT_END => Some((&mut self.clint, addr - CLINT_BASE)),
            a if a >= PLIC_BASE && a <= PLIC_END => Some((&mut self.plic, addr - PLIC_BASE)),
            a if a >= FB_BASE && a <= FB_END => Some((&mut self.framebuffer, addr - FB_BASE)),
            a if a >= KBD_BASE && a <= KBD_END => Some((&mut self.keyboard, addr - KBD_BASE)),
            a if a >= ROM_BASE && a <= ROM_END => Some((&mut self.rom, addr)),
            _ => {
                // Route all RAM accesses through L1 cache (which cascades to L2, L3, then RAM)
                Some((&mut self.l1_cache, addr))
            }
        }
    }

    // Expose RAM size for bounds check (also needed by route)
    pub fn ram_size(&self) -> usize {
        // Return the size of the underlying RAM in the data cache
        // This is a simplification - in reality we'd need to expose this from Cache
        RAM_SIZE_DEFAULT
    }

    pub fn rom_size(&self) -> usize {
        self.rom.size() as usize
    }

    /// Cheap stats-only read, no allocation. Called every step.
    pub fn get_cache_stats(
        &self,
    ) -> (
        crate::memory::cache::CacheStats,
        crate::memory::cache::CacheStats,
        crate::memory::cache::CacheStats,
    ) {
        let l1 = self.l1_cache.stats().clone();
        let l2 = self.l1_cache.peek_next().stats().clone();
        let l3 = self.l1_cache.peek_next().peek_next().stats().clone();
        (l1, l2, l3)
    }

    /// Flush all dirty cache lines to RAM then invalidate every level cold.
    pub fn cold_cache_reset(&mut self) {
        self.l1_cache.flush_and_invalidate();
        self.l1_cache.peek_next_mut().flush_and_invalidate();
        self.l1_cache
            .peek_next_mut()
            .peek_next_mut()
            .flush_and_invalidate();
    }

    /// Get full snapshots for all three cache levels (params + line states + stats).
    pub fn get_cache_snapshots(
        &self,
    ) -> (
        crate::memory::cache::CacheSnapshot,
        crate::memory::cache::CacheSnapshot,
        crate::memory::cache::CacheSnapshot,
    ) {
        let l1 = self.l1_cache.snapshot();
        let l2 = self.l1_cache.peek_next().snapshot();
        let l3 = self.l1_cache.peek_next().peek_next().snapshot();
        (l1, l2, l3)
    }

    /// Read bytes from the address space for debug inspection.
    /// ROM and MMIO are read directly. RAM is read via the cache hierarchy, which updates
    /// cache stats as a side effect (use `peek_bytes_raw` when that matters).
    pub fn peek_bytes(&mut self, addr: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| match addr + i {
                a if a >= ROM_BASE && a <= ROM_END => self.rom.read_byte(a).unwrap_or(0),
                a if a >= UART_BASE && a <= UART_END => {
                    self.uart.read_byte(a - UART_BASE).unwrap_or(0)
                }
                a if a >= CLINT_BASE && a <= CLINT_END => {
                    self.clint.read_byte(a - CLINT_BASE).unwrap_or(0)
                }
                a if a >= PLIC_BASE && a <= PLIC_END => {
                    self.plic.read_byte(a - PLIC_BASE).unwrap_or(0)
                }
                a if a >= FB_BASE && a <= FB_END => self
                    .framebuffer
                    .pixels()
                    .get((a - FB_BASE) as usize)
                    .copied()
                    .unwrap_or(0),
                // Keyboard DATA reads have a pop side effect; never touch it from
                // the debug peek path.
                a if a >= KBD_BASE && a <= KBD_END => 0,
                _ => self.l1_cache.read_byte(addr + i).unwrap_or(0),
            })
            .collect()
    }

    /// Read bytes for debug display without touching cache stats or LRU state.
    /// Routes by address: ROM and MMIO are read directly (non-destructive).
    /// RAM is read through the L1/L2/L3 cache hierarchy.
    pub fn peek_bytes_raw(&self, addr: u64, len: usize) -> Vec<u8> {
        (0..len as u64)
            .map(|i| {
                let a = addr + i;
                match a {
                    a if a >= UART_BASE && a <= UART_END => {
                        self.uart.peek_byte(a - UART_BASE).unwrap_or(0)
                    }
                    a if a >= CLINT_BASE && a <= CLINT_END => {
                        // CLINT does not support byte access; return 0 for debug.
                        0
                    }
                    a if a >= PLIC_BASE && a <= PLIC_END => {
                        // PLIC does not support byte access; return 0 for debug.
                        0
                    }
                    a if a >= FB_BASE && a <= FB_END => self
                        .framebuffer
                        .pixels()
                        .get((a - FB_BASE) as usize)
                        .copied()
                        .unwrap_or(0),
                    // Keyboard DATA reads have a pop side effect; never touch from
                    // the debug peek path.
                    a if a >= KBD_BASE && a <= KBD_END => 0,
                    a if a >= ROM_BASE && a <= ROM_END => {
                        self.rom.peek_byte(a).unwrap_or(0)
                    }
                    _ => self.l1_cache.peek_byte_raw(a).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Borrow the framebuffer's pixel buffer for display.
    pub fn peek_framebuffer(&self) -> &[u8] {
        self.framebuffer.pixels()
    }

    /// Number of full-screen FILL clears the framebuffer has performed.
    pub fn framebuffer_fill_count(&self) -> u64 {
        self.framebuffer.fill_count()
    }

    /// Push a key event from the host GUI into the keyboard device.
    pub fn keyboard_push(&mut self, scancode: u16, pressed: bool) {
        self.keyboard.push_event(scancode, pressed);
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

    /// Drain and return the SYSCON exit code, if `sys_exit` was called.
    pub fn take_syscon_exit(&mut self) -> Option<i64> {
        self.syscon_exit.take()
    }
}

impl MemoryAccess for SystemBus {
    #[inline]
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_byte(local)
    }

    #[inline]
    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_halfword(local)
    }

    #[inline]
    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_word(local)
    }

    #[inline]
    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.read_doubleword(local)
    }

    #[inline]
    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        if addr >= SYSCON_BASE && addr <= SYSCON_END {
            if addr == SYSCON_BASE {
                self.syscon_exit = Some(data as i64);
            }
            return Ok(());
        }
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_byte(local, data)
    }

    #[inline]
    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        if addr >= SYSCON_BASE && addr <= SYSCON_END {
            if addr == SYSCON_BASE {
                self.syscon_exit = Some(data as i64);
            }
            return Ok(());
        }
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_halfword(local, data)
    }

    #[inline]
    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        if addr >= SYSCON_BASE && addr <= SYSCON_END {
            if addr == SYSCON_BASE {
                self.syscon_exit = Some(data as i64);
            }
            return Ok(());
        }
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_word(local, data)
    }

    #[inline]
    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        if addr >= SYSCON_BASE && addr <= SYSCON_END {
            self.syscon_exit = Some(data as i64);
            return Ok(());
        }
        let (dev, local) = self.route(addr).ok_or(VmError::BusError(addr))?;
        dev.write_doubleword(local, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Writes at FB_BASE land in the device, not RAM, and read back via the peek path.
    #[test]
    fn framebuffer_writes_route_to_device() {
        let mut bus = SystemBus::new(vec![0u8; 64]);

        // Top-left pixel: R=0x11 G=0x22 B=0x33 A=0xFF.
        bus.write_word(FB_BASE, 0xFF33_2211).unwrap();
        let p = FB_BASE + 100;
        bus.write_byte(p, 0xAB).unwrap();

        assert_eq!(bus.read_word(FB_BASE).unwrap(), 0xFF33_2211);
        assert_eq!(bus.read_byte(p).unwrap(), 0xAB);

        let px = bus.peek_framebuffer();
        assert_eq!(&px[0..4], &[0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(px[(p - FB_BASE) as usize], 0xAB);
    }
}
