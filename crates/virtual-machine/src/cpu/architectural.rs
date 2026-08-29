//! Architecturally visible CPU state shared by execution models.

use crate::cpu::csr::CsrFile;
use crate::cpu::registers::{PrivilegeMode, Registers};

#[derive(Clone)]
pub struct ArchitecturalState {
    registers: Registers,
    csrs: CsrFile,
    reservation: Option<u64>,
}

impl ArchitecturalState {
    pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
        let mut registers = Registers::new();
        registers.pc = start_pc;
        registers.write_x(2, stack_ptr);

        let mut csrs = CsrFile::new();
        csrs.mtvec = crate::rom::M_TRAP_ADDR;
        csrs.mscratch = stack_ptr - 4096;

        Self {
            registers,
            csrs,
            reservation: None,
        }
    }

    pub fn registers(&self) -> &Registers {
        &self.registers
    }

    pub(crate) fn registers_mut(&mut self) -> &mut Registers {
        &mut self.registers
    }

    pub fn csrs(&self) -> &CsrFile {
        &self.csrs
    }

    pub(crate) fn csrs_mut(&mut self) -> &mut CsrFile {
        &mut self.csrs
    }

    pub(crate) fn registers_and_csrs_mut(&mut self) -> (&mut Registers, &mut CsrFile) {
        (&mut self.registers, &mut self.csrs)
    }

    pub(crate) fn reservation_mut(&mut self) -> &mut Option<u64> {
        &mut self.reservation
    }

    pub fn reservation(&self) -> Option<u64> {
        self.reservation
    }

    pub fn pc(&self) -> u64 {
        self.registers.pc
    }

    pub(crate) fn set_pc(&mut self, pc: u64) {
        self.registers.pc = pc;
    }

    pub fn privilege_mode(&self) -> PrivilegeMode {
        self.registers.priv_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_state_owns_registers_csrs_and_reservation() {
        let mut state = ArchitecturalState::new(0x8000_1000, 0x8800_0000);
        state.registers_mut().write_x(0, u64::MAX);

        assert_eq!(state.pc(), 0x8000_1000);
        assert_eq!(state.registers().read_x(0), 0);
        assert_eq!(state.registers().read_x(2), 0x8800_0000);
        assert_eq!(state.privilege_mode(), PrivilegeMode::Machine);
        assert_eq!(state.csrs().mtvec, crate::rom::M_TRAP_ADDR);
        assert_eq!(state.csrs().mscratch, 0x87FF_F000);
        assert_eq!(state.reservation, None);
    }
}
