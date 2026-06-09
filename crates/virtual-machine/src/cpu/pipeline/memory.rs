//! Memory access stage, resolves Load/Store/Atomic `ExecResults` against the system bus.

use crate::bus::SystemBus;
use crate::cpu::mmu;
use crate::cpu::pipeline::execute::ExecResult;
use crate::cpu::registers::PrivilegeMode;
use crate::error::VmError;
use crate::memory::MemoryAccess as _;

// --- Public result type ---

#[derive(Clone, Debug)]
pub enum MemResult {
    WriteInt {
        rd: usize,
        val: u64,
        next_pc: u64,
    },
    WriteFp {
        rd: usize,
        bits: u64,
        next_pc: u64,
    },
    WriteIntFlags {
        rd: usize,
        val: u64,
        fflags: u8,
        next_pc: u64,
    },
    WriteFpFlags {
        rd: usize,
        bits: u64,
        fflags: u8,
        next_pc: u64,
    },
    Jump {
        next_pc: u64,
    },
    Csr {
        funct3: u8,
        rd: usize,
        rs1_uimm: usize,
        csr: u16,
        operand: u64,
        old_val: u64,
        next_pc: u64,
    },
    Fence {
        next_pc: u64,
    },
    FenceI {
        next_pc: u64,
    },
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Wfi {
        next_pc: u64,
    },
}

// --- Public entry point ---

#[expect(clippy::too_many_arguments)]
pub fn memory_stage_with_pmp(
    result: ExecResult,
    bus: &mut SystemBus,
    reservation: &mut Option<u64>,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
    pmpcfg0: u64,
    pmpaddr0: u64,
    tlb: &mut mmu::Tlb,
) -> Result<MemResult, VmError> {
    // Note: extended signature accepts PMP CSRs; a backward-compatible
    // wrapper `memory_stage` is provided below for existing tests.
    match result {
        // Pass-through variants
        ExecResult::WriteInt { rd, val, next_pc } => Ok(MemResult::WriteInt { rd, val, next_pc }),
        ExecResult::WriteFp { rd, bits, next_pc } => Ok(MemResult::WriteFp { rd, bits, next_pc }),
        ExecResult::WriteIntFlags {
            rd,
            val,
            fflags,
            next_pc,
        } => Ok(MemResult::WriteIntFlags {
            rd,
            val,
            fflags,
            next_pc,
        }),
        ExecResult::WriteFpFlags {
            rd,
            bits,
            fflags,
            next_pc,
        } => Ok(MemResult::WriteFpFlags {
            rd,
            bits,
            fflags,
            next_pc,
        }),
        ExecResult::Jump { next_pc } => Ok(MemResult::Jump { next_pc }),
        ExecResult::Csr {
            funct3,
            rd,
            rs1_uimm,
            csr,
            operand,
            old_val,
            next_pc,
        } => Ok(MemResult::Csr {
            funct3,
            rd,
            rs1_uimm,
            csr,
            operand,
            old_val,
            next_pc,
        }),
        ExecResult::Fence { next_pc } => Ok(MemResult::Fence { next_pc }),
        ExecResult::FenceI { next_pc } => Ok(MemResult::FenceI { next_pc }),
        ExecResult::Ecall => Ok(MemResult::Ecall),
        ExecResult::Ebreak => Ok(MemResult::Ebreak),
        ExecResult::Mret => Ok(MemResult::Mret),
        ExecResult::Sret => Ok(MemResult::Sret),
        ExecResult::SfenceVma => Ok(MemResult::SfenceVma),
        ExecResult::Wfi { next_pc } => Ok(MemResult::Wfi { next_pc }),

        // Integer Load
        ExecResult::Load {
            rd,
            addr,
            funct3,
            next_pc,
        } => {
            let phys_addr = mmu::translate_with_pmp(
                addr, satp, priv_mode, mstatus, bus, false, false, pmpcfg0, pmpaddr0, tlb,
            )?;
            let val = load_int(bus, phys_addr, funct3)?;
            Ok(MemResult::WriteInt { rd, val, next_pc })
        }

        // Integer Store
        ExecResult::Store {
            addr,
            val,
            funct3,
            next_pc,
        } => {
            let phys_addr = mmu::translate_with_pmp(
                addr, satp, priv_mode, mstatus, bus, true, false, pmpcfg0, pmpaddr0, tlb,
            )?;
            store_int(bus, phys_addr, val, funct3)?;
            Ok(MemResult::Jump { next_pc })
        }

        // FP Load
        ExecResult::FLoad {
            rd,
            addr,
            funct3,
            next_pc,
        } => {
            let phys_addr = mmu::translate_with_pmp(
                addr, satp, priv_mode, mstatus, bus, false, false, pmpcfg0, pmpaddr0, tlb,
            )?;
            let bits = load_fp(bus, phys_addr, funct3)?;
            Ok(MemResult::WriteFp { rd, bits, next_pc })
        }

        // FP Store
        ExecResult::FStore {
            addr,
            bits,
            funct3,
            next_pc,
        } => {
            let phys_addr = mmu::translate_with_pmp(
                addr, satp, priv_mode, mstatus, bus, true, false, pmpcfg0, pmpaddr0, tlb,
            )?;
            store_fp(bus, phys_addr, bits, funct3)?;
            Ok(MemResult::Jump { next_pc })
        }

        // Atomic
        ExecResult::Atomic {
            funct5,
            aq: _aq,
            rl: _rl,
            funct3,
            rd,
            addr,
            val,
            next_pc,
        } => {
            let phys_addr = mmu::translate_with_pmp(
                addr, satp, priv_mode, mstatus, bus, true, false, pmpcfg0, pmpaddr0, tlb,
            )?;
            let result_val = handle_atomic(bus, reservation, funct5, funct3, rd, phys_addr, val)?;
            Ok(MemResult::WriteInt {
                rd,
                val: result_val,
                next_pc,
            })
        }
    }
}

/// Backwards-compatible wrapper for existing callers/tests that don't provide PMP CSRs.
/// Uses a throwaway TLB, so it never benefits from caching (test/diagnostic use).
pub fn memory_stage(
    result: ExecResult,
    bus: &mut SystemBus,
    reservation: &mut Option<u64>,
    satp: u64,
    priv_mode: PrivilegeMode,
    mstatus: u64,
) -> Result<MemResult, VmError> {
    let mut tlb = mmu::Tlb::new();
    memory_stage_with_pmp(
        result,
        bus,
        reservation,
        satp,
        priv_mode,
        mstatus,
        0,
        0,
        &mut tlb,
    )
}

// --- Integer load ---

fn load_int(bus: &mut SystemBus, addr: u64, funct3: u8) -> Result<u64, VmError> {
    match funct3 {
        0 => {
            let byte = bus
                .read_byte(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok((byte as i8) as i64 as u64)
        }
        1 => {
            let hw = bus
                .read_halfword(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok((hw as i16) as i64 as u64)
        }
        2 => {
            let w = bus
                .read_word(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok((w as i32) as i64 as u64)
        }
        3 => {
            let dw = bus
                .read_doubleword(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(dw)
        }
        4 => {
            let byte = bus
                .read_byte(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(byte as u64)
        }
        5 => {
            let hw = bus
                .read_halfword(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(hw as u64)
        }
        6 => {
            let w = bus
                .read_word(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(w as u64)
        }
        _ => Err(VmError::LoadAccessFault(addr)),
    }
}

// --- Integer store ---

fn store_int(bus: &mut SystemBus, addr: u64, val: u64, funct3: u8) -> Result<(), VmError> {
    match funct3 {
        0 => bus
            .write_byte(addr, val as u8)
            .map_err(|_| VmError::StoreAccessFault(addr)),
        1 => bus
            .write_halfword(addr, val as u16)
            .map_err(|_| VmError::StoreAccessFault(addr)),
        2 => bus
            .write_word(addr, val as u32)
            .map_err(|_| VmError::StoreAccessFault(addr)),
        3 => bus
            .write_doubleword(addr, val)
            .map_err(|_| VmError::StoreAccessFault(addr)),
        _ => Err(VmError::StoreAccessFault(addr)),
    }
}

// --- FP load ---

fn load_fp(bus: &mut SystemBus, addr: u64, funct3: u8) -> Result<u64, VmError> {
    match funct3 {
        2 => {
            // FLW: NaN-box
            let word = bus
                .read_word(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(0xFFFF_FFFF_0000_0000u64 | (word as u64))
        }
        3 => {
            // FLD
            let dw = bus
                .read_doubleword(addr)
                .map_err(|_| VmError::LoadAccessFault(addr))?;
            Ok(dw)
        }
        _ => Err(VmError::LoadAccessFault(addr)),
    }
}

// --- FP store ---

fn store_fp(bus: &mut SystemBus, addr: u64, bits: u64, funct3: u8) -> Result<(), VmError> {
    match funct3 {
        2 => {
            // FSW: lower 32 bits
            bus.write_word(addr, bits as u32)
                .map_err(|_| VmError::StoreAccessFault(addr))
        }
        3 => {
            // FSD
            bus.write_doubleword(addr, bits)
                .map_err(|_| VmError::StoreAccessFault(addr))
        }
        _ => Err(VmError::StoreAccessFault(addr)),
    }
}

// --- Atomics ---

fn handle_atomic(
    bus: &mut SystemBus,
    reservation: &mut Option<u64>,
    funct5: u8,
    funct3: u8,
    rd: usize,
    addr: u64,
    val: u64,
) -> Result<u64, VmError> {
    let is_word = funct3 == 2;

    // Natural alignment is required for all atomics: 4-byte for .W, 8-byte for .D.
    let align = if is_word { 4u64 } else { 8u64 };
    if !addr.is_multiple_of(align) {
        return Err(VmError::StoreAccessFault(addr));
    }

    match funct5 {
        // LR
        0x02 => {
            *reservation = Some(addr);
            let result = if is_word {
                let w = bus
                    .read_word(addr)
                    .map_err(|_| VmError::LoadAccessFault(addr))?;
                (w as i32) as i64 as u64
            } else {
                bus.read_doubleword(addr)
                    .map_err(|_| VmError::LoadAccessFault(addr))?
            };
            Ok(result)
        }
        // SC
        0x03 => {
            if *reservation == Some(addr) {
                *reservation = None;
                if is_word {
                    bus.write_word(addr, val as u32)
                        .map_err(|_| VmError::StoreAccessFault(addr))?;
                } else {
                    bus.write_doubleword(addr, val)
                        .map_err(|_| VmError::StoreAccessFault(addr))?;
                }
                Ok(0) // success
            } else {
                Ok(1) // failure
            }
        }
        // AMOs
        _ => {
            // Suppress unused rd warning, it's used by the caller
            let _ = rd;
            amo_op(bus, funct5, funct3, addr, val)
        }
    }
}

fn amo_op(
    bus: &mut SystemBus,
    funct5: u8,
    funct3: u8,
    addr: u64,
    val: u64,
) -> Result<u64, VmError> {
    let is_word = funct3 == 2;

    if is_word {
        // 32-bit AMO
        let old_w = bus
            .read_word(addr)
            .map_err(|_| VmError::LoadAccessFault(addr))?;
        let old_i = old_w as i32;
        let val_i = val as i32;

        let new_i: i32 = match funct5 {
            0x00 => old_i.wrapping_add(val_i), // AMOADD.W
            0x01 => val_i,                     // AMOSWAP.W
            0x04 => old_i ^ val_i,             // AMOXOR.W
            0x08 => old_i | val_i,             // AMOOR.W
            0x0C => old_i & val_i,             // AMOAND.W
            0x10 => old_i.min(val_i),          // AMOMIN.W
            0x14 => old_i.max(val_i),          // AMOMAX.W
            0x18 => {
                let old_u = old_w;
                let val_u = val as u32;
                old_u.min(val_u) as i32
            }
            0x1C => {
                let old_u = old_w;
                let val_u = val as u32;
                old_u.max(val_u) as i32
            }
            _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
        };

        bus.write_word(addr, new_i as u32)
            .map_err(|_| VmError::StoreAccessFault(addr))?;

        // Return old value sign-extended
        Ok(old_i as i64 as u64)
    } else {
        // 64-bit AMO
        let old = bus
            .read_doubleword(addr)
            .map_err(|_| VmError::LoadAccessFault(addr))?;
        let old_i = old as i64;
        let val_i = val as i64;

        let new_val: u64 = match funct5 {
            0x00 => old.wrapping_add(val),   // AMOADD.D
            0x01 => val,                     // AMOSWAP.D
            0x04 => old ^ val,               // AMOXOR.D
            0x08 => old | val,               // AMOOR.D
            0x0C => old & val,               // AMOAND.D
            0x10 => old_i.min(val_i) as u64, // AMOMIN.D
            0x14 => old_i.max(val_i) as u64, // AMOMAX.D
            0x18 => old.min(val),            // AMOMINU.D
            0x1C => old.max(val),            // AMOMAXU.D
            _ => return Err(VmError::IllegalInstruction(funct5 as u32)),
        };

        bus.write_doubleword(addr, new_val)
            .map_err(|_| VmError::StoreAccessFault(addr))?;

        Ok(old)
    }
}
