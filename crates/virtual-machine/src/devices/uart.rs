//! Simple memory-mapped UART (NS16550A subset).
//! Base address: `0x1000_0000` (typical RISC-V virt machine).

use crate::error::VmError;
use crate::memory::MemoryAccess;

/// UART registers (byte offsets from base)
pub mod reg {
    pub const RBR: u64 = 0; // Receiver Buffer (read)
    pub const THR: u64 = 0; // Transmitter Holding (write)
    pub const IER: u64 = 1; // Interrupt Enable
    pub const IIR: u64 = 2; // Interrupt Identification (read)
    pub const FCR: u64 = 2; // FIFO Control (write)
    pub const LCR: u64 = 3; // Line Control
    pub const MCR: u64 = 4; // Modem Control
    pub const LSR: u64 = 5; // Line Status
    pub const MSR: u64 = 6; // Modem Status
    pub const SCR: u64 = 7; // Scratch
}

pub struct Uart {
    ier: u8,
    iir: u8,
    fcr: u8,
    lcr: u8,
    mcr: u8,
    lsr: u8,
    msr: u8,
    scr: u8,
    pub rx_buf: std::collections::VecDeque<u8>,
    pub tx_out: Vec<u8>,
}

impl Default for Uart {
    fn default() -> Self {
        Self::new()
    }
}

/// PLIC source ID used for UART RX external interrupts.
pub const UART_RX_IRQ_SOURCE: u32 = 10;

impl Uart {
    pub fn new() -> Self {
        Self {
            ier: 0,
            iir: 0x01, // no interrupts pending
            fcr: 0,
            lcr: 0,
            mcr: 0,
            lsr: 0x60, // TX empty & TX holding empty
            msr: 0,
            scr: 0,
            rx_buf: std::collections::VecDeque::new(),
            tx_out: Vec::new(),
        }
    }

    pub fn receive(&mut self, byte: u8) {
        self.rx_buf.push_back(byte);
        self.lsr |= 0x01; // Data Ready
    }

    /// True when UART RX interrupt should be raised.
    pub fn rx_irq_pending(&self) -> bool {
        (self.ier & 0x01) != 0 && !self.rx_buf.is_empty()
    }

    pub fn drain_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx_out)
    }
}

impl MemoryAccess for Uart {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        let offset = addr % 8;
        match offset {
            reg::RBR => {
                if let Some(byte) = self.rx_buf.pop_front() {
                    if self.rx_buf.is_empty() {
                        self.lsr &= !0x01;
                    }
                    Ok(byte)
                } else {
                    Ok(0)
                }
            }
            reg::IER => Ok(self.ier),
            reg::IIR => Ok(self.iir),
            reg::LCR => Ok(self.lcr),
            reg::MCR => Ok(self.mcr),
            reg::LSR => Ok(self.lsr),
            reg::MSR => Ok(self.msr),
            reg::SCR => Ok(self.scr),
            _ => Err(VmError::BusError(addr)),
        }
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        let offset = addr % 8;
        match offset {
            reg::THR => {
                self.tx_out.push(data);
                self.lsr |= 0x60;
            }
            reg::IER => self.ier = data,
            reg::FCR => self.fcr = data,
            reg::LCR => self.lcr = data,
            reg::MCR => self.mcr = data,
            reg::SCR => self.scr = data,
            _ => return Err(VmError::BusError(addr)),
        }
        Ok(())
    }
}
