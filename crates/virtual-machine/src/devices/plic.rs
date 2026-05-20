//! PLIC - Platform-Level Interrupt Controller.
//! Base address 0x0C00_0000.

use crate::error::VmError;
use crate::memory::MemoryAccess;

const MAX_SOURCES: usize = 32;
const MAX_CONTEXTS: usize = 1;

pub struct Plic {
    priority: [u32; MAX_SOURCES],
    pending: [u32; (MAX_SOURCES + 31) / 32],
    enable: [u32; (MAX_SOURCES + 31) / 32 * MAX_CONTEXTS],
    threshold: [u32; MAX_CONTEXTS],
    #[allow(dead_code)]
    claim_complete: [u32; MAX_CONTEXTS],
}

impl Plic {
    pub fn new() -> Self {
        Self {
            priority: [0; MAX_SOURCES],
            pending: [0; (MAX_SOURCES + 31) / 32],
            enable: [0; (MAX_SOURCES + 31) / 32 * MAX_CONTEXTS],
            threshold: [0; MAX_CONTEXTS],
            claim_complete: [0; MAX_CONTEXTS],
        }
    }

    pub fn set_irq(&mut self, source: u32) {
        if source > 0 && source <= MAX_SOURCES as u32 {
            let idx = (source as usize - 1) / 32;
            let bit = (source - 1) % 32;
            self.pending[idx] |= 1 << bit;
        }
    }

    pub fn clear_irq(&mut self, source: u32) {
        if source > 0 && source <= MAX_SOURCES as u32 {
            let idx = (source as usize - 1) / 32;
            let bit = (source - 1) % 32;
            self.pending[idx] &= !(1 << bit);
        }
    }

    pub fn next_irq(&self, hart: usize) -> Option<u32> {
        if hart >= MAX_CONTEXTS {
            return None;
        }
        let threshold = self.threshold[hart];
        let mut best_id = None;
        let mut best_prio = 0;
        for id in 1..=MAX_SOURCES as u32 {
            let prio = self.priority[id as usize - 1];
            if prio > threshold
                && prio > best_prio
                && self.is_pending(id)
                && self.is_enabled(hart, id)
            {
                best_prio = prio;
                best_id = Some(id);
            }
        }
        best_id
    }

    fn is_pending(&self, id: u32) -> bool {
        let idx = (id as usize - 1) / 32;
        let bit = (id - 1) % 32;
        (self.pending[idx] & (1 << bit)) != 0
    }

    fn is_enabled(&self, hart: usize, id: u32) -> bool {
        let base = hart * (MAX_SOURCES / 32);
        let idx = base + (id as usize - 1) / 32;
        let bit = (id - 1) % 32;
        (self.enable[idx] & (1 << bit)) != 0
    }
}

impl MemoryAccess for Plic {
    // Byte and halfword accesses are illegal.
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        Err(VmError::LoadAccessFault(addr))
    }

    fn write_byte(&mut self, _addr: u64, _data: u8) -> Result<(), VmError> {
        Err(VmError::StoreAccessFault(_addr))
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let offset = addr; // base already subtracted by caller
        match offset {
            0x0000..=0x007C => {
                let src_idx = (offset / 4) as usize;
                if src_idx < MAX_SOURCES {
                    Ok(self.priority[src_idx])
                } else {
                    Err(VmError::LoadAccessFault(addr))
                }
            }
            0x1000..=0x107C => {
                let idx = ((offset - 0x1000) / 4) as usize;
                if idx < self.pending.len() {
                    Ok(self.pending[idx])
                } else {
                    Err(VmError::LoadAccessFault(addr))
                }
            }
            0x2000..=0x207C => {
                let idx = ((offset - 0x2000) / 4) as usize;
                if idx < (MAX_SOURCES / 32) {
                    Ok(self.enable[idx])
                } else {
                    Err(VmError::LoadAccessFault(addr))
                }
            }
            0x200000 => Ok(self.threshold[0]),
            0x200004 => {
                if let Some(id) = self.next_irq(0) {
                    self.clear_irq(id);
                    Ok(id)
                } else {
                    Ok(0)
                }
            }
            _ => Err(VmError::LoadAccessFault(addr)),
        }
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        let offset = addr;
        match offset {
            0x0000..=0x007C => {
                let src_idx = (offset / 4) as usize;
                if src_idx < MAX_SOURCES {
                    self.priority[src_idx] = data;
                    Ok(())
                } else {
                    Err(VmError::StoreAccessFault(addr))
                }
            }
            0x2000..=0x207C => {
                let idx = ((offset - 0x2000) / 4) as usize;
                if idx < (MAX_SOURCES / 32) {
                    self.enable[idx] = data;
                    Ok(())
                } else {
                    Err(VmError::StoreAccessFault(addr))
                }
            }
            0x200000 => {
                self.threshold[0] = data;
                Ok(())
            }
            0x200004 => {
                // Writing to claim/complete completes an interrupt.
                Ok(())
            }
            _ => Err(VmError::StoreAccessFault(addr)),
        }
    }
}
