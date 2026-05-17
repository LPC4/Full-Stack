//! Trap and interrupt handling for the RISC-V CPU.
//!
//! Implements the RISC-V privilege specification for:
//! - Synchronous exceptions (illegal instructions, access faults, breakpoints)
//! - Asynchronous interrupts (timer, software, external)
//! - Trap entry: saves state, updates CSRs, jumps to handler (with medeleg/mideleg delegation)
//! - Trap return: MRET restores M-mode state; SRET properly restores SPP/SPIE

use crate::virtual_machine::cpu::csr::CsrFile;
use crate::virtual_machine::cpu::registers::{PrivilegeMode, Registers};
use crate::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// mcause constants
// ---------------------------------------------------------------------------

/// Instruction access fault
pub const CAUSE_INSN_ACCESS_FAULT: u64 = 1;
/// Illegal instruction
pub const CAUSE_ILLEGAL_INSN: u64 = 2;
/// Breakpoint (EBREAK)
pub const CAUSE_EBREAK: u64 = 3;
/// Load access fault
pub const CAUSE_LOAD_ACCESS_FAULT: u64 = 5;
/// Store/AMO access fault
pub const CAUSE_STORE_ACCESS_FAULT: u64 = 7;
/// Environment call from U-mode
pub const CAUSE_ECALL_U: u64 = 8;
/// Environment call from S-mode
pub const CAUSE_ECALL_S: u64 = 9;
/// Environment call from M-mode
pub const CAUSE_ECALL_M: u64 = 11;
/// Instruction page fault
#[allow(dead_code)]
pub const CAUSE_PAGE_FAULT_INST: u64 = 12;
/// Load page fault
pub const CAUSE_PAGE_FAULT_LOAD: u64 = 13;
/// Store/AMO page fault
#[allow(dead_code)]
pub const CAUSE_PAGE_FAULT_STORE: u64 = 15;

// Interrupt causes have bit 63 set.
/// Machine software interrupt
pub const CAUSE_M_SOFTWARE_IRQ: u64 = (1u64 << 63) | 3;
/// Machine timer interrupt
pub const CAUSE_M_TIMER_IRQ: u64 = (1u64 << 63) | 7;
/// Machine external interrupt
pub const CAUSE_M_EXTERNAL_IRQ: u64 = (1u64 << 63) | 11;

// ---------------------------------------------------------------------------
// Trap entry
// ---------------------------------------------------------------------------

/// Enter a trap: saves state, updates CSRs, jumps to the appropriate handler.
///
/// Delegation: if the current privilege is U or S and the corresponding bit in
/// medeleg (exceptions) or mideleg (interrupts) is set, the trap is delivered
/// to S-mode using sepc/scause/stval/sstatus/stvec.  Otherwise it goes to M-mode.
///
/// Returns the new PC (trap handler entry address).
pub fn take_trap(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    cause: u64,
    tval: u64,
    pc: u64,
) -> u64 {
    let current_priv = regs.priv_mode;
    let is_interrupt = (cause & (1u64 << 63)) != 0;
    let cause_idx = cause & !(1u64 << 63);

    // Determine whether to delegate to S-mode.
    // M-mode traps always stay in M-mode.
    let delegate = match current_priv {
        PrivilegeMode::User | PrivilegeMode::Supervisor => {
            if is_interrupt {
                (csrs.mideleg >> cause_idx) & 1 == 1
            } else {
                (csrs.medeleg >> cause_idx) & 1 == 1
            }
        }
        PrivilegeMode::Machine => false,
    };

    if delegate {
        // Deliver to S-mode.
        csrs.sepc = pc & !0x3u64;
        csrs.scause = cause;
        csrs.stval = tval;

        // sstatus bits live inside mstatus:
        //   SPIE = old SIE; SIE = 0; SPP = current_priv (0=U, 1=S)
        let sie = (csrs.mstatus >> 1) & 1;
        csrs.mstatus = (csrs.mstatus & !(1u64 << 5)) | (sie << 5); // SPIE = old SIE
        csrs.mstatus &= !(1u64 << 1);                               // SIE = 0
        let spp: u64 = if current_priv == PrivilegeMode::User { 0 } else { 1 };
        csrs.mstatus = (csrs.mstatus & !(1u64 << 8)) | (spp << 8); // SPP = current_priv

        regs.priv_mode = PrivilegeMode::Supervisor;

        // Jump to stvec (direct or vectored).
        let stvec = csrs.stvec;
        let base = stvec & !0x3u64;
        if (stvec & 0x3) == 1 && is_interrupt {
            base + 4 * cause_idx
        } else {
            base
        }
    } else {
        // Deliver to M-mode.
        csrs.mepc = pc & !0x3u64;
        csrs.mcause = cause;
        csrs.mtval = tval;

        // mstatus: MPIE = old MIE; MIE = 0; MPP = current_priv
        let mie = (csrs.mstatus >> 3) & 1;
        csrs.mstatus = (csrs.mstatus & !(1u64 << 7)) | (mie << 7); // MPIE = old MIE
        csrs.mstatus &= !(1u64 << 3);                               // MIE = 0
        csrs.mstatus = (csrs.mstatus & !(0x3u64 << 11)) | ((current_priv as u64) << 11); // MPP

        regs.priv_mode = PrivilegeMode::Machine;

        // Jump to mtvec (direct or vectored).
        let mtvec = csrs.mtvec;
        let base = mtvec & !0x3u64;
        if (mtvec & 0x3) == 1 && is_interrupt {
            base + 4 * cause_idx
        } else {
            base
        }
    }
}

// ---------------------------------------------------------------------------
// Trap dispatch (error to trap mapping)
// ---------------------------------------------------------------------------

/// Map a `VmError` to a trap cause/tval pair.
/// Returns `Some((cause, tval))` if the error should trigger a trap,
/// or `None` if it is a fatal error that cannot be trapped.
pub fn error_to_trap_cause(e: &VmError) -> Option<(u64, u64)> {
    match e {
        VmError::InstructionAccessFault(addr) => Some((CAUSE_INSN_ACCESS_FAULT, *addr)),
        VmError::IllegalInstruction(insn) => Some((CAUSE_ILLEGAL_INSN, *insn as u64)),
        VmError::LoadAccessFault(addr) | VmError::BusError(addr) => {
            Some((CAUSE_LOAD_ACCESS_FAULT, *addr))
        }
        VmError::StoreAccessFault(addr) => Some((CAUSE_STORE_ACCESS_FAULT, *addr)),
        VmError::PageFault(addr) => Some((CAUSE_PAGE_FAULT_LOAD, *addr)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Trap return (MRET / SRET)
// ---------------------------------------------------------------------------

/// Handle MRET — return from machine-mode trap.
///
/// Restores MIE from MPIE, sets MPIE=1, restores privilege from MPP, sets MPP=User.
/// Returns the PC to jump to (MEPC).
pub fn handle_mret(regs: &mut Registers, csrs: &mut CsrFile) -> u64 {
    // MIE = old MPIE; MPIE = 1
    let mpie = (csrs.mstatus >> 7) & 1;
    csrs.mstatus = (csrs.mstatus & !(1u64 << 3)) | (mpie << 3); // MIE = old MPIE
    csrs.mstatus |= 1u64 << 7;                                   // MPIE = 1

    // Restore privilege from MPP [12:11], then set MPP = User (0).
    let mpp = (csrs.mstatus >> 11) & 0x3;
    let prev_priv = match mpp {
        0 => PrivilegeMode::User,
        1 => PrivilegeMode::Supervisor,
        3 => PrivilegeMode::Machine,
        _ => PrivilegeMode::Machine, // reserved value; treat as Machine
    };
    regs.priv_mode = prev_priv;
    csrs.mstatus &= !(0x3u64 << 11); // MPP = User

    // Clear MPRV if returning below M-mode.
    if prev_priv != PrivilegeMode::Machine {
        csrs.mstatus &= !(1u64 << 17);
    }

    csrs.mepc
}

/// Handle SRET — return from supervisor-mode trap.
///
/// Restores SIE from SPIE, sets SPIE=1, restores privilege from SPP, sets SPP=User.
/// Returns the PC to jump to (SEPC), or Err if SRET is illegal in the current mode.
pub fn handle_sret(regs: &mut Registers, csrs: &mut CsrFile) -> Result<u64, VmError> {
    // SRET is illegal from U-mode.
    if regs.priv_mode == PrivilegeMode::User {
        return Err(VmError::IllegalInstruction(0x1020_0073));
    }
    // TSR bit (mstatus[22]): if set while in S-mode, SRET raises illegal instruction.
    if regs.priv_mode == PrivilegeMode::Supervisor && (csrs.mstatus >> 22) & 1 == 1 {
        return Err(VmError::IllegalInstruction(0x1020_0073));
    }

    // SIE = old SPIE; SPIE = 1
    let spie = (csrs.mstatus >> 5) & 1;
    csrs.mstatus = (csrs.mstatus & !(1u64 << 1)) | (spie << 1); // SIE = old SPIE
    csrs.mstatus |= 1u64 << 5;                                   // SPIE = 1

    // Restore privilege from SPP (mstatus[8]), then set SPP = User (0).
    let spp = (csrs.mstatus >> 8) & 1;
    let prev_priv = if spp == 0 {
        PrivilegeMode::User
    } else {
        PrivilegeMode::Supervisor
    };
    regs.priv_mode = prev_priv;
    csrs.mstatus &= !(1u64 << 8); // SPP = User (0)

    // Clear MPRV (not returning to M-mode).
    csrs.mstatus &= !(1u64 << 17);

    Ok(csrs.sepc)
}
