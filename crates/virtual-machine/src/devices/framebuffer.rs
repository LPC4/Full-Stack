//! Linear RGBA8888 framebuffer device: a pixel buffer plus a one-page control
//! block (FILL, PRESENT, DBMODE registers) with optional double buffering to
//! avoid flicker. See the VM spec section 6.5 for the register and buffer layout.

use crate::error::VmError;
use crate::memory::MemoryAccess;

pub const FB_WIDTH: usize = 320;
pub const FB_HEIGHT: usize = 240;
pub const FB_BPP: usize = 4;
pub const FB_BYTES: usize = FB_WIDTH * FB_HEIGHT * FB_BPP;
// Control block (one page), mapped immediately after the pixel buffer.
pub const FB_CTRL_BYTES: usize = 4096;
// Control register offsets (word writes): fill the buffer, swap back->front,
// and enable double buffering respectively.
pub const FB_FILL_REG: usize = 0;
pub const FB_PRESENT_REG: usize = 4;
pub const FB_DBMODE_REG: usize = 8;
pub const FB_TOTAL_BYTES: usize = FB_BYTES + FB_CTRL_BYTES;

/// A flat pixel buffer the guest draws into; the GUI reads it back for display.
pub struct Framebuffer {
    // Buffer the GUI displays.
    front: Vec<u8>,
    // Buffer the guest draws into when double buffering is on.
    back: Vec<u8>,
    // When set, draws/fills hit `back` until PRESENT; else they hit `front`.
    double_buffered: bool,
    // FILL clears performed; a per-frame counter for benchmarking.
    fill_count: u64,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            front: vec![0u8; FB_BYTES],
            back: vec![0u8; FB_BYTES],
            double_buffered: false,
            fill_count: 0,
        }
    }

    /// The buffer the GUI displays.
    pub fn pixels(&self) -> &[u8] {
        &self.front
    }

    /// Total FILL-register clears since boot.
    pub fn fill_count(&self) -> u64 {
        self.fill_count
    }

    // The buffer the guest currently draws into and reads from.
    fn draw_buffer(&mut self) -> &mut Vec<u8> {
        if self.double_buffered {
            &mut self.back
        } else {
            &mut self.front
        }
    }

    // Fill the draw buffer with one RGBA colour (little-endian word).
    fn fill(&mut self, color: u32) {
        let bytes = color.to_le_bytes();
        for px in self.draw_buffer().chunks_exact_mut(FB_BPP) {
            px.copy_from_slice(&bytes);
        }
        self.fill_count += 1;
    }

    // Publish the back buffer to the front (no-op when single-buffered).
    fn present(&mut self) {
        if self.double_buffered {
            self.front.copy_from_slice(&self.back);
        }
    }
}

impl MemoryAccess for Framebuffer {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        let a = addr as usize;
        if a < FB_BYTES {
            // Reads see the buffer the guest is actively drawing into.
            Ok(self.draw_buffer()[a])
        } else if a < FB_TOTAL_BYTES {
            // Control registers read back as zero.
            Ok(0)
        } else {
            Err(VmError::BusError(addr))
        }
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        let a = addr as usize;
        if a < FB_BYTES {
            self.draw_buffer()[a] = data;
            Ok(())
        } else if a < FB_TOTAL_BYTES {
            // Control registers only act on word writes; ignore byte dribbles.
            Ok(())
        } else {
            Err(VmError::BusError(addr))
        }
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        let a = addr as usize;
        if a == FB_BYTES + FB_FILL_REG {
            self.fill(data);
        } else if a == FB_BYTES + FB_PRESENT_REG {
            self.present();
        } else if a == FB_BYTES + FB_DBMODE_REG {
            self.double_buffered = data != 0;
        } else if a + FB_BPP <= FB_BYTES {
            // Pixel word: write the 4 little-endian bytes to the draw buffer.
            self.draw_buffer()[a..a + FB_BPP].copy_from_slice(&data.to_le_bytes());
        } else if a >= FB_BYTES && a < FB_TOTAL_BYTES {
            // Other control offsets: no-op.
        } else {
            // Spans the pixel/end boundary or is out of range: fall back to bytes.
            for i in 0..FB_BPP as u64 {
                self.write_byte(addr + i, (data >> (i * 8)) as u8)?;
            }
        }
        Ok(())
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
        assert!(fb.read_byte(FB_TOTAL_BYTES as u64).is_err());
        assert!(fb.write_byte(FB_TOTAL_BYTES as u64, 1).is_err());
    }

    #[test]
    fn fill_register_clears_whole_buffer() {
        let mut fb = Framebuffer::new();
        // Dirty a couple of pixels first.
        fb.write_word(0, 0x1234_5678).unwrap();
        fb.write_word(FB_BYTES as u64 - 4, 0x9ABC_DEF0).unwrap();

        // Opaque black = 0xFF000000 -> bytes [00,00,00,FF].
        fb.write_word((FB_BYTES + FB_FILL_REG) as u64, 0xFF00_0000)
            .unwrap();

        let px = fb.pixels();
        assert_eq!(px.len(), FB_BYTES);
        for chunk in px.chunks_exact(FB_BPP) {
            assert_eq!(chunk, &[0x00, 0x00, 0x00, 0xFF]);
        }
    }

    #[test]
    fn double_buffer_hides_draws_until_present() {
        let mut fb = Framebuffer::new();
        // Enable double buffering.
        fb.write_word((FB_BYTES + FB_DBMODE_REG) as u64, 1).unwrap();

        // Clear and draw into the back buffer.
        fb.write_word((FB_BYTES + FB_FILL_REG) as u64, 0xFF00_0000)
            .unwrap();
        fb.write_word(0, 0xFFFF_FFFF).unwrap();

        // Front buffer must still be untouched (all zero) before PRESENT.
        assert!(
            fb.pixels().iter().all(|&b| b == 0),
            "front changed before present"
        );

        // After PRESENT the front reflects the back buffer.
        fb.write_word((FB_BYTES + FB_PRESENT_REG) as u64, 0)
            .unwrap();
        assert_eq!(&fb.pixels()[0..4], &[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(&fb.pixels()[4..8], &[0x00, 0x00, 0x00, 0xFF]);
    }

    #[test]
    fn single_buffer_is_the_default() {
        let mut fb = Framebuffer::new();
        // Without enabling double buffering, draws are visible immediately.
        fb.write_word(0, 0xFFFF_FFFF).unwrap();
        assert_eq!(&fb.pixels()[0..4], &[0xFF, 0xFF, 0xFF, 0xFF]);
    }
}
