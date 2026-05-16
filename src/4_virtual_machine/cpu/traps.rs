//! Trap and interrupt handling for the RISC-V CPU.
//!
//! This module implements the RISC-V privilege specification for handling:
//! - Synchronous exceptions (illegal instructions, access faults, breakpoints)
//! - Asynchronous interrupts (timer, software, external)
//! - Trap entry (saving state, updating CSRs, jumping to handler)
//! - Trap return (MRET/SRET instructions)

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

/// Enter a trap: saves state, updates CSRs, jumps to handler.
/// This handles both M-mode and S-mode traps.
///
/// # Arguments
/// * `regs` - CPU registers (will be modified)
/// * `csrs` - CSR file (will be modified)
/// * `cause` - The trap cause (mcause value)
/// * `tval` - The trap value (mtval)
/// * `pc` - The PC where the trap occurred
///
/// # Returns
/// The new PC to jump to (trap handler address)
pub fn take_trap(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    cause: u64,
    tval: u64,
    pc: u64,
) -> u64 {
    let current_priv = regs.priv_mode;

    // Save exception PC (aligned to 4 bytes)
    csrs.mepc = pc & !0x3u64;
    csrs.mcause = cause;
    csrs.mtval = tval;

    // Save MIE -> MPIE; clear MIE
    let mie_bit = (csrs.mstatus >> 3) & 1;
    csrs.mstatus &= !(1u64 << 7); // clear MPIE
    csrs.mstatus |= mie_bit << 7; // MPIE = old MIE
    csrs.mstatus &= !(1u64 << 3); // clear MIE

    // Set MPP based on current privilege
    csrs.mstatus &= !(0x3u64 << 11);
    csrs.mstatus |= (current_priv as u64) << 11;

    // Jump to mtvec (with optional vectored mode)
    let mtvec = csrs.mtvec;
    let mode = mtvec & 0x3;
    let base = mtvec & !0x3u64;

    let trap_handler_pc = if mode == 1 && (cause & (1u64 << 63)) != 0 {
        // Vectored mode: base + 4 * cause_index
        let idx = cause & !(1u64 << 63);
        base + 4 * idx
    } else {
        // Direct mode: always jump to base
        base
    };

    // Switch to M-mode (all traps go to M-mode unless delegated)
    regs.priv_mode = PrivilegeMode::Machine;

    trap_handler_pc
}

// ---------------------------------------------------------------------------
// Trap dispatch (error to trap mapping)
// ---------------------------------------------------------------------------

/// Map a `VmError` to a trap cause/tval pair.
/// Returns `Some((cause, tval))` if the error should trigger a trap,
/// or `None` if it's a fatal error that can't be trapped.
pub fn error_to_trap_cause(e: &VmError) -> Option<(u64, u64)> {
    match e {
        VmError::InstructionAccessFault(addr) => Some((CAUSE_INSN_ACCESS_FAULT, *addr)),
        VmError::IllegalInstruction(insn) => Some((CAUSE_ILLEGAL_INSN, *insn as u64)),
        VmError::LoadAccessFault(addr) | VmError::BusError(addr) => {
            Some((CAUSE_LOAD_ACCESS_FAULT, *addr))
        }
        VmError::StoreAccessFault(addr) => Some((CAUSE_STORE_ACCESS_FAULT, *addr)),
        VmError::PageFault(addr) => {
            // Determine page fault type based on context
            // For simplicity, default to load page fault
            Some((CAUSE_PAGE_FAULT_LOAD, *addr))
        }
        _ => None, // Fatal errors that can't be trapped
    }
}

// ---------------------------------------------------------------------------
// Trap return (MRET/SRET)
// ---------------------------------------------------------------------------

/// Handle MRET instruction - return from machine-mode trap.
///
/// # Arguments
/// * `regs` - CPU registers (will be modified)
/// * `csrs` - CSR file (will be modified)
///
/// # Returns
/// The PC to jump to (MEPC value)
pub fn handle_mret(regs: &mut Registers, csrs: &mut CsrFile) -> u64 {
    // Restore MIE from MPIE
    let mpie = (csrs.mstatus >> 7) & 1;
    csrs.mstatus &= !(1u64 << 3); // clear MIE
    csrs.mstatus |= mpie << 3; // MIE = old MPIE

    // Set MPIE to 1
    csrs.mstatus |= 1u64 << 7;

    // Restore previous privilege mode from MPP
    let mpp = (csrs.mstatus >> 11) & 0x3;
    let prev_priv = match mpp {
        0 => PrivilegeMode::User,
        1 => PrivilegeMode::Supervisor,
        3 => PrivilegeMode::Machine,
        _ => PrivilegeMode::Machine, // default to M-mode
    };

    regs.priv_mode = prev_priv;

    // Set MPP to User mode (least privileged)
    csrs.mstatus &= !(0x3u64 << 11);
    csrs.mstatus |= 0u64 << 11;

    // Jump to MEPC
    csrs.mepc
}

/// Handle SRET instruction - return from supervisor-mode trap.
///
/// # Arguments
/// * `regs` - CPU registers (will be modified)
/// * `csrs` - CSR file (will be modified)
///
/// # Returns
/// The PC to jump to (SEPC value), or error if SRET is illegal
pub fn handle_sret(regs: &mut Registers, csrs: &mut CsrFile) -> Result<u64, VmError> {
    // Check if SRET is allowed (not in U-mode)
    if regs.priv_mode == PrivilegeMode::User {
        return Err(VmError::IllegalInstruction(0x102));
    }

    // For now, we'll implement basic SRET similar to MRET but using sstatus/sepc
    // In a full implementation, this would use sstatus fields

    // Restore previous privilege mode
    // For simplicity, assume returning to U-mode
    regs.priv_mode = PrivilegeMode::User;

    // Jump to SEPC
    Ok(csrs.sepc)
}

