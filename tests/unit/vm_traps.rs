//! Tests for the trap handling module

#[cfg(test)]
mod tests {
    use full_stack::virtual_machine::cpu::csr::CsrFile;
    use full_stack::virtual_machine::cpu::registers::{PrivilegeMode, Registers};
    use full_stack::virtual_machine::cpu::traps;
    use full_stack::virtual_machine::error::VmError;

    #[test]
    fn test_take_trap_sets_csrs() {
        let mut regs = Registers::new();
        let mut csrs = CsrFile::new();
        regs.pc = 0x8000_0010;
        regs.priv_mode = PrivilegeMode::User;
        csrs.mtvec = 0x8000_0100;

        let new_pc = traps::take_trap(&mut regs, &mut csrs, traps::CAUSE_EBREAK, 0, 0x8000_0010);

        // Check CSRs are set correctly
        assert_eq!(csrs.mepc, 0x8000_0010);
        assert_eq!(csrs.mcause, traps::CAUSE_EBREAK);
        assert_eq!(csrs.mtval, 0);

        // Check privilege mode changed to Machine
        assert_eq!(regs.priv_mode, PrivilegeMode::Machine);

        // Check returned PC jumped to trap handler
        assert_eq!(new_pc, 0x8000_0100);
        // Note: regs.pc is NOT set by take_trap - caller must set it
    }

    #[test]
    fn test_take_trap_vectored_mode() {
        let mut regs = Registers::new();
        let mut csrs = CsrFile::new();
        regs.pc = 0x8000_0010;
        csrs.mtvec = 0x8000_0101; // bit 0 set = vectored mode

        let cause = traps::CAUSE_M_TIMER_IRQ;
        let new_pc = traps::take_trap(&mut regs, &mut csrs, cause, 0, 0x8000_0010);

        // In vectored mode with interrupt, should jump to base + 4 * index
        let expected = 0x8000_0100 + 4 * 7; // timer irq is cause 7
        assert_eq!(new_pc, expected);
    }

    #[test]
    fn test_handle_mret_restores_state() {
        let mut regs = Registers::new();
        let mut csrs = CsrFile::new();

        // Set up trap state
        csrs.mepc = 0x8000_0020;
        csrs.mstatus = (3u64 << 11) | (1u64 << 7); // MPP=Machine, MPIE=1

        let new_pc = traps::handle_mret(&mut regs, &mut csrs);

        // Should return MEPC value
        assert_eq!(new_pc, 0x8000_0020);
        // Note: regs.pc is NOT set by handle_mret - caller must set it

        // Should restore MIE from MPIE
        assert_eq!((csrs.mstatus >> 3) & 1, 1); // MIE should be 1
        assert_eq!((csrs.mstatus >> 7) & 1, 1); // MPIE should be 1
    }

    #[test]
    fn test_error_to_trap_cause() {
        // Test instruction access fault
        let err = VmError::InstructionAccessFault(0x1234);
        let (cause, tval) = traps::error_to_trap_cause(&err).unwrap();
        assert_eq!(cause, traps::CAUSE_INSN_ACCESS_FAULT);
        assert_eq!(tval, 0x1234);

        // Test illegal instruction
        let err = VmError::IllegalInstruction(0xDEAD);
        let (cause, tval) = traps::error_to_trap_cause(&err).unwrap();
        assert_eq!(cause, traps::CAUSE_ILLEGAL_INSN);
        assert_eq!(tval, 0xDEAD);

        // Test load access fault
        let err = VmError::LoadAccessFault(0x5678);
        let (cause, tval) = traps::error_to_trap_cause(&err).unwrap();
        assert_eq!(cause, traps::CAUSE_LOAD_ACCESS_FAULT);
        assert_eq!(tval, 0x5678);

        // Test that ecall cannot be trapped
        let err = VmError::Ecall;
        assert!(traps::error_to_trap_cause(&err).is_none());
    }
}
