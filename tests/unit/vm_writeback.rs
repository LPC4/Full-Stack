use full_stack::virtual_machine::cpu::pipeline::writeback::writeback;
use full_stack::virtual_machine::cpu::pipeline::memory::MemResult;
use full_stack::virtual_machine::cpu::registers::Registers;
use full_stack::virtual_machine::cpu::csr::{CsrFile, addr};
use full_stack::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// Writeback stage, integer register writes
// ---------------------------------------------------------------------------

#[test]
fn writeback_write_int() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::WriteInt {
        rd: 5,
        val: 0xDEAD_BEEF,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    assert_eq!(regs.read_x(5), 0xDEAD_BEEF);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_write_int_to_x0() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::WriteInt {
        rd: 0,
        val: 42,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // x0 must always be zero
    assert_eq!(regs.read_x(0), 0);
    assert_eq!(next_pc, 0x8000_0004);
}

// ---------------------------------------------------------------------------
// Writeback stage, floating-point register writes
// ---------------------------------------------------------------------------

#[test]
fn writeback_write_fp() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let bits = 0x4009_21FB_5444_2D18u64; // pi as f64
    
    let mem_result = MemResult::WriteFp {
        rd: 10,
        bits,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    assert_eq!(regs.read_f_bits(10), bits);
    assert_eq!(next_pc, 0x8000_0004);
}

// ---------------------------------------------------------------------------
// Writeback stage, operations with FP flags
// ---------------------------------------------------------------------------

#[test]
fn writeback_write_int_with_flags() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::WriteIntFlags {
        rd: 5,
        val: 42,
        fflags: 0x11, // NV + NX
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    assert_eq!(regs.read_x(5), 42);
    assert_eq!(csrs.fflags, 0x11);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_accumulate_fp_flags() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    // First operation sets NX
    let mem_result1 = MemResult::WriteFpFlags {
        rd: 0,
        bits: 0,
        fflags: 0x01,
        next_pc: 0x8000_0004,
    };
    writeback(mem_result1, &mut regs, &mut csrs).unwrap();
    assert_eq!(csrs.fflags, 0x01);
    
    // Second operation adds NV
    let mem_result2 = MemResult::WriteFpFlags {
        rd: 0,
        bits: 0,
        fflags: 0x10,
        next_pc: 0x8000_0008,
    };
    writeback(mem_result2, &mut regs, &mut csrs).unwrap();
    assert_eq!(csrs.fflags, 0x11); // NX | NV
}

// ---------------------------------------------------------------------------
// Writeback stage, CSR operations
// ---------------------------------------------------------------------------

#[test]
fn writeback_csrrw() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    // Set initial MSTATUS
    csrs.write(addr::MSTATUS, 0x1234).unwrap();
    
    let mem_result = MemResult::Csr {
        funct3: 1, // CSRRW
        rd: 5,
        rs1_uimm: 0,
        csr: addr::MSTATUS,
        operand: 0x5678,
        old_val: 0x1234,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // Old value written to rd
    assert_eq!(regs.read_x(5), 0x1234);
    // New value written to CSR
    assert_eq!(csrs.read(addr::MSTATUS).unwrap(), 0x5678);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_csrrs_set_bits() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    // Initial MIE = 0x100
    csrs.write(addr::MIE, 0x100).unwrap();
    
    let mem_result = MemResult::Csr {
        funct3: 2, // CSRRS
        rd: 5,
        rs1_uimm: 11, // a1 register (non-zero means do write)
        csr: addr::MIE,
        operand: 0x800, // Set MTIE bit
        old_val: 0x100,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // Old value returned
    assert_eq!(regs.read_x(5), 0x100);
    // Bits set: 0x100 | 0x800 = 0x900
    assert_eq!(csrs.read(addr::MIE).unwrap(), 0x900);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_csrrc_clear_bits() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    // Initial MIP = 0xF00
    csrs.write(addr::MIP, 0xF00).unwrap();
    
    let mem_result = MemResult::Csr {
        funct3: 3, // CSRRC
        rd: 5,
        rs1_uimm: 12, // a2 register (non-zero means do write)
        csr: addr::MIP,
        operand: 0x800, // Clear MTIP bit
        old_val: 0xF00,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // Old value returned
    assert_eq!(regs.read_x(5), 0xF00);
    // Bits cleared: 0xF00 & !0x800 = 0x700
    assert_eq!(csrs.read(addr::MIP).unwrap(), 0x700);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_csrrwi_immediate() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Csr {
        funct3: 5, // CSRRWI
        rd: 5,
        rs1_uimm: 7, // Immediate value
        csr: addr::FRM,
        operand: 7,
        old_val: 0,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // Old value (0) returned
    assert_eq!(regs.read_x(5), 0);
    // New value written
    assert_eq!(csrs.read(addr::FRM).unwrap(), 7);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_csrrsi_no_write_when_zero() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let initial_mie = csrs.read(addr::MIE).unwrap();
    
    // rs1_uimm = 0 means don't write
    let mem_result = MemResult::Csr {
        funct3: 6, // CSRRSI
        rd: 5,
        rs1_uimm: 0,
        csr: addr::MIE,
        operand: 0,
        old_val: initial_mie,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // CSR should be unchanged
    assert_eq!(csrs.read(addr::MIE).unwrap(), initial_mie);
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_csr_read_only() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Csr {
        funct3: 1, // CSRRW
        rd: 5,
        rs1_uimm: 0,
        csr: addr::MHARTID, // Read-only
        operand: 42,
        old_val: 0,
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    
    // Old value (0) returned
    assert_eq!(regs.read_x(5), 0);
    // MHARTID is still 0 (writes ignored)
    assert_eq!(csrs.read(addr::MHARTID).unwrap(), 0);
    assert_eq!(next_pc, 0x8000_0004);
}

// ---------------------------------------------------------------------------
// Writeback stage, special operations
// ---------------------------------------------------------------------------

#[test]
fn writeback_jump() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Jump {
        next_pc: 0x8000_0100,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    assert_eq!(next_pc, 0x8000_0100);
}

#[test]
fn writeback_fence() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Fence {
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_fencei() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::FenceI {
        next_pc: 0x8000_0004,
    };
    
    let next_pc = writeback(mem_result, &mut regs, &mut csrs).unwrap();
    assert_eq!(next_pc, 0x8000_0004);
}

#[test]
fn writeback_ecall_error() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Ecall;
    
    let result = writeback(mem_result, &mut regs, &mut csrs);
    assert!(matches!(result, Err(VmError::Ecall)));
}

#[test]
fn writeback_ebreak_error() {
    let mut regs = Registers::new();
    let mut csrs = CsrFile::new();
    
    let mem_result = MemResult::Ebreak;
    
    let result = writeback(mem_result, &mut regs, &mut csrs);
    assert!(matches!(result, Err(VmError::Ebreak)));
}
