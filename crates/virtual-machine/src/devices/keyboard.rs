//! Keyboard input MMIO device: a small ring buffer of key events the host GUI
//! pushes and the guest pops. Mapped at `KBD_BASE` (0x10070000, above the
//! framebuffer span). See the VM spec section 6.6 for the register layout.

use crate::error::VmError;
use crate::memory::MemoryAccess;

// Register offsets (word accesses).
// STATUS reads back the number of queued events (0 = empty).
// DATA reads pop the oldest event; on an empty queue it reads back KBD_EMPTY.
pub const KBD_STATUS_REG: usize = 0;
pub const KBD_DATA_REG: usize = 4;

// One control page; the device occupies a single page on the bus.
pub const KBD_TOTAL_BYTES: usize = 4096;

// Sentinel returned by a DATA read when the queue is empty.
pub const KBD_EMPTY: u32 = 0xFFFF_FFFF;

// Bit 16 of a packed event marks a press (1) vs a release (0); bits 15..0 are the
// scancode.
pub const KBD_PRESSED_BIT: u32 = 1 << 16;

// Cap the queue so a host that never lets the guest drain it cannot grow unbounded.
const KBD_QUEUE_CAP: usize = 256;

/// A bounded FIFO of key events shared between the host GUI and the guest.
pub struct Keyboard {
    queue: std::collections::VecDeque<u32>,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            queue: std::collections::VecDeque::new(),
        }
    }

    /// Push a key event from the host. `scancode` is truncated to 16 bits;
    /// `pressed` distinguishes a key-down from a key-up. Drops the event when the
    /// queue is full so a stalled guest cannot make the host leak memory.
    pub fn push_event(&mut self, scancode: u16, pressed: bool) {
        if self.queue.len() >= KBD_QUEUE_CAP {
            return;
        }
        let mut ev = scancode as u32;
        if pressed {
            ev |= KBD_PRESSED_BIT;
        }
        self.queue.push_back(ev);
    }

    /// Number of events waiting to be read.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    // Pop the oldest event, or the empty sentinel.
    fn pop(&mut self) -> u32 {
        self.queue.pop_front().unwrap_or(KBD_EMPTY)
    }

    fn read_reg(&mut self, offset: usize) -> u32 {
        match offset {
            KBD_STATUS_REG => self.queue.len() as u32,
            KBD_DATA_REG => self.pop(),
            _ => 0,
        }
    }
}

impl MemoryAccess for Keyboard {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        let a = addr as usize;
        if a >= KBD_TOTAL_BYTES {
            return Err(VmError::BusError(addr));
        }
        // A byte read returns the matching byte of the register word. Reading the
        // DATA byte still pops, so guests should use a word read.
        let reg = a & !0x3;
        let shift = (a & 0x3) * 8;
        Ok((self.read_reg(reg) >> shift) as u8)
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let a = addr as usize;
        if a + 4 > KBD_TOTAL_BYTES {
            return Err(VmError::BusError(addr));
        }
        Ok(self.read_reg(a))
    }

    fn write_byte(&mut self, addr: u64, _data: u8) -> Result<(), VmError> {
        let a = addr as usize;
        if a >= KBD_TOTAL_BYTES {
            return Err(VmError::BusError(addr));
        }
        // Registers are read-only from the guest; ignore writes.
        Ok(())
    }

    fn write_word(&mut self, addr: u64, _data: u32) -> Result<(), VmError> {
        let a = addr as usize;
        if a + 4 > KBD_TOTAL_BYTES {
            return Err(VmError::BusError(addr));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_queue_depth() {
        let mut kbd = Keyboard::new();
        assert_eq!(kbd.read_word(KBD_STATUS_REG as u64).unwrap(), 0);
        kbd.push_event(0x41, true);
        kbd.push_event(0x41, false);
        assert_eq!(kbd.read_word(KBD_STATUS_REG as u64).unwrap(), 2);
    }

    #[test]
    fn data_pops_in_fifo_order_with_press_bit() {
        let mut kbd = Keyboard::new();
        kbd.push_event(0x41, true);
        kbd.push_event(0x42, false);

        let first = kbd.read_word(KBD_DATA_REG as u64).unwrap();
        assert_eq!(first & 0xFFFF, 0x41);
        assert_eq!(first & KBD_PRESSED_BIT, KBD_PRESSED_BIT);

        let second = kbd.read_word(KBD_DATA_REG as u64).unwrap();
        assert_eq!(second & 0xFFFF, 0x42);
        assert_eq!(second & KBD_PRESSED_BIT, 0);

        // Drained: status is zero and DATA reads the empty sentinel.
        assert_eq!(kbd.read_word(KBD_STATUS_REG as u64).unwrap(), 0);
        assert_eq!(kbd.read_word(KBD_DATA_REG as u64).unwrap(), KBD_EMPTY);
    }

    #[test]
    fn queue_is_bounded() {
        let mut kbd = Keyboard::new();
        for _ in 0..(KBD_QUEUE_CAP + 50) {
            kbd.push_event(1, true);
        }
        assert_eq!(kbd.pending(), KBD_QUEUE_CAP);
    }

    #[test]
    fn out_of_range_access_errors() {
        let mut kbd = Keyboard::new();
        assert!(kbd.read_word(KBD_TOTAL_BYTES as u64).is_err());
        assert!(kbd.write_word(KBD_TOTAL_BYTES as u64, 0).is_err());
    }
}
