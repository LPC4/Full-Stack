//! Instruction fetch stage.

use crate::bus::SystemBus;
use crate::cpu::mmu;
use crate::cpu::registers::PrivilegeMode;
use crate::error::VmError;
use crate::memory::MemoryAccess as _;

/// Fetch the 32-bit instruction word at `pc`.
/// Returns `InstructionAccessFault` on misalignment or bus error.
#[expect(clippy::too_many_arguments)]
pub fn fetch_with_pmp(
    bus: &mut SystemBus,
    pc: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
    pmpcfg0: u64,
    pmpaddr0: u64,
    tlb: &mut mmu::Tlb,
) -> Result<u32, VmError> {
    if pc & 0x3 != 0 {
        return Err(VmError::InstructionAccessFault(pc));
    }

    let phys_addr = mmu::translate_with_pmp(
        pc, satp, priv_mode, mstatus, bus, false, true, pmpcfg0, pmpaddr0, tlb,
    )?;

    bus.read_word(phys_addr)
        .map_err(|_| VmError::InstructionAccessFault(pc))
}

/// Backwards-compatible wrapper: old callers without PMP info should use this.
/// Uses a throwaway TLB, so it never benefits from caching (test/diagnostic use).
pub fn fetch(
    bus: &mut SystemBus,
    pc: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
) -> Result<u32, VmError> {
    let mut tlb = mmu::Tlb::new();
    fetch_with_pmp(bus, pc, satp, priv_mode, mstatus, 0, 0, &mut tlb)
}
