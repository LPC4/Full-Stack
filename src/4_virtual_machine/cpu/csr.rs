//! Machine-mode Control and Status Register (CSR) file for the RV64 CPU.

use crate::virtual_machine::error::VmError;

/// Well-known CSR addresses.
pub mod addr {
    pub const FFLAGS: u16 = 0x001;
    pub const FRM: u16 = 0x002;
    pub const FCSR: u16 = 0x003;
    pub const CYCLE: u16 = 0xC00;
    pub const TIME: u16 = 0xC01;
    pub const INSTRET: u16 = 0xC02;

    // Supervisor-mode CSRs
    pub const SSTATUS: u16 = 0x100;
    pub const SIE: u16 = 0x104;
    pub const STVEC: u16 = 0x105;
    pub const SCOUNTEREN: u16 = 0x106;
    pub const SSCRATCH: u16 = 0x140;
    pub const SEPC: u16 = 0x141;
    pub const SCAUSE: u16 = 0x142;
    pub const STVAL: u16 = 0x143;
    pub const SIP: u16 = 0x144;
    pub const SATP: u16 = 0x180;

    // Machine-mode CSRs
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MIE: u16 = 0x304;
    pub const MTVEC: u16 = 0x305;
    pub const MCOUNTEREN: u16 = 0x306;
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
    pub const MIP: u16 = 0x344;
    pub const MHARTID: u16 = 0xF14;
}

pub struct CsrFile {
    // Machine-mode CSRs
    pub mstatus: u64,
    pub misa: u64,
    pub mie: u64,
    pub mtvec: u64,
    pub mscratch: u64,
    pub mepc: u64,
    pub mcause: u64,
    pub mtval: u64,
    pub mip: u64,

    // Supervisor-mode CSRs
    pub sstatus: u64,
    pub sie: u64,
    pub stvec: u64,
    pub sscratch: u64,
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
    pub sip: u64,
    pub satp: u64,

    // Performance counters
    pub cycle: u64,
    pub instret: u64,

    // Floating-point state
    pub fflags: u8,
    pub frm: u8,
}

impl CsrFile {
    pub fn new() -> Self {
        Self {
            // Machine-mode CSRs
            mstatus: 0,
            // RV64IMAFD: MXL=2 (bits 63:62=0b10), A(0)+D(3)+F(5)+I(8)+M(12) = 0x1129.
            misa: 0x8000_0000_0000_1129,
            mie: 0,
            mtvec: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip: 0,

            // Supervisor-mode CSRs
            sstatus: 0,
            sie: 0,
            stvec: 0,
            sscratch: 0,
            sepc: 0,
            scause: 0,
            stval: 0,
            sip: 0,
            satp: 0, // Bare mode by default

            // Performance counters
            cycle: 0,
            instret: 0,

            // Floating-point state
            fflags: 0,
            frm: 0,
        }
    }

    pub fn read(&self, addr: u16) -> Result<u64, VmError> {
        use addr::*;
        match addr {
            FFLAGS => Ok(u64::from(self.fflags)),
            FRM => Ok(u64::from(self.frm)),
            // FCSR = frm[7:5] | fflags[4:0]
            FCSR => Ok((u64::from(self.frm) << 5) | u64::from(self.fflags)),
            CYCLE => Ok(self.cycle),
            // TIME is aliased to the cycle counter in this implementation.
            TIME => Ok(self.cycle),
            INSTRET => Ok(self.instret),

            // Supervisor-mode CSRs
            SSTATUS => Ok(self.sstatus),
            SIE => Ok(self.sie),
            STVEC => Ok(self.stvec),
            SSCRATCH => Ok(self.sscratch),
            SEPC => Ok(self.sepc),
            SCAUSE => Ok(self.scause),
            STVAL => Ok(self.stval),
            SIP => Ok(self.sip),
            SATP => Ok(self.satp),

            // Machine-mode CSRs
            MSTATUS => Ok(self.mstatus),
            MISA => Ok(self.misa),
            MIE => Ok(self.mie),
            MTVEC => Ok(self.mtvec),
            MSCRATCH => Ok(self.mscratch),
            MEPC => Ok(self.mepc),
            MCAUSE => Ok(self.mcause),
            MTVAL => Ok(self.mtval),
            MIP => Ok(self.mip),
            MHARTID => Ok(0),
            _ => Err(VmError::IllegalInstruction(u32::from(addr))),
        }
    }

    pub fn write(&mut self, addr: u16, val: u64) -> Result<(), VmError> {
        use addr::*;
        match addr {
            FFLAGS => {
                self.fflags = (val & 0x1F) as u8;
            }
            FRM => {
                self.frm = (val & 0x07) as u8;
            }
            FCSR => {
                self.fflags = (val & 0x1F) as u8;
                self.frm = ((val >> 5) & 0x07) as u8;
            }
            // CYCLE, INSTRET, MHARTID, and MISA are read-only / WARL; writes are silently ignored.
            CYCLE | INSTRET | MHARTID | MISA => {}

            // Supervisor-mode CSRs
            SSTATUS => {
                self.sstatus = val;
            }
            SIE => {
                self.sie = val;
            }
            STVEC => {
                self.stvec = val;
            }
            SSCRATCH => {
                self.sscratch = val;
            }
            SEPC => {
                self.sepc = val & !0x3;
            }
            SCAUSE => {
                self.scause = val;
            }
            STVAL => {
                self.stval = val;
            }
            SIP => {
                self.sip = val;
            }
            SATP => {
                // For Sv39, mode must be 0 (Bare) or 8 (Sv39)
                // Mask off unsupported modes - only support Bare and Sv39 for now
                let mode = (val >> 60) & 0xF;
                if mode == 0 || mode == 8 {
                    self.satp = val;
                } else {
                    // Write with mode=0 (Bare) if unsupported mode is requested
                    self.satp = val & !(0xFu64 << 60);
                }
            }

            // Machine-mode CSRs
            MSTATUS => {
                self.mstatus = val;
            }
            MIE => {
                self.mie = val;
            }
            MTVEC => {
                self.mtvec = val;
            }
            MSCRATCH => {
                self.mscratch = val;
            }
            // MEPC must be aligned to 4 bytes; clear the lowest 2 bits.
            MEPC => {
                self.mepc = val & !0x3;
            }
            MCAUSE => {
                self.mcause = val;
            }
            MTVAL => {
                self.mtval = val;
            }
            MIP => {
                self.mip = val;
            }
            _ => return Err(VmError::IllegalInstruction(u32::from(addr))),
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
