use full_stack::virtual_machine::bus::{SystemBus, RAM_BASE, ROM_BASE};
use full_stack::virtual_machine::memory::MemoryAccess;
use full_stack::virtual_machine::error::VmError;

// ---------------------------------------------------------------------------
// System Bus, routing to different devices
// ---------------------------------------------------------------------------

#[test]
fn bus_route_to_ram() {
    let mut bus = SystemBus::new(Vec::new());
    
    // Write to RAM
    let _ = bus.write_byte(RAM_BASE, 0x42);
    let byte = bus.read_byte(RAM_BASE).unwrap();
    assert_eq!(byte, 0x42);
}

#[test]
fn bus_route_to_rom() {
    let rom_data = vec![0x13, 0x00, 0x00, 0x00]; // nop instruction
    let mut bus = SystemBus::new(rom_data);
    
    // Read from ROM
    let byte = bus.read_byte(ROM_BASE).unwrap();
    assert_eq!(byte, 0x13);
}

#[test]
fn bus_write_to_rom_fails() {
    let rom_data = vec![0x00];
    let mut bus = SystemBus::new(rom_data);
    
    // Writing to ROM should fail
    let result = bus.write_byte(ROM_BASE, 0xFF);
    assert!(result.is_err());
}

#[test]
fn bus_read_word_little_endian() {
    let mut bus = SystemBus::new(Vec::new());
    
    // Write bytes in little-endian order: 0x04030201
    let _ = bus.write_byte(RAM_BASE, 0x01);
    let _ = bus.write_byte(RAM_BASE + 1, 0x02);
    let _ = bus.write_byte(RAM_BASE + 2, 0x03);
    let _ = bus.write_byte(RAM_BASE + 3, 0x04);
    
    let word = bus.read_word(RAM_BASE).unwrap();
    assert_eq!(word, 0x04030201);
}

#[test]
fn bus_read_doubleword_little_endian() {
    let mut bus = SystemBus::new(Vec::new());
    
    // Write 64-bit value in little-endian
    let value = 0x0807_0605_0403_0201u64;
    let _ = bus.write_doubleword(RAM_BASE, value).unwrap();
    
    let dw = bus.read_doubleword(RAM_BASE).unwrap();
    assert_eq!(dw, value);
}

#[test]
fn bus_unmapped_address_read() {
    let mut bus = SystemBus::new(Vec::new());
    
    // Address not mapped to any device
    let result = bus.read_byte(0x4000_0000);
    assert!(matches!(result, Err(VmError::BusError(0x4000_0000))));
}

#[test]
fn bus_unmapped_address_write() {
    let mut bus = SystemBus::new(Vec::new());
    
    let result = bus.write_byte(0x4000_0000, 0x42);
    assert!(matches!(result, Err(VmError::BusError(0x4000_0000))));
}

// ---------------------------------------------------------------------------
// System Bus, RAM bounds checking
// ---------------------------------------------------------------------------

#[test]
fn bus_ram_first_byte() {
    let mut bus = SystemBus::new(Vec::new());
    
    let _ = bus.write_byte(RAM_BASE, 0xAA);
    let byte = bus.read_byte(RAM_BASE).unwrap();
    assert_eq!(byte, 0xAA);
}

#[test]
fn bus_ram_last_byte() {
    let mut bus = SystemBus::new(Vec::new());
    
    // RAM_SIZE_DEFAULT is 128 MB
    let last_addr = RAM_BASE + (128 * 1024 * 1024) as u64 - 1;
    
    let _ = bus.write_byte(last_addr, 0xBB);
    let byte = bus.read_byte(last_addr).unwrap();
    assert_eq!(byte, 0xBB);
}

#[test]
fn bus_ram_beyond_end() {
    let mut bus = SystemBus::new(Vec::new());
    
    // Just beyond RAM
    let beyond_addr = RAM_BASE + (128 * 1024 * 1024) as u64;
    
    let result = bus.write_byte(beyond_addr, 0xCC);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// System Bus, alignment requirements
// ---------------------------------------------------------------------------

#[test]
fn bus_read_halfword_aligned() {
    let mut bus = SystemBus::new(Vec::new());
    
    let _ = bus.write_halfword(RAM_BASE, 0xBEEF).unwrap();
    let hw = bus.read_halfword(RAM_BASE).unwrap();
    assert_eq!(hw, 0xBEEF);
}

#[test]
fn bus_read_word_aligned() {
    let mut bus = SystemBus::new(Vec::new());
    
    let _ = bus.write_word(RAM_BASE, 0xDEAD_BEEF).unwrap();
    let word = bus.read_word(RAM_BASE).unwrap();
    assert_eq!(word, 0xDEAD_BEEF);
}

#[test]
fn bus_read_doubleword_aligned() {
    let mut bus = SystemBus::new(Vec::new());
    
    let value = 0xDEAD_BEEF_CAFE_BABEu64;
    let _ = bus.write_doubleword(RAM_BASE, value).unwrap();
    let dw = bus.read_doubleword(RAM_BASE).unwrap();
    assert_eq!(dw, value);
}

// ---------------------------------------------------------------------------
// System Bus, device accessors
// ---------------------------------------------------------------------------

#[test]
fn bus_uart_mut_access() {
    let mut bus = SystemBus::new(Vec::new());
    
    let uart = bus.uart_mut();
    // Check that TX output buffer is empty
    assert_eq!(uart.tx_out.len(), 0);
}

#[test]
fn bus_clint_mut_access() {
    let mut bus = SystemBus::new(Vec::new());
    
    let clint = bus.clint_mut();
    // CLINT should be in initial state
    assert!(!clint.timer_irq_pending());
}

#[test]
fn bus_plic_mut_access() {
    let mut bus = SystemBus::new(Vec::new());
    
    let _plic = bus.plic_mut();
    // Just checking we can get mutable reference
}

// ---------------------------------------------------------------------------
// System Bus, size queries
// ---------------------------------------------------------------------------

#[test]
fn bus_ram_size() {
    let bus = SystemBus::new(Vec::new());
    
    assert_eq!(bus.ram_size(), 128 * 1024 * 1024);
}

#[test]
fn bus_rom_size() {
    let rom_data = vec![0u8; 1024];
    let bus = SystemBus::new(rom_data);
    
    assert_eq!(bus.rom_size(), 1024);
}
