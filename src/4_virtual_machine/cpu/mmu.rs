//! Sv39 MMU implementation for RV64 virtual memory.

use crate::virtual_machine::cpu::registers::PrivilegeMode;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

// Sv39 constants
const VPN_BITS: u64 = 9;
const PTE_SIZE: u64 = 8; // Each PTE is 8 bytes
const LEVELS: usize = 3; // 3 levels for Sv39

/// Translate a virtual address to a physical address using Sv39 page tables.
pub fn translate(
    vaddr: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    bus: &mut impl MemoryAccess,
    is_write: bool,
    is_execute: bool,
) -> Result<u64, VmError> {
    // Extract SATP fields
    let mode = (satp >> 60) & 0xF;
    let ppn = satp & 0x0000_0FFF_FFFF_FFFF; // 44-bit PPN for Sv39

    // In M-mode or Bare mode, use identity mapping
    if mode == 0 || priv_mode == PrivilegeMode::Machine {
        return Ok(vaddr);
    }

    // Only Sv39 supported (mode = 8)
    if mode != 8 {
        return Err(VmError::PageFault(vaddr));
    }

    // Validate that the virtual address is canonical (bits [63:39] must be equal)
    let upper_bits = vaddr >> 39;
    if upper_bits != 0 && upper_bits != 0x1_FFFF_FFFF {
        return Err(VmError::PageFault(vaddr));
    }

    // Walk the page table
    let mut current_ppn = ppn;

    for level in (0..LEVELS).rev() {
        // Extract VPN for this level
        let vpn_shift = 12 + level as u64 * VPN_BITS;
        let vpn = (vaddr >> vpn_shift) & 0x1FF;

        // Calculate PTE address
        let pte_addr = (current_ppn << 12) | (vpn * PTE_SIZE);

        // Read PTE from physical memory
        let pte = match bus.read_doubleword(pte_addr) {
            Ok(val) => val,
            Err(_) => return Err(VmError::PageFault(vaddr)),
        };

        // Check valid bit
        if pte & 0x1 == 0 {
            return Err(VmError::PageFault(vaddr));
        }

        // Check if this is a leaf PTE (R=1 or X=1)
        let r = (pte >> 1) & 1;
        let w = (pte >> 2) & 1;
        let x = (pte >> 3) & 1;

        if r == 1 || x == 1 {
            // This is a leaf PTE

            // Check permissions based on access type
            if is_execute && x == 0 {
                return Err(VmError::InstructionAccessFault(vaddr));
            }
            if is_write && w == 0 {
                return Err(VmError::StoreAccessFault(vaddr));
            }
            if !is_write && !is_execute && r == 0 {
                return Err(VmError::LoadAccessFault(vaddr));
            }

            // For supervisor mode, check U bit (bit 4)
            // If U=0, only accessible in S-mode or M-mode
            // If U=1, accessible in U-mode
            let u_bit = (pte >> 4) & 1;
            if priv_mode == PrivilegeMode::User && u_bit == 0 {
                return Err(VmError::PageFault(vaddr));
            }

            // Extract physical page number from PTE
            let page_ppn = (pte >> 10) & 0x0000_0FFF_FFFF_FFFF;

            // Calculate physical address
            let page_offset = vaddr & 0xFFF;
            let phys_addr = (page_ppn << 12) | page_offset;

            return Ok(phys_addr);
        } else {
            // This is a non-leaf PTE (pointer to next level)
            // R=0, W=0, X=0 for valid non-leaf entries
            if w == 1 {
                return Err(VmError::PageFault(vaddr));
            }

            // Move to next level
            current_ppn = (pte >> 10) & 0x0000_0FFF_FFFF_FFFF;
        }
    }

    // Should not reach here
    Err(VmError::PageFault(vaddr))
}
