use virtual_machine::cpu::csr::{CsrFile, addr};

// ---------------------------------------------------------------------------
// CSR File, basic read/write
// ---------------------------------------------------------------------------

#[test]
fn csr_mstatus_read_write() {
    let mut csrs = CsrFile::new();
    assert_eq!(csrs.read(addr::MSTATUS).unwrap(), 0);

    // 0x1234_4678: MPP[12:11]=0 (User, valid) - round-trips unchanged
    csrs.write(addr::MSTATUS, 0x1234_4678).unwrap();
    assert_eq!(csrs.read(addr::MSTATUS).unwrap(), 0x1234_4678);

    // WARL: MPP=2 (reserved) must be mapped to 3 (Machine)
    csrs.write(addr::MSTATUS, 0x1234_5678).unwrap(); // MPP=2 in bits[12:11]
    let stored = csrs.read(addr::MSTATUS).unwrap();
    assert_eq!(
        (stored >> 11) & 3,
        3,
        "WARL must map reserved MPP=2 to MPP=3"
    );
}

#[test]
fn csr_misa_readonly() {
    let mut csrs = CsrFile::new();
    let original = csrs.read(addr::MISA).unwrap();

    // Writes to MISA should be ignored (WARL)
    csrs.write(addr::MISA, 0).unwrap();
    assert_eq!(csrs.read(addr::MISA).unwrap(), original);
}

#[test]
fn csr_mtvec_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::MTVEC, 0x8000_0000 | 1).unwrap(); // vectored mode
    assert_eq!(csrs.read(addr::MTVEC).unwrap(), 0x8000_0001);
}

#[test]
fn csr_mepc_alignment() {
    let mut csrs = CsrFile::new();

    // MEPC must be aligned to 4 bytes; lowest 2 bits are cleared
    csrs.write(addr::MEPC, 0x8000_0003).unwrap();
    assert_eq!(csrs.read(addr::MEPC).unwrap(), 0x8000_0000);

    csrs.write(addr::MEPC, 0x8000_0002).unwrap();
    assert_eq!(csrs.read(addr::MEPC).unwrap(), 0x8000_0000);
}

#[test]
fn csr_mie_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::MIE, 0x888).unwrap(); // Enable timer, software, external interrupts
    assert_eq!(csrs.read(addr::MIE).unwrap(), 0x888);
}

#[test]
fn csr_mip_read_write() {
    let mut csrs = CsrFile::new();

    // SSIP (bit 1) is software-writable; MTIP/MEIP are hardware-driven read-only
    csrs.write(addr::MIP, 0x2).unwrap(); // Set SSIP
    assert_eq!(csrs.read(addr::MIP).unwrap(), 0x2);

    // Writing a hardware-only bit (MTIP=bit7) must be silently ignored
    csrs.write(addr::MIP, 0x80).unwrap();
    assert_eq!(
        csrs.read(addr::MIP).unwrap(),
        0,
        "MTIP is read-only via software writes"
    );
}

// ---------------------------------------------------------------------------
// CSR File, floating-point CSRs
// ---------------------------------------------------------------------------

#[test]
fn csr_fflags_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::FFLAGS, 0x1F).unwrap(); // All exception flags
    assert_eq!(csrs.read(addr::FFLAGS).unwrap(), 0x1F);

    // Only lower 5 bits are valid
    csrs.write(addr::FFLAGS, 0xFF).unwrap();
    assert_eq!(csrs.read(addr::FFLAGS).unwrap(), 0x1F);
}

#[test]
fn csr_frm_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::FRM, 0x7).unwrap(); // RMM
    assert_eq!(csrs.read(addr::FRM).unwrap(), 0x7);

    // Only lower 3 bits are valid
    csrs.write(addr::FRM, 0xFF).unwrap();
    assert_eq!(csrs.read(addr::FRM).unwrap(), 0x7);
}

#[test]
fn csr_fcsr_composite() {
    let mut csrs = CsrFile::new();

    // Write FCSR: frm=2 (RDN), fflags=0x11 (NV + NX)
    let fcsr_val = (2u64 << 5) | 0x11;
    csrs.write(addr::FCSR, fcsr_val).unwrap();

    assert_eq!(csrs.read(addr::FRM).unwrap(), 2);
    assert_eq!(csrs.read(addr::FFLAGS).unwrap(), 0x11);
    assert_eq!(csrs.read(addr::FCSR).unwrap(), fcsr_val);
}

#[test]
fn csr_accumulate_fflags() {
    let mut csrs = CsrFile::new();

    csrs.accumulate_fflags(0x01); // NX
    assert_eq!(csrs.fflags, 0x01);

    csrs.accumulate_fflags(0x10); // NV
    assert_eq!(csrs.fflags, 0x11);

    // Accumulation is OR, not replace
    csrs.accumulate_fflags(0x01);
    assert_eq!(csrs.fflags, 0x11);
}

// ---------------------------------------------------------------------------
// CSR File, performance counters
// ---------------------------------------------------------------------------

#[test]
fn csr_cycle_increment() {
    let mut csrs = CsrFile::new();

    assert_eq!(csrs.read(addr::CYCLE).unwrap(), 0);
    csrs.increment_cycle();
    assert_eq!(csrs.read(addr::CYCLE).unwrap(), 1);
    csrs.increment_cycle();
    assert_eq!(csrs.read(addr::CYCLE).unwrap(), 2);
}

#[test]
fn csr_instret_increment() {
    let mut csrs = CsrFile::new();

    assert_eq!(csrs.read(addr::INSTRET).unwrap(), 0);
    csrs.increment_instret();
    assert_eq!(csrs.read(addr::INSTRET).unwrap(), 1);
}

#[test]
fn csr_time_alias() {
    let mut csrs = CsrFile::new();

    csrs.increment_cycle();
    csrs.increment_cycle();

    // TIME is aliased to CYCLE
    assert_eq!(
        csrs.read(addr::TIME).unwrap(),
        csrs.read(addr::CYCLE).unwrap()
    );
}

#[test]
fn csr_counters_wrap() {
    let mut csrs = CsrFile::new();

    // Set cycle near max
    csrs.cycle = u64::MAX;
    csrs.increment_cycle();

    // Should wrap to 0
    assert_eq!(csrs.read(addr::CYCLE).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// CSR File, special registers
// ---------------------------------------------------------------------------

#[test]
fn csr_mscratch_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::MSCRATCH, 0xDEAD_BEEF).unwrap();
    assert_eq!(csrs.read(addr::MSCRATCH).unwrap(), 0xDEAD_BEEF);
}

#[test]
fn csr_mhartid_readonly() {
    let csrs = CsrFile::new();

    // MHARTID is always 0 in single-hart implementation
    assert_eq!(csrs.read(addr::MHARTID).unwrap(), 0);

    // Write should be ignored
    let mut csrs = csrs;
    csrs.write(addr::MHARTID, 42).unwrap();
    assert_eq!(csrs.read(addr::MHARTID).unwrap(), 0);
}

#[test]
fn csr_mcause_read_write() {
    let mut csrs = CsrFile::new();

    // Exception cause
    csrs.write(addr::MCAUSE, 2).unwrap(); // Illegal instruction
    assert_eq!(csrs.read(addr::MCAUSE).unwrap(), 2);

    // Interrupt cause (bit 63 set)
    csrs.write(addr::MCAUSE, (1u64 << 63) | 7).unwrap(); // Machine timer interrupt
    assert_eq!(csrs.read(addr::MCAUSE).unwrap(), (1u64 << 63) | 7);
}

#[test]
fn csr_mtval_read_write() {
    let mut csrs = CsrFile::new();

    csrs.write(addr::MTVAL, 0xBAD_ADD).unwrap();
    assert_eq!(csrs.read(addr::MTVAL).unwrap(), 0xBAD_ADD);
}

// ---------------------------------------------------------------------------
// CSR File, illegal CSR addresses
// ---------------------------------------------------------------------------

#[test]
fn csr_illegal_address_read() {
    let csrs = CsrFile::new();

    // Non-existent CSR address
    assert!(csrs.read(0x999).is_err());
}

#[test]
fn csr_illegal_address_write() {
    let mut csrs = CsrFile::new();

    // Non-existent CSR address
    assert!(csrs.write(0x999, 42).is_err());
}

// ---------------------------------------------------------------------------
// CSR File, rounding mode helper
// ---------------------------------------------------------------------------

#[test]
fn csr_rounding_mode() {
    let mut csrs = CsrFile::new();

    assert_eq!(csrs.rounding_mode(), 0); // Default RNE

    csrs.write(addr::FRM, 3).unwrap(); // RTZ
    assert_eq!(csrs.rounding_mode(), 3);
}
