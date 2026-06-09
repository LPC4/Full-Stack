//! Linear RGBA8888 framebuffer device. See the VM spec for the layout.

use crate::error::VmError;
use crate::memory::MemoryAccess;

pub const FB_WIDTH: usize = 320;
pub const FB_HEIGHT: usize = 240;
pub const FB_BPP: usize = 4;
pub const FB_BYTES: usize = FB_WIDTH * FB_HEIGHT * FB_BPP;

/// A flat pixel buffer the guest draws into; the GUI reads it back for display.
pub struct Framebuffer {
    pixels: Vec<u8>,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; FB_BYTES],
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl MemoryAccess for Framebuffer {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        self.pixels
            .get(addr as usize)
            .copied()
            .ok_or(VmError::BusError(addr))
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        match self.pixels.get_mut(addr as usize) {
            Some(slot) => {
                *slot = data;
                Ok(())
            }
            None => Err(VmError::BusError(addr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrips_each_width() {
        let mut fb = Framebuffer::new();
        fb.write_byte(0, 0xAB).unwrap();
        fb.write_halfword(4, 0xBEEF).unwrap();
        fb.write_word(8, 0xDEAD_BEEF).unwrap();
        assert_eq!(fb.read_byte(0).unwrap(), 0xAB);
        assert_eq!(fb.read_halfword(4).unwrap(), 0xBEEF);
        assert_eq!(fb.read_word(8).unwrap(), 0xDEAD_BEEF);
    }

    #[test]
    fn out_of_range_access_errors() {
        let mut fb = Framebuffer::new();
        assert!(fb.read_byte(FB_BYTES as u64).is_err());
        assert!(fb.write_byte(FB_BYTES as u64, 1).is_err());
    }
}
