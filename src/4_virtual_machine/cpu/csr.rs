//! Machine-mode Control and Status Register (CSR) file for the RV64 CPU.

use crate::virtual_machine::error::VmError;

/// Well-known CSR addresses.
pub mod addr {
    pub const FFLAGS: u16   = 0x001;
    pub const FRM: u16      = 0x002;
    pub const FCSR: u16     = 0x003;
    pub const CYCLE: u16    = 0xC00;
    pub const TIME: u16     = 0xC01;
    pub const INSTRET: u16  = 0xC02;
    pub const MSTATUS: u16  = 0x300;
    pub const MISA: u16     = 0x301;
    pub const MIE: u16      = 0x304;
    pub const MTVEC: u16    = 0x305;
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16     = 0x341;
    pub const MCAUSE: u16   = 0x342;
    pub const MTVAL: u16    = 0x343;
    pub const MIP: u16      = 0x344;
    pub const MHARTID: u16  = 0xF14;
}

pub struct CsrFile {
    pub mstatus:  u64,
    pub misa:     u64,
    pub mie:      u64,
    pub mtvec:    u64,
    pub mscratch: u64,
    pub mepc:     u64,
    pub mcause:   u64,
    pub mtval:    u64,
    pub mip:      u64,
    pub cycle:    u64,
    pub instret:  u64,
    pub fflags:   u8,
    pub frm:      u8,
}

impl CsrFile {
    pub fn new() -> Self {
        Self {
            mstatus:  0,
            // RV64IMAFD: MXL=2 (bits 63:62=0b10), A(0)+D(3)+F(5)+I(8)+M(12) = 0x1129.
            misa:     0x8000_0000_0000_1129,
            mie:      0,
            mtvec:    0,
            mscratch: 0,
            mepc:     0,
            mcause:   0,
            mtval:    0,
            mip:      0,
            cycle:    0,
            instret:  0,
            fflags:   0,
            frm:      0,
        }
    }

    pub fn read(&self, addr: u16) -> Result<u64, VmError> {
        use addr::*;
        match addr {
            FFLAGS   => Ok(u64::from(self.fflags)),
            FRM      => Ok(u64::from(self.frm)),
            // FCSR = frm[7:5] | fflags[4:0]
            FCSR     => Ok((u64::from(self.frm) << 5) | u64::from(self.fflags)),
            CYCLE    => Ok(self.cycle),
            // TIME is aliased to the cycle counter in this implementation.
            TIME     => Ok(self.cycle),
            INSTRET  => Ok(self.instret),
            MSTATUS  => Ok(self.mstatus),
            MISA     => Ok(self.misa),
            MIE      => Ok(self.mie),
            MTVEC    => Ok(self.mtvec),
            MSCRATCH => Ok(self.mscratch),
            MEPC     => Ok(self.mepc),
            MCAUSE   => Ok(self.mcause),
            MTVAL    => Ok(self.mtval),
            MIP      => Ok(self.mip),
            MHARTID  => Ok(0),
            _        => Err(VmError::IllegalInstruction(u32::from(addr))),
        }
    }

    pub fn write(&mut self, addr: u16, val: u64) -> Result<(), VmError> {
        use addr::*;
        match addr {
            FFLAGS  => { self.fflags = (val & 0x1F) as u8; }
            FRM     => { self.frm    = (val & 0x07) as u8; }
            FCSR    => {
                self.fflags = (val & 0x1F) as u8;
                self.frm    = ((val >> 5) & 0x07) as u8;
            }
            // CYCLE, INSTRET, MHARTID, and MISA are read-only / WARL; writes are silently ignored.
            CYCLE | INSTRET | MHARTID | MISA => {}
            MSTATUS  => { self.mstatus  = val; }
            MIE      => { self.mie      = val; }
            MTVEC    => { self.mtvec    = val; }
            MSCRATCH => { self.mscratch = val; }
            // MEPC must be aligned to 4 bytes; clear the lowest 2 bits.
            MEPC     => { self.mepc     = val & !0x3; }
            MCAUSE   => { self.mcause   = val; }
            MTVAL    => { self.mtval    = val; }
            MIP      => { self.mip      = val; }
            _        => return Err(VmError::IllegalInstruction(u32::from(addr))),
        }
        Ok(())
    }

    /// ORs `flags` into the accrued floating-point flags (fflags).
    pub fn accumulate_fflags(&mut self, flags: u8) {
        self.fflags |= flags & 0x1F;
    }

    pub fn rounding_mode(&self) -> u8 {
        self.frm
    }

    pub fn increment_instret(&mut self) {
        self.instret = self.instret.wrapping_add(1);
    }

    pub fn increment_cycle(&mut self) {
        self.cycle = self.cycle.wrapping_add(1);
    }
}

impl Default for CsrFile {
    fn default() -> Self {
        Self::new()
    }
}
