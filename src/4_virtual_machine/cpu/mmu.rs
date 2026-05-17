//! Sv39 MMU implementation for RV64 virtual memory.
//!
//! Hardware-accurate page-table walk:
//!   - Canonical address check (bits [63:39] must sign-extend bit 38)
//!   - U-bit and SUM-bit permission enforcement
//!   - MXR: executable pages become readable when MXR=1
//!   - Superpage support (1 GB / 2 MB) with alignment check
//!   - A/D bit maintenance: A set on any access, D set on stores

use crate::virtual_machine::cpu::registers::PrivilegeMode;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

const VPN_BITS: u64 = 9;
const PTE_SIZE: u64 = 8;
const LEVELS: usize = 3;

/// Translate a virtual address to a physical address using Sv39 page tables.
///
/// `mstatus` is needed for the SUM (bit 18) and MXR (bit 19) bits.
pub fn translate(
    vaddr: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
    bus: &mut impl MemoryAccess,
    is_write: bool,
    is_execute: bool,
) -> Result<u64, VmError> {
    let mode = (satp >> 60) & 0xF;
    let ppn = satp & 0x0000_0FFF_FFFF_FFFF;

    // Bare mode or M-mode: identity mapping (no translation).
    if mode == 0 || priv_mode == PrivilegeMode::Machine {
        return Ok(vaddr);
    }

    // Only Sv39 (mode = 8) is supported.
    if mode != 8 {
        return Err(VmError::PageFault(vaddr));
    }

    // Sv39 canonical check: bits [63:39] must all equal bit 38 (sign extension).
    // vaddr >> 39 gives the 25 upper bits; valid values are 0 (all-zero) or
    // 0x1FF_FFFF (all-one, i.e. 25 ones = 2^25 - 1).
    let upper = vaddr >> 39;
    if upper != 0 && upper != 0x1FF_FFFF {
        return Err(VmError::PageFault(vaddr));
    }

    let sum = (mstatus >> 18) & 1; // Supervisor User Memory access bit
    let mxr = (mstatus >> 19) & 1; // Make eXecutable Readable bit

    let mut current_ppn = ppn;

    for level in (0..LEVELS).rev() {
        let vpn_shift = 12 + level as u64 * VPN_BITS;
        let vpn = (vaddr >> vpn_shift) & 0x1FF;
        let pte_addr = (current_ppn << 12) | (vpn * PTE_SIZE);

        let pte = bus.read_doubleword(pte_addr).map_err(|_| VmError::PageFault(vaddr))?;

        // Valid bit must be set.
        if pte & 0x1 == 0 {
            return Err(VmError::PageFault(vaddr));
        }

        // Reserved bits [63:54] must be zero.
        if (pte >> 54) != 0 {
            return Err(VmError::PageFault(vaddr));
        }

        let r = (pte >> 1) & 1;
        let w = (pte >> 2) & 1;
        let x = (pte >> 3) & 1;
        let u_bit = (pte >> 4) & 1;

        if r == 1 || x == 1 {
            // ---- Leaf PTE ----

            // Permission checks.
            if is_execute && x == 0 {
                return Err(VmError::InstructionAccessFault(vaddr));
            }
            if is_write && w == 0 {
                return Err(VmError::StoreAccessFault(vaddr));
            }
            if !is_write && !is_execute && r == 0 {
                // MXR=1: executable pages are also readable.
                if mxr == 0 || x == 0 {
                    return Err(VmError::LoadAccessFault(vaddr));
                }
            }

            // U-bit / SUM privilege checks.
            match priv_mode {
                PrivilegeMode::User => {
                    if u_bit == 0 {
                        return Err(VmError::PageFault(vaddr));
                    }
                }
                PrivilegeMode::Supervisor => {
                    // S-mode cannot access U-mode pages unless SUM=1.
                    if u_bit == 1 && sum == 0 {
                        return Err(VmError::PageFault(vaddr));
                    }
                }
                PrivilegeMode::Machine => {}
            }

            // Superpage alignment: lower PPN bits must be zero.
            if level > 0 {
                let lower_mask = (1u64 << (level as u64 * VPN_BITS)) - 1;
                if ((pte >> 10) & lower_mask) != 0 {
                    return Err(VmError::PageFault(vaddr));
                }
            }

            // Maintain A/D bits (set A on any access, D on writes).
            let mut new_pte = pte | (1u64 << 6); // A bit
            if is_write {
                new_pte |= 1u64 << 7; // D bit
            }
            if new_pte != pte {
                // If we cannot update the PTE, raise a page fault.
                bus.write_doubleword(pte_addr, new_pte)
                    .map_err(|_| VmError::PageFault(vaddr))?;
            }

            // Physical address construction.
            // For superpages (level > 0) the lower VPN bits substitute for the
            // lower PPN bits in the physical address.
            let lower_ppn_bits = level as u64 * VPN_BITS;
            let offset_bits = 12 + lower_ppn_bits;
            let page_ppn = (pte >> 10) & 0x0000_0FFF_FFFF_FFFF;
            let super_offset = vaddr & ((1u64 << offset_bits) - 1);
            let phys_addr = ((page_ppn >> lower_ppn_bits) << offset_bits) | super_offset;

            return Ok(phys_addr);
        } else {
            // Non-leaf PTE: W=1 is reserved for non-leaf entries.
            if w == 1 {
                return Err(VmError::PageFault(vaddr));
            }
            current_ppn = (pte >> 10) & 0x0000_0FFF_FFFF_FFFF;
        }
    }

    Err(VmError::PageFault(vaddr))
}
