//! Machine-mode Control and Status Register (CSR) file for the RV64 CPU.

use crate::error::VmError;

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
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
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

// Bits of mstatus that are visible via the sstatus alias:
//   SD(63) | MXR(19) | SUM(18) | XS(16:15) | FS(14:13) | SPP(8) | UBE(6) | SPIE(5) | SIE(1)
const SSTATUS_MASK: u64 = (1u64 << 63)
    | (1u64 << 19)
    | (1u64 << 18)
    | (3u64 << 15)
    | (3u64 << 13)
    | (1u64 << 8)
    | (1u64 << 6)
    | (1u64 << 5)
    | (1u64 << 1);

// Bits of mip/mie that are visible via sip/sie aliases:
//   SEIP(9) | STIP(5) | SSIP(1)
const S_IRQ_MASK: u64 = (1u64 << 9) | (1u64 << 5) | (1u64 << 1);

// Bits of mip that may be written by software (CSR instruction).
// MTIP(7) and MEIP(11) are hardware-only; MSIP(3), STIP(5), SEIP(9), SSIP(1) are writable.
const MIP_W_MASK: u64 = (1u64 << 1) | (1u64 << 3) | (1u64 << 5) | (1u64 << 9);

pub struct CsrFile {
    // Machine-mode CSRs
    pub mstatus: u64,
    pub misa: u64,
    pub medeleg: u64,
    pub mideleg: u64,
    pub mie: u64,
    pub mtvec: u64,
    pub mscratch: u64,
    pub mepc: u64,
    pub mcause: u64,
    pub mtval: u64,
    pub mip: u64,
    // Physical Memory Protection CSRs (simple storage; enforcement is not implemented)
    pub pmpcfg0: u64,
    pub pmpaddr0: u64,

    // Supervisor-mode CSRs (truly independent S-mode registers)
    // sstatus, sie, sip are views into mstatus, mie, mip - not stored separately.
    pub stvec: u64,
    pub sscratch: u64,
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
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
            mstatus: 0,
            // RV64IMAFD: MXL=2 (bits 63:62=0b10), A+D+F+I+M = 0x1129.
            misa: 0x8000_0000_0000_1129,
            medeleg: 0,
            mideleg: 0,
            mie: 0,
            mtvec: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip: 0,

            stvec: 0,
            sscratch: 0,
            sepc: 0,
            scause: 0,
            stval: 0,
            satp: 0, // Bare mode by default
            pmpcfg0: 0,
            pmpaddr0: 0,

            cycle: 0,
            instret: 0,

            fflags: 0,
            frm: 0,
        }
    }

    pub fn read(&self, addr: u16) -> Result<u64, VmError> {
        use addr::{
            CYCLE, FCSR, FFLAGS, FRM, INSTRET, MCAUSE, MCOUNTEREN, MEDELEG, MEPC, MHARTID, MIDELEG,
            MIE, MIP, MISA, MSCRATCH, MSTATUS, MTVAL, MTVEC, SATP, SCAUSE, SCOUNTEREN, SEPC, SIE,
            SIP, SSCRATCH, SSTATUS, STVAL, STVEC, TIME,
        };
        match addr {
            FFLAGS => Ok(u64::from(self.fflags)),
            FRM => Ok(u64::from(self.frm)),
            FCSR => Ok((u64::from(self.frm) << 5) | u64::from(self.fflags)),
            CYCLE => Ok(self.cycle),
            // TIME is aliased to the cycle counter (no separate real-time clock).
            TIME => Ok(self.cycle),
            INSTRET => Ok(self.instret),

            // sstatus is the S-mode-visible subset of mstatus.
            SSTATUS => Ok(self.mstatus & SSTATUS_MASK),
            // sie is the S-mode-visible subset of mie.
            SIE => Ok(self.mie & S_IRQ_MASK),
            STVEC => Ok(self.stvec),
            // SCOUNTEREN: stub - all counters accessible from lower modes.
            SCOUNTEREN => Ok(0),
            SSCRATCH => Ok(self.sscratch),
            SEPC => Ok(self.sepc),
            SCAUSE => Ok(self.scause),
            STVAL => Ok(self.stval),
            // sip is the S-mode-visible subset of mip.
            SIP => Ok(self.mip & S_IRQ_MASK),
            SATP => Ok(self.satp),

            MSTATUS => Ok(self.mstatus),
            MISA => Ok(self.misa),
            MEDELEG => Ok(self.medeleg),
            MIDELEG => Ok(self.mideleg),
            // PMP CSRs
            0x3A0 => Ok(self.pmpcfg0),
            0x3B0 => Ok(self.pmpaddr0),
            MIE => Ok(self.mie),
            MTVEC => Ok(self.mtvec),
            // MCOUNTEREN: stub - counters accessible from S/U modes.
            MCOUNTEREN => Ok(0),
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
        use addr::{
            CYCLE, FCSR, FFLAGS, FRM, INSTRET, MCAUSE, MCOUNTEREN, MEDELEG, MEPC, MHARTID, MIDELEG,
            MIE, MIP, MISA, MSCRATCH, MSTATUS, MTVAL, MTVEC, SATP, SCAUSE, SCOUNTEREN, SEPC, SIE,
            SIP, SSCRATCH, SSTATUS, STVAL, STVEC,
        };
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
            // Read-only counters; writes silently ignored.
            CYCLE | INSTRET | MHARTID | MISA => {}

            // sstatus write updates only the S-mode-visible bits of mstatus.
            SSTATUS => {
                self.mstatus = (self.mstatus & !SSTATUS_MASK) | (val & SSTATUS_MASK);
            }
            // sie write updates only the S-mode-visible interrupt-enable bits of mie.
            SIE => {
                self.mie = (self.mie & !S_IRQ_MASK) | (val & S_IRQ_MASK);
            }
            STVEC => {
                self.stvec = val;
            }
            // SCOUNTEREN: stub, ignore writes.
            SCOUNTEREN => {}
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
            // sip write: only SSIP (bit 1) is software-writable from S-mode.
            SIP => {
                self.mip = (self.mip & !(1u64 << 1)) | (val & (1u64 << 1));
            }
            SATP => {
                let mode = (val >> 60) & 0xF;
                if mode == 0 || mode == 8 {
                    self.satp = val;
                } else {
                    // Unsupported mode: set Bare (mode=0) silently.
                    self.satp = val & !(0xFu64 << 60);
                }
            }

            MSTATUS => {
                // WARL: MPP [12:11] must be a legal privilege level (0=U, 1=S, 3=M).
                // Reserved value 2 is mapped to Machine (3) per QEMU convention.
                let mpp = (val >> 11) & 0x3;
                let mpp_legal = if mpp == 2 { 3u64 } else { mpp };
                self.mstatus = (val & !(0x3u64 << 11)) | (mpp_legal << 11);
            }
            MIE => {
                self.mie = val;
            }
            MTVEC => {
                self.mtvec = val;
            }
            // MCOUNTEREN: stub, ignore writes.
            MCOUNTEREN => {}
            MSCRATCH => {
                self.mscratch = val;
            }
            MEPC => {
                self.mepc = val & !0x3;
            }
            MCAUSE => {
                self.mcause = val;
            }
            MTVAL => {
                self.mtval = val;
            }
            // PMP CSRs: simple writable storage
            0x3A0 => {
                self.pmpcfg0 = val;
            }
            0x3B0 => {
                self.pmpaddr0 = val;
            }
            MIP => {
                // WARL: MTIP(7) and MEIP(11) are read-only (hardware-driven).
                // Only software-settable bits may be written.
                self.mip = (self.mip & !MIP_W_MASK) | (val & MIP_W_MASK);
            }
            MEDELEG => {
                self.medeleg = val;
            }
            MIDELEG => {
                self.mideleg = val;
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

/// Plain-data snapshot of all CSR values, suitable for cloning and passing to the UI.
#[derive(Clone, Debug, Default)]
pub struct CsrSnapshot {
    pub mstatus: u64,
    pub misa: u64,
    pub medeleg: u64,
    pub mideleg: u64,
    pub mie: u64,
    pub mtvec: u64,
    pub mscratch: u64,
    pub mepc: u64,
    pub mcause: u64,
    pub mtval: u64,
    pub mip: u64,
    // sstatus is derived from mstatus & SSTATUS_MASK for display.
    pub sstatus: u64,
    pub stvec: u64,
    pub sscratch: u64,
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
    pub satp: u64,
    pub pmpcfg0: u64,
    pub pmpaddr0: u64,
    pub cycle: u64,
    pub instret: u64,
    pub fflags: u8,
    pub frm: u8,
}

impl CsrFile {
    pub fn snapshot(&self) -> CsrSnapshot {
        CsrSnapshot {
            mstatus: self.mstatus,
            misa: self.misa,
            medeleg: self.medeleg,
            mideleg: self.mideleg,
            mie: self.mie,
            mtvec: self.mtvec,
            mscratch: self.mscratch,
            mepc: self.mepc,
            mcause: self.mcause,
            mtval: self.mtval,
            mip: self.mip,
            sstatus: self.mstatus & SSTATUS_MASK,
            stvec: self.stvec,
            sscratch: self.sscratch,
            sepc: self.sepc,
            scause: self.scause,
            stval: self.stval,
            satp: self.satp,
            pmpcfg0: self.pmpcfg0,
            pmpaddr0: self.pmpaddr0,
            cycle: self.cycle,
            instret: self.instret,
            fflags: self.fflags,
            frm: self.frm,
        }
    }
}
