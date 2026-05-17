//! Instruction fetch stage.

use crate::virtual_machine::bus::SystemBus;
use crate::virtual_machine::cpu::mmu;
use crate::virtual_machine::cpu::registers::PrivilegeMode;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

/// Fetch the 32-bit instruction word at `pc`.
/// Returns `InstructionAccessFault` on misalignment or bus error.
pub fn fetch(
    bus: &mut SystemBus,
    pc: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
) -> Result<u32, VmError> {
    if pc & 0x3 != 0 {
        return Err(VmError::InstructionAccessFault(pc));
    }

    let phys_addr = mmu::translate(pc, satp, priv_mode, mstatus, bus, false, true)?;

    bus.read_word(phys_addr)
        .map_err(|_| VmError::InstructionAccessFault(pc))
}
