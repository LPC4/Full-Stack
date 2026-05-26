//! Writeback stage -- commits results to registers and CSRs, returns next PC.

use crate::cpu::csr::CsrFile;
use crate::cpu::pipeline::memory::MemResult;
use crate::cpu::registers::Registers;
use crate::error::VmError;

pub fn writeback(
    result: MemResult,
    regs: &mut Registers,
    csrs: &mut CsrFile,
) -> Result<u64, VmError> {
    match result {
        MemResult::WriteInt { rd, val, next_pc } => {
            regs.write_x(rd, val);
            Ok(next_pc)
        }
        MemResult::WriteFp { rd, bits, next_pc } => {
            regs.write_f_bits(rd, bits);
            Ok(next_pc)
        }
        MemResult::WriteIntFlags {
            rd,
            val,
            fflags,
            next_pc,
        } => {
            csrs.accumulate_fflags(fflags);
            regs.write_x(rd, val);
            Ok(next_pc)
        }
        MemResult::WriteFpFlags {
            rd,
            bits,
            fflags,
            next_pc,
        } => {
            csrs.accumulate_fflags(fflags);
            regs.write_f_bits(rd, bits);
            Ok(next_pc)
        }
        MemResult::Jump { next_pc } => Ok(next_pc),
        MemResult::Fence { next_pc } => Ok(next_pc),
        MemResult::FenceI { next_pc } => Ok(next_pc),
        MemResult::Csr {
            funct3,
            rd,
            rs1_uimm,
            csr,
            operand,
            old_val: _,
            next_pc,
        } => {
            // Re-read CSR at WB time for architectural correctness (e.g. instret
            // increments between EX and WB; old_val is used only for forwarding).
            let old = csrs.read(csr)?;

            let (new_val, do_write) = match funct3 {
                1 => (operand, true),                 // CSRRW
                2 => (old | operand, rs1_uimm != 0),  // CSRRS
                3 => (old & !operand, rs1_uimm != 0), // CSRRC
                5 => (operand, true),                 // CSRRWI
                6 => (old | operand, rs1_uimm != 0),  // CSRRSI
                7 => (old & !operand, rs1_uimm != 0), // CSRRCI
                _ => return Err(VmError::IllegalInstruction(funct3 as u32)),
            };

            if do_write {
                csrs.write(csr, new_val)?;
            }

            // Always write old (WB-time) value to rd
            regs.write_x(rd, old);
            Ok(next_pc)
        }
        MemResult::Ecall => Err(VmError::Ecall),
        MemResult::Ebreak => Err(VmError::Ebreak),
        MemResult::Mret => {
            // MRET is handled specially in the CPU - restore state and jump to mepc
            Err(VmError::Mret)
        }
        MemResult::Sret => {
            // SRET is handled specially in the CPU
            Err(VmError::Sret)
        }
        MemResult::SfenceVma => {
            // SFENCE.VMA is a no-op in our simple implementation (no TLB)
            // Just advance PC by 4
            let pc = regs.pc;
            Ok(pc.wrapping_add(4))
        }
        MemResult::Wfi { next_pc } => {
            // WFI is a no-op in our simple implementation
            Ok(next_pc)
        }
    }
}
