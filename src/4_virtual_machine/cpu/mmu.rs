//! Identity MMU — passes physical addresses through unchanged.

use crate::virtual_machine::error::VmError;

/// Translate a virtual address to a physical address.
/// Currently identity mapping — no page tables implemented.
pub fn translate(addr: u64) -> Result<u64, VmError> {
    Ok(addr)
}
