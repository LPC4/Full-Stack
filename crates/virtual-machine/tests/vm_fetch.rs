use virtual_machine::bus::SystemBus;
use virtual_machine::cpu::PrivilegeMode;
use virtual_machine::cpu::pipeline::fetch::fetch;
use virtual_machine::error::VmError;
use virtual_machine::memory::MemoryAccess as _;

// --- Fetch stage, alignment checks ---

#[test]
fn fetch_aligned_word() {
    let mut bus = SystemBus::new(Vec::new());

    // Write a valid instruction at aligned address
    bus.write_byte(0x8000_0000, 0x13)
        .expect("bus write should succeed");
    bus.write_byte(0x8000_0001, 0x00)
        .expect("bus write should succeed");
    bus.write_byte(0x8000_0002, 0x00)
        .expect("bus write should succeed");
    bus.write_byte(0x8000_0003, 0x00)
        .expect("bus write should succeed"); // addi x0, x0, 0 (nop)

    let result = fetch(&mut bus, 0x8000_0000, 0, PrivilegeMode::Machine, 0);
    assert!(result.is_ok());
    assert_eq!(
        result.expect("instruction fetch should succeed"),
        0x0000_0013
    );
}

#[test]
fn fetch_misaligned_halfword() {
    let mut bus = SystemBus::new(Vec::new());

    // PC not aligned to 4 bytes
    let result = fetch(&mut bus, 0x8000_0002, 0, PrivilegeMode::Machine, 0);
    assert!(matches!(
        result,
        Err(VmError::InstructionAccessFault(0x8000_0002))
    ));
}

#[test]
fn fetch_misaligned_byte() {
    let mut bus = SystemBus::new(Vec::new());

    let result = fetch(&mut bus, 0x8000_0001, 0, PrivilegeMode::Machine, 0);
    assert!(matches!(
        result,
        Err(VmError::InstructionAccessFault(0x8000_0001))
    ));
}

#[test]
fn fetch_out_of_bounds() {
    let mut bus = SystemBus::new(Vec::new());

    // Address outside any mapped region
    let result = fetch(&mut bus, 0xFFFF_FFFF, 0, PrivilegeMode::Machine, 0);
    assert!(matches!(
        result,
        Err(VmError::InstructionAccessFault(0xFFFF_FFFF))
    ));
}
