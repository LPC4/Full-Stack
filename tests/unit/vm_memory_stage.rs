use full_stack::virtual_machine::cpu::pipeline::memory::{memory_stage, MemResult};
use full_stack::virtual_machine::cpu::pipeline::execute::ExecResult;
use full_stack::virtual_machine::bus::SystemBus;
use full_stack::virtual_machine::memory::MemoryAccess;
use full_stack::virtual_machine::cpu::PrivilegeMode;

// ---------------------------------------------------------------------------
// Memory stage, integer loads
// ---------------------------------------------------------------------------

#[test]
fn memory_load_byte_sign_extend() {
    let mut bus = SystemBus::new(Vec::new());
    let _ = bus.write_byte(0x8000_0000, 0xFF); // -1 as i8
    
    let exec_result = ExecResult::Load {
        rd: 1,
        addr: 0x8000_0000,
        funct3: 0, // lb
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { val, .. } = result {
        // Should be sign-extended: 0xFF -> 0xFFFF_FFFF_FFFF_FFFF
        assert_eq!(val, 0xFFFF_FFFF_FFFF_FFFFu64);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

#[test]
fn memory_load_byte_zero_extend() {
    let mut bus = SystemBus::new(Vec::new());
    let _ = bus.write_byte(0x8000_0000, 0xFF);
    
    let exec_result = ExecResult::Load {
        rd: 1,
        addr: 0x8000_0000,
        funct3: 4, // lbu
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { val, .. } = result {
        // Should be zero-extended: 0xFF -> 0x0000_0000_0000_00FF
        assert_eq!(val, 0xFF);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

#[test]
fn memory_load_halfword_sign_extend() {
    let mut bus = SystemBus::new(Vec::new());
    let _ = bus.write_halfword(0x8000_0000, 0xFFFF).unwrap(); // -1 as i16
    
    let exec_result = ExecResult::Load {
        rd: 1,
        addr: 0x8000_0000,
        funct3: 1, // lh
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { val, .. } = result {
        assert_eq!(val, 0xFFFF_FFFF_FFFF_FFFFu64);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

#[test]
fn memory_load_word_sign_extend() {
    let mut bus = SystemBus::new(Vec::new());
    let _ = bus.write_word(0x8000_0000, 0xFFFF_FFFF).unwrap(); // -1 as i32
    
    let exec_result = ExecResult::Load {
        rd: 1,
        addr: 0x8000_0000,
        funct3: 2, // lw
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { val, .. } = result {
        assert_eq!(val, 0xFFFF_FFFF_FFFF_FFFFu64);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

#[test]
fn memory_load_doubleword() {
    let mut bus = SystemBus::new(Vec::new());
    let value = 0xDEAD_BEEF_CAFE_BABEu64;
    let _ = bus.write_doubleword(0x8000_0000, value).unwrap();
    
    let exec_result = ExecResult::Load {
        rd: 1,
        addr: 0x8000_0000,
        funct3: 3, // ld
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { val, .. } = result {
        assert_eq!(val, value);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

// ---------------------------------------------------------------------------
// Memory stage, integer stores
// ---------------------------------------------------------------------------

#[test]
fn memory_store_byte() {
    let mut bus = SystemBus::new(Vec::new());
    
    let exec_result = ExecResult::Store {
        addr: 0x8000_0000,
        val: 0xFF,
        funct3: 0, // sb
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Jump { .. }));
    
    let byte = bus.read_byte(0x8000_0000).unwrap();
    assert_eq!(byte, 0xFF);
}

#[test]
fn memory_store_halfword() {
    let mut bus = SystemBus::new(Vec::new());
    
    let exec_result = ExecResult::Store {
        addr: 0x8000_0000,
        val: 0xBEEF,
        funct3: 1, // sh
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Jump { .. }));
    
    let hw = bus.read_halfword(0x8000_0000).unwrap();
    assert_eq!(hw, 0xBEEF);
}

#[test]
fn memory_store_word() {
    let mut bus = SystemBus::new(Vec::new());
    
    let exec_result = ExecResult::Store {
        addr: 0x8000_0000,
        val: 0xDEAD_BEEF,
        funct3: 2, // sw
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Jump { .. }));
    
    let word = bus.read_word(0x8000_0000).unwrap();
    assert_eq!(word, 0xDEAD_BEEF);
}

#[test]
fn memory_store_doubleword() {
    let mut bus = SystemBus::new(Vec::new());
    let value = 0xDEAD_BEEF_CAFE_BABEu64;
    
    let exec_result = ExecResult::Store {
        addr: 0x8000_0000,
        val: value,
        funct3: 3, // sd
        next_pc: 0x8000_0004,
    };
    
    let mut reservation = None;
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Jump { .. }));
    
    let dw = bus.read_doubleword(0x8000_0000).unwrap();
    assert_eq!(dw, value);
}

// ---------------------------------------------------------------------------
// Memory stage, pass-through operations
// ---------------------------------------------------------------------------

#[test]
fn memory_pass_through_write_int() {
    let mut bus = SystemBus::new(Vec::new());
    let mut reservation = None;
    
    let exec_result = ExecResult::WriteInt {
        rd: 1,
        val: 42,
        next_pc: 0x8000_0004,
    };
    
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::WriteInt { rd, val, next_pc } = result {
        assert_eq!(rd, 1);
        assert_eq!(val, 42);
        assert_eq!(next_pc, 0x8000_0004);
    } else {
        panic!("expected WriteInt, got {:?}", result);
    }
}

#[test]
fn memory_pass_through_jump() {
    let mut bus = SystemBus::new(Vec::new());
    let mut reservation = None;
    
    let exec_result = ExecResult::Jump {
        next_pc: 0x8000_0100,
    };
    
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    if let MemResult::Jump { next_pc } = result {
        assert_eq!(next_pc, 0x8000_0100);
    } else {
        panic!("expected Jump, got {:?}", result);
    }
}

#[test]
fn memory_pass_through_fence() {
    let mut bus = SystemBus::new(Vec::new());
    let mut reservation = None;
    
    let exec_result = ExecResult::Fence {
        next_pc: 0x8000_0004,
    };
    
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Fence { .. }));
}

#[test]
fn memory_pass_through_ecall() {
    let mut bus = SystemBus::new(Vec::new());
    let mut reservation = None;
    
    let exec_result = ExecResult::Ecall;
    
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Ecall));
}

#[test]
fn memory_pass_through_ebreak() {
    let mut bus = SystemBus::new(Vec::new());
    let mut reservation = None;
    
    let exec_result = ExecResult::Ebreak;
    
    let result = memory_stage(exec_result, &mut bus, &mut reservation, 0, PrivilegeMode::Machine, 0).unwrap();
    
    assert!(matches!(result, MemResult::Ebreak));
}
