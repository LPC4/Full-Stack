//! Tests for the ROM boot stub and kernel-mode boot sequence.
//!
//! Covers:
//!   - ROM assembles without error and is correctly padded.
//!   - `M_TRAP_ADDR` is at the expected offset (0x100).
//!   - A freshly-created pipeline initialises `mtvec` to `M_TRAP_ADDR`.
//!   - `new_kernel` redirects the CPU to ROM_BASE so `_start` runs.
//!   - Full kernel boot: ROM `_start` does PMP + delegation + mret into
//!     S-mode; minimal kernel calls sys_exit and the VM halts correctly.

use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;
use virtual_machine::VirtualMachine;
use virtual_machine::bus::{RAM_BASE, SystemBus};
use virtual_machine::cpu::StepOutcome;
use virtual_machine::cpu::pipeline::Pipeline;
use virtual_machine::rom::{M_TRAP_ADDR, ROM_BASE, generate_rom_image};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assemble a minimal kernel and run it via `VirtualMachine::new_kernel`.
/// Returns (uart_output, exit_code).
fn run_kernel(src: &str, max_steps: u64) -> (String, i64) {
    let tokens: Vec<RvInstruction> = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if let Some(name) = l.strip_suffix(':') {
                RvInstruction::Label(name.to_string())
            } else if l.starts_with('.') {
                RvInstruction::Directive(l.to_string())
            } else {
                RvInstruction::Directive(format!("\t{l}"))
            }
        })
        .collect();

    let output = Assembler::assemble(&tokens).expect("kernel assembly failed");
    let mut vm = VirtualMachine::new_kernel(&output);
    let result = vm.run(max_steps);
    let code = match result.outcome {
        StepOutcome::Halted(c) => c,
        StepOutcome::Continue => i64::MIN,
    };
    (result.uart_output, code)
}

// ---------------------------------------------------------------------------
// ROM structure tests
// ---------------------------------------------------------------------------

#[test]
fn rom_assembles_without_error() {
    let bytes = generate_rom_image();
    assert!(!bytes.is_empty(), "ROM image must not be empty");
}

#[test]
fn rom_is_at_least_m_trap_offset() {
    let bytes = generate_rom_image();
    assert!(
        bytes.len() >= M_TRAP_ADDR as usize,
        "ROM must be at least {} bytes so _m_trap fits; got {} bytes",
        M_TRAP_ADDR,
        bytes.len()
    );
}

#[test]
fn m_trap_addr_is_256() {
    // _m_trap must be at a known, stable offset so Pipeline::new and _start
    // can both hardcode 0x100 without a symbol-table lookup at runtime.
    assert_eq!(M_TRAP_ADDR, ROM_BASE + 0x100);
    assert_eq!(M_TRAP_ADDR, 0x100);
}

#[test]
fn m_trap_addr_is_4_byte_aligned() {
    assert_eq!(
        M_TRAP_ADDR % 4,
        0,
        "_m_trap address must be 4-byte aligned for mtvec"
    );
}

// ---------------------------------------------------------------------------
// Pipeline initialisation test
// ---------------------------------------------------------------------------

#[test]
fn pipeline_mtvec_initialised_to_m_trap() {
    // A freshly-constructed Pipeline (for hosted non-kernel programs) must
    // point mtvec at _m_trap, not at _start.
    let rom = generate_rom_image();
    let mut bus = SystemBus::new(rom);
    let pipeline = Pipeline::new(RAM_BASE, RAM_BASE + 4 * 1024 * 1024);
    let _ = &mut bus; // bus not used after construction; suppress warning

    assert_eq!(
        pipeline.peek_csr_mtvec(),
        M_TRAP_ADDR,
        "mtvec must be M_TRAP_ADDR so ecalls from hosted programs reach _m_trap"
    );
}

// ---------------------------------------------------------------------------
// new_kernel boot-stub test
// ---------------------------------------------------------------------------

#[test]
fn new_kernel_starts_cpu_at_rom_base() {
    // Minimal kernel: just enough labels to assemble; the test only checks
    // that the CPU PC is ROM_BASE before any cycles run.
    let src = "
        .section .text
        .globl _kernel_start
        _kernel_start:
            li a7, 93
            li a0, 0
            ecall
    ";
    let tokens: Vec<RvInstruction> = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if let Some(name) = l.strip_suffix(':') {
                RvInstruction::Label(name.to_string())
            } else if l.starts_with('.') {
                RvInstruction::Directive(l.to_string())
            } else {
                RvInstruction::Directive(format!("\t{l}"))
            }
        })
        .collect();
    let output = Assembler::assemble(&tokens).expect("assembly failed");
    let vm = VirtualMachine::new_kernel(&output);

    assert_eq!(
        vm.peek_pc(),
        ROM_BASE,
        "new_kernel must start the CPU at ROM_BASE (0x0) so _start runs"
    );
}

// ---------------------------------------------------------------------------
// Full kernel boot integration test
// ---------------------------------------------------------------------------

#[test]
fn kernel_boot_pmp_delegation_mret_smode_exit() {
    // A minimal kernel that:
    //   1. Receives control in S-mode at RAM_BASE (after ROM _start mrets).
    //   2. Calls sys_exit(42) via ecall.
    //
    // The ecall from S-mode has cause=9 (supervisor ecall).  Bit 9 is NOT in
    // medeleg (we only delegate bits 8/12/13/15), so it goes to _m_trap in
    // M-mode.  _m_trap dispatches to sys_exit which writes code 42 to SYSCON.
    //
    // If PMP or the mret are broken the kernel never fetches from RAM and the
    // test either hangs (Continue) or faults.
    let src = "
        .section .text
        .globl _kernel_start
        _kernel_start:
            li a7, 93
            li a0, 42
            ecall
    ";
    let (_uart, code) = run_kernel(src, 500);
    assert_eq!(code, 42, "kernel should exit with code 42 via sys_exit");
}

#[test]
fn kernel_uart_output_via_smode_ecall() {
    // Kernel writes a single character ('A' = 65) via sys_putchar (syscall 1000).
    // This ecall from S-mode -> M-mode -> sys_putchar -> UART.
    let src = "
        .section .text
        .globl _kernel_start
        _kernel_start:
            li a7, 1000
            li a0, 65
            ecall
            li a7, 93
            li a0, 0
            ecall
    ";
    let (uart, code) = run_kernel(src, 1000);
    assert_eq!(code, 0);
    assert_eq!(
        uart, "A",
        "kernel should have written 'A' to UART via sys_putchar"
    );
}

#[test]
fn hosted_program_mtvec_still_works_after_rom_change() {
    // Non-kernel hosted programs (VirtualMachine::new) must still reach the
    // M-mode trap handler.  mtvec is no longer 0x000 (_start) but 0x100
    // (_m_trap), which is the right target.
    //
    // We assemble a tiny program that calls sys_exit(7) and verify it halts.
    let src = "
        .section .text
        .globl _start
        _start:
            li a7, 93
            li a0, 7
            ecall
    ";
    let tokens: Vec<RvInstruction> = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if let Some(name) = l.strip_suffix(':') {
                RvInstruction::Label(name.to_string())
            } else if l.starts_with('.') {
                RvInstruction::Directive(l.to_string())
            } else {
                RvInstruction::Directive(format!("\t{l}"))
            }
        })
        .collect();
    let output = Assembler::assemble(&tokens).expect("assembly failed");
    let mut vm = VirtualMachine::new(&output);
    let result = vm.run(500);
    let code = match result.outcome {
        StepOutcome::Halted(c) => c,
        StepOutcome::Continue => i64::MIN,
    };
    assert_eq!(
        code, 7,
        "hosted program must still reach M-mode trap handler"
    );
}

#[test]
fn start_is_not_trap_handler() {
    // Sanity: the bytes at ROM_BASE (0x000) must NOT be the same as the bytes
    // at M_TRAP_ADDR (0x100).  Before the ROM refactor, both pointed to the
    // same handler.  Now _start and _m_trap are different code.
    let bytes = generate_rom_image();
    if bytes.len() >= M_TRAP_ADDR as usize + 4 {
        let at_start: [u8; 4] = bytes[..4].try_into().unwrap();
        let at_trap: [u8; 4] = bytes[M_TRAP_ADDR as usize..M_TRAP_ADDR as usize + 4]
            .try_into()
            .unwrap();
        assert_ne!(
            at_start, at_trap,
            "_start and _m_trap should have different first instructions"
        );
    }
}
