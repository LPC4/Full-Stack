//! Unit tests for Sv39 MMU implementation

use virtual_machine::cpu::mmu;
use virtual_machine::cpu::registers::PrivilegeMode;
use virtual_machine::error::VmError;
use virtual_machine::memory::MemoryAccess;

/// Simple mock memory for testing.
/// Supports writes so the MMU can update A/D bits in PTEs.
struct MockMemory {
    data: std::collections::HashMap<u64, u8>,
}

impl MockMemory {
    fn new() -> Self {
        Self {
            data: std::collections::HashMap::new(),
        }
    }

    fn write_doubleword(&mut self, addr: u64, value: u64) {
        for i in 0..8 {
            self.data
                .insert(addr + i, ((value >> (i * 8)) & 0xFF) as u8);
        }
    }
}

impl MemoryAccess for MockMemory {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        self.data.get(&addr).copied().ok_or(VmError::BusError(addr))
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        let mut value = 0u16;
        for i in 0..2 {
            let byte = self.read_byte(addr + i)?;
            value |= (byte as u16) << (i * 8);
        }
        Ok(value)
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        let mut value = 0u32;
        for i in 0..4 {
            let byte = self.read_byte(addr + i)?;
            value |= (byte as u32) << (i * 8);
        }
        Ok(value)
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        let mut value = 0u64;
        for i in 0..8 {
            let byte = self.read_byte(addr + i)?;
            value |= (byte as u64) << (i * 8);
        }
        Ok(value)
    }

    // Writes are supported so the MMU can maintain A/D bits in PTEs.
    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), VmError> {
        self.data.insert(addr, value);
        Ok(())
    }

    fn write_halfword(&mut self, addr: u64, value: u16) -> Result<(), VmError> {
        self.write_byte(addr, value as u8)?;
        self.write_byte(addr + 1, (value >> 8) as u8)?;
        Ok(())
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), VmError> {
        for i in 0..4u64 {
            self.write_byte(addr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
        }
        Ok(())
    }

    fn write_doubleword(&mut self, addr: u64, value: u64) -> Result<(), VmError> {
        for i in 0..8u64 {
            self.write_byte(addr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
        }
        Ok(())
    }
}

#[test]
fn test_identity_mapping_in_m_mode() {
    let mut mem = MockMemory::new();

    // In M-mode, should use identity mapping regardless of SATP.
    let satp = 0; // Bare mode
    let result = mmu::translate(
        0x12345,
        satp,
        PrivilegeMode::Machine,
        0,
        &mut mem,
        false,
        false,
    );
    assert_eq!(result.unwrap(), 0x12345);
}

#[test]
fn test_identity_mapping_in_bare_mode() {
    let mut mem = MockMemory::new();

    // In S-mode with Bare mode (SATP.mode = 0), should use identity mapping.
    let satp = 0;
    let result = mmu::translate(
        0x12345,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        false,
    );
    assert_eq!(result.unwrap(), 0x12345);
}

#[test]
fn test_sv39_noncanonical_address_faults() {
    let mut mem = MockMemory::new();

    // Bit 38 is set, but the upper bits are not sign-extended, so this is not canonical.
    let vaddr = 0x0000_0040_0000_0000u64;
    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        vaddr,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        false,
    );

    assert!(matches!(result, Err(VmError::PageFault(addr)) if addr == vaddr));
}

#[test]
fn test_sv39_translation() {
    let mut mem = MockMemory::new();

    // Set up a simple 3-level page table for identity mapping.
    // Level 2 table at physical 0x1000
    // Level 1 table at physical 0x2000
    // Level 0 table at physical 0x3000

    let l2_pte = (0x2000 >> 12) << 10 | 0x1; // PPN=0x2, V=1
    mem.write_doubleword(0x1000, l2_pte);

    let l1_pte = (0x3000 >> 12) << 10 | 0x1; // PPN=0x3, V=1
    mem.write_doubleword(0x2000, l1_pte);

    // Leaf PTE: PPN=0, V|R|W|X|U|A|D=1 (A and D pre-set so no write-back needed)
    let l0_pte = (0x0 >> 12) << 10 | 0xEF;
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12); // MODE=8 (Sv39), PPN=1

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        false,
    );
    assert_eq!(result.unwrap(), 0x0);

    let result = mmu::translate(
        0x100,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        false,
    );
    assert_eq!(result.unwrap(), 0x100);
}

#[test]
fn test_sv39_permission_check_execute() {
    let mut mem = MockMemory::new();

    let l2_pte = (0x2000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x1000, l2_pte);
    let l1_pte = (0x3000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x2000, l1_pte);
    let l0_pte = (0x0 >> 12) << 10 | 0xEF; // X=1
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        true,
    );
    assert!(result.is_ok());
}

#[test]
fn test_sv39_permission_check_no_execute() {
    let mut mem = MockMemory::new();

    let l2_pte = (0x2000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x1000, l2_pte);
    let l1_pte = (0x3000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x2000, l1_pte);
    // R|U|A|D=1, X=0
    let l0_pte = (0x0 >> 12) << 10 | 0x73;
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        true,
    );
    assert!(matches!(result, Err(VmError::InstructionAccessFault(_))));
}

#[test]
fn test_sv39_permission_check_write() {
    let mut mem = MockMemory::new();

    let l2_pte = (0x2000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x1000, l2_pte);
    let l1_pte = (0x3000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x2000, l1_pte);
    let l0_pte = (0x0 >> 12) << 10 | 0xEF; // W=1
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        true,
        false,
    );
    assert!(result.is_ok());
}

#[test]
fn test_sv39_permission_check_no_write() {
    let mut mem = MockMemory::new();

    let l2_pte = (0x2000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x1000, l2_pte);
    let l1_pte = (0x3000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x2000, l1_pte);
    // R|U|A|D=1, W=0, X=0
    let l0_pte = (0x0 >> 12) << 10 | 0x73;
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        true,
        false,
    );
    assert!(matches!(result, Err(VmError::StoreAccessFault(_))));
}

#[test]
fn test_sv39_invalid_pte() {
    let mut mem = MockMemory::new();

    let l2_pte = (0x2000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x1000, l2_pte);
    let l1_pte = (0x3000 >> 12) << 10 | 0x1;
    mem.write_doubleword(0x2000, l1_pte);
    let l0_pte = 0x0; // V=0
    mem.write_doubleword(0x3000, l0_pte);

    let satp = (8u64 << 60) | (0x1000 >> 12);

    let result = mmu::translate(
        0x0,
        satp,
        PrivilegeMode::Supervisor,
        0,
        &mut mem,
        false,
        false,
    );
    assert!(matches!(result, Err(VmError::PageFault(_))));
}
