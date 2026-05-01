//! Instruction fetch stage.

use crate::virtual_machine::bus::SystemBus;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

/// Fetch the 32-bit instruction word at `pc`.
/// Returns `InstructionAccessFault` on misalignment or bus error.
pub fn fetch(bus: &mut SystemBus, pc: u64) -> Result<u32, VmError> {
    if pc & 0x3 != 0 {
        return Err(VmError::InstructionAccessFault(pc));
    }
    bus.read_word(pc)
        .map_err(|_| VmError::InstructionAccessFault(pc))
}
