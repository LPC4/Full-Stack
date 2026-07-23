//! Sv39 MMU implementation for RV64 virtual memory.
//!
//! Hardware-accurate page-table walk:
//!   - Canonical address check (bits [63:39] must sign-extend bit 38)
//!   - U-bit and SUM-bit permission enforcement
//!   - MXR: executable pages become readable when MXR=1
//!   - Superpage support (1 GB / 2 MB) with alignment check
//!   - A/D bit maintenance: A set on any access, D set on stores

use crate::cpu::registers::PrivilegeMode;
use crate::error::VmError;
use crate::memory::MemoryAccess;

const VPN_BITS: u64 = 9;
const PTE_SIZE: u64 = 8;
const LEVELS: usize = 3;

// Direct-mapped TLB size; covers the framebuffer (76 pages) plus code/stack.
const TLB_ENTRIES: usize = 256;

#[derive(Clone, Copy)]
pub struct TranslationContext {
    pub satp: u64,
    pub priv_mode: PrivilegeMode,
    pub mstatus: u64,
    pub pmpcfg0: u64,
    pub pmpaddr0: u64,
}

impl TranslationContext {
    pub const fn without_pmp(satp: u64, priv_mode: PrivilegeMode, mstatus: u64) -> Self {
        Self {
            satp,
            priv_mode,
            mstatus,
            pmpcfg0: 0,
            pmpaddr0: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub enum AccessKind {
    Load,
    Store,
    Execute,
}

impl AccessKind {
    const fn is_write(self) -> bool {
        matches!(self, Self::Store)
    }

    const fn is_execute(self) -> bool {
        matches!(self, Self::Execute)
    }

    const fn fault(self, vaddr: u64) -> VmError {
        match self {
            Self::Load => VmError::LoadAccessFault(vaddr),
            Self::Store => VmError::StoreAccessFault(vaddr),
            Self::Execute => VmError::InstructionAccessFault(vaddr),
        }
    }
}

#[inline]
pub(crate) fn is_sv39_canonical(vaddr: u64) -> bool {
    let upper_bits = vaddr >> 39;
    upper_bits == 0 || upper_bits == 0x1FF_FFFF
}

#[derive(Clone, Copy)]
struct TlbEntry {
    valid: bool,
    vpn: u64,
    // Leaf PTE as written back to memory (A set, D set for write inserts).
    pte: u64,
    // Leaf level, for superpage offset reconstruction.
    level: usize,
}

/// Direct-mapped cache of Sv39 leaf mappings, keyed by satp and flushed on
/// sfence.vma. Permissions are not cached: they are re-checked per access.
#[derive(Clone)]
pub struct Tlb {
    satp: u64,
    entries: [TlbEntry; TLB_ENTRIES],
}

impl Default for Tlb {
    fn default() -> Self {
        Self::new()
    }
}

impl Tlb {
    pub fn new() -> Self {
        Self {
            satp: 0,
            entries: [TlbEntry {
                valid: false,
                vpn: 0,
                pte: 0,
                level: 0,
            }; TLB_ENTRIES],
        }
    }

    /// Invalidate every entry. Called on sfence.vma.
    pub fn flush(&mut self) {
        for e in &mut self.entries {
            e.valid = false;
        }
    }

    // Drop all entries if the active address space (satp) changed.
    fn sync_satp(&mut self, satp: u64) {
        if self.satp != satp {
            self.satp = satp;
            self.flush();
        }
    }

    pub(crate) fn lookup(&self, vpn: u64) -> Option<(u64, usize)> {
        let e = &self.entries[(vpn as usize) & (TLB_ENTRIES - 1)];
        if e.valid && e.vpn == vpn {
            Some((e.pte, e.level))
        } else {
            None
        }
    }

    fn insert(&mut self, vpn: u64, pte: u64, level: usize) {
        self.entries[(vpn as usize) & (TLB_ENTRIES - 1)] = TlbEntry {
            valid: true,
            vpn,
            pte,
            level,
        };
    }
}

// Check a leaf PTE and build the physical address (permission/privilege/superpage/
// PMP). Shared by the walk and a TLB hit; touches no memory and no A/D bits.
pub(crate) fn finalize_leaf(
    pte: u64,
    vaddr: u64,
    level: usize,
    ctx: TranslationContext,
    access: AccessKind,
) -> Result<u64, VmError> {
    let sum = (ctx.mstatus >> 18) & 1;
    let mxr = (ctx.mstatus >> 19) & 1;
    let r = (pte >> 1) & 1;
    let w = (pte >> 2) & 1;
    let x = (pte >> 3) & 1;
    let u_bit = (pte >> 4) & 1;

    // Permission checks.
    if access.is_execute() && x == 0 {
        return Err(VmError::InstructionAccessFault(vaddr));
    }
    if access.is_write() && w == 0 {
        return Err(VmError::StoreAccessFault(vaddr));
    }
    if matches!(access, AccessKind::Load) && r == 0 {
        // MXR=1: executable pages are also readable.
        if mxr == 0 || x == 0 {
            return Err(VmError::LoadAccessFault(vaddr));
        }
    }

    // U-bit / SUM privilege checks.
    match ctx.priv_mode {
        PrivilegeMode::User => {
            if u_bit == 0 {
                return Err(VmError::PageFault(vaddr));
            }
        }
        PrivilegeMode::Supervisor => {
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

    // Physical address construction. For superpages the lower VPN bits substitute
    // for the lower PPN bits.
    let lower_ppn_bits = level as u64 * VPN_BITS;
    let offset_bits = 12 + lower_ppn_bits;
    let page_ppn = (pte >> 10) & 0x0000_0FFF_FFFF_FFFF;
    let super_offset = vaddr & ((1u64 << offset_bits) - 1);
    let phys_addr = ((page_ppn >> lower_ppn_bits) << offset_bits) | super_offset;

    // PMP enforcement (simple single-entry TOR-like semantics).
    if ctx.pmpcfg0 != 0 {
        let allow_r = (ctx.pmpcfg0 & 0x1) != 0;
        let allow_w = (ctx.pmpcfg0 & 0x2) != 0;
        let allow_x = (ctx.pmpcfg0 & 0x4) != 0;
        let pmp_match = ctx.pmpaddr0 == u64::MAX || phys_addr <= ctx.pmpaddr0;
        if pmp_match {
            if access.is_execute() && !allow_x {
                return Err(VmError::InstructionAccessFault(vaddr));
            }
            if access.is_write() && !allow_w {
                return Err(VmError::StoreAccessFault(vaddr));
            }
            if matches!(access, AccessKind::Load) && !allow_r {
                return Err(VmError::LoadAccessFault(vaddr));
            }
        } else {
            return Err(VmError::PageFault(vaddr));
        }
    }

    Ok(phys_addr)
}

/// Translate a virtual address through the Sv39 page tables. `ctx` carries
/// `mstatus` for the SUM (bit 18) and MXR (bit 19) bits.
pub fn translate_with_pmp(
    vaddr: u64,
    bus: &mut impl MemoryAccess,
    ctx: TranslationContext,
    access: AccessKind,
    tlb: &mut Tlb,
) -> Result<u64, VmError> {
    let mode = (ctx.satp >> 60) & 0xF;
    let ppn = ctx.satp & 0x0FFF_FFFF_FFFF; // Mask to 44 bits

    // M-mode bypasses address translation and PMP checks in this VM model.
    if ctx.priv_mode == PrivilegeMode::Machine {
        return Ok(vaddr);
    }

    if mode == 0 {
        let phys_addr = vaddr;
        if ctx.pmpcfg0 != 0 {
            let allow_r = (ctx.pmpcfg0 & 0x1) != 0;
            let allow_w = (ctx.pmpcfg0 & 0x2) != 0;
            let allow_x = (ctx.pmpcfg0 & 0x4) != 0;
            let pmp_match = ctx.pmpaddr0 == u64::MAX || phys_addr <= ctx.pmpaddr0;
            if pmp_match {
                if access.is_execute() && !allow_x {
                    return Err(VmError::InstructionAccessFault(vaddr));
                }
                if access.is_write() && !allow_w {
                    return Err(VmError::StoreAccessFault(vaddr));
                }
                if matches!(access, AccessKind::Load) && !allow_r {
                    return Err(VmError::LoadAccessFault(vaddr));
                }
            } else {
                return Err(access.fault(vaddr));
            }
        }
        return Ok(phys_addr);
    }

    // Only Sv39 (mode = 8) is supported.
    if mode != 8 {
        return Err(VmError::PageFault(vaddr));
    }

    // Sv39 canonical check: bits [63:39] must all equal bit 38 (sign extension).
    if !is_sv39_canonical(vaddr) {
        return Err(VmError::PageFault(vaddr));
    }

    // TLB fast path. A write to a page with a clean cached D bit re-walks so the
    // PTE's D bit gets set in memory.
    tlb.sync_satp(ctx.satp);
    let vpn_page = vaddr >> 12;
    if let Some((pte, level)) = tlb.lookup(vpn_page) {
        let needs_dirty = access.is_write() && ((pte >> 7) & 1) == 0;
        if !needs_dirty {
            return finalize_leaf(pte, vaddr, level, ctx, access);
        }
    }

    let mut current_ppn = ppn;

    for level in (0..LEVELS).rev() {
        let vpn_shift = 12 + level as u64 * VPN_BITS;
        let vpn = (vaddr >> vpn_shift) & 0x1FF;
        let pte_addr = (current_ppn << 12) | (vpn * PTE_SIZE);

        let pte = bus
            .read_doubleword(pte_addr)
            .map_err(|_| VmError::PageFault(vaddr))?;

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

        if r == 1 || x == 1 {
            // ---- Leaf PTE ----

            // Permission/privilege/superpage/PMP checks and phys construction.
            let phys_addr = finalize_leaf(pte, vaddr, level, ctx, access)?;

            // Maintain A/D bits (set A on any access, D on writes).
            let mut new_pte = pte | (1u64 << 6); // A bit
            if access.is_write() {
                new_pte |= 1u64 << 7; // D bit
            }
            if new_pte != pte {
                // If we cannot update the PTE, raise a page fault.
                bus.write_doubleword(pte_addr, new_pte)
                    .map_err(|_| VmError::PageFault(vaddr))?;
            }

            // Cache the post-update leaf so later accesses to this page skip the walk.
            tlb.insert(vaddr >> 12, new_pte, level);

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

/// Backwards-compatible wrapper keeping the original `translate()` signature.
/// Calls the extended translator with PMP disabled (pmpcfg0=0, pmpaddr0=0).
pub fn translate(
    vaddr: u64,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
    bus: &mut impl MemoryAccess,
    is_write: bool,
    is_execute: bool,
) -> Result<u64, VmError> {
    let mut tlb = Tlb::new();
    let ctx = TranslationContext::without_pmp(satp, priv_mode, mstatus);
    let access = if is_execute {
        AccessKind::Execute
    } else if is_write {
        AccessKind::Store
    } else {
        AccessKind::Load
    };
    translate_with_pmp(vaddr, bus, ctx, access, &mut tlb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::SystemBus;

    #[test]
    fn pmp_allows_supervisor_fetch_when_configured() {
        let mut bus = SystemBus::new(vec![]);
        let instr: u32 = 0x0000_0013; // addi x0,x0,0
        let _ = bus.write_word(crate::bus::RAM_BASE, instr);

        let ctx = TranslationContext {
            satp: 0,
            priv_mode: crate::cpu::registers::PrivilegeMode::Supervisor,
            mstatus: 0,
            pmpcfg0: 0x7,
            pmpaddr0: u64::MAX,
        };

        let mut tlb = Tlb::new();
        let fetched = crate::cpu::pipeline::fetch::fetch_with_pmp(
            &mut bus,
            crate::bus::RAM_BASE,
            ctx,
            &mut tlb,
        )
        .expect("fetch should succeed");

        assert_eq!(fetched, instr);
    }

    #[test]
    fn pmp_blocks_execute_when_x_clear() {
        let mut bus = SystemBus::new(vec![]);
        let instr: u32 = 0x0000_0013; // addi x0,x0,0
        let _ = bus.write_word(crate::bus::RAM_BASE, instr);

        let ctx = TranslationContext {
            satp: 0,
            priv_mode: crate::cpu::registers::PrivilegeMode::Supervisor,
            mstatus: 0,
            pmpcfg0: 0x3,
            pmpaddr0: u64::MAX,
        };

        let mut tlb = Tlb::new();
        let res = crate::cpu::pipeline::fetch::fetch_with_pmp(
            &mut bus,
            crate::bus::RAM_BASE,
            ctx,
            &mut tlb,
        );

        assert!(matches!(
            res,
            Err(crate::error::VmError::InstructionAccessFault(_))
        ));
    }
}
