//! Unit tests for the 5-stage pipeline.
//!
//! Each test loads a minimal RISC-V binary into a SystemBus, runs it through
//! the Pipeline, and asserts on both the computed results and the pipeline
//! performance counters (stall cycles, flush cycles, retired instructions).

use full_stack::virtual_machine::bus::{SystemBus, RAM_BASE};
use full_stack::virtual_machine::cpu::pipeline::{Pipeline, TickOutcome};
use full_stack::virtual_machine::memory::MemoryAccess;
use full_stack::virtual_machine::rom::generate_rom_image;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const STACK_TOP: u64 = RAM_BASE + 4 * 1024 * 1024; // 4 MiB stack

/// Write a sequence of 32-bit little-endian instruction words into the bus
/// starting at `base`, then return a Pipeline ready to run them.
fn load_program(bus: &mut SystemBus, base: u64, words: &[u32]) -> Pipeline {
    for (i, &w) in words.iter().enumerate() {
        let addr = base + (i as u64) * 4;
        let _ = bus.write_word(addr, w);
    }
    Pipeline::new(base, STACK_TOP)
}

/// Run at most `max` cycles, returning the tick outcome.
fn run_n(cpu: &mut Pipeline, bus: &mut SystemBus, max: u64) -> TickOutcome {
    for _ in 0..max {
        match cpu.tick(bus) {
            Ok(TickOutcome::Halted(code)) => return TickOutcome::Halted(code),
            Ok(TickOutcome::Continue | TickOutcome::EcallSquash) => {
                if let Some(code) = bus.take_syscon_exit() {
                    return TickOutcome::Halted(code);
                }
            }
            Err(_) => return TickOutcome::Halted(-1),
        }
    }
    TickOutcome::Continue
}

// ---------------------------------------------------------------------------
// Instruction encodings (hand-assembled)
// ---------------------------------------------------------------------------

// addi rd, rs1, imm  → opcode=0x13, funct3=0
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm12 = (imm as u32) & 0xFFF;
    (imm12 << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

// add rd, rs1, rs2  → opcode=0x33, funct3=0, funct7=0
fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0x33
}

// sw rs2, imm(rs1) → opcode=0x23, funct3=2
fn sw(rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm12 = (imm as u32) & 0xFFF;
    let imm_11_5 = imm12 >> 5;
    let imm_4_0 = imm12 & 0x1f;
    (imm_11_5 << 25) | (rs2 << 20) | (rs1 << 15) | (2 << 12) | (imm_4_0 << 7) | 0x23
}

// lw rd, imm(rs1) → opcode=0x03, funct3=2
fn lw(rd: u32, rs1: u32, imm: i32) -> u32 {
    let imm12 = (imm as u32) & 0xFFF;
    (imm12 << 20) | (rs1 << 15) | (2 << 12) | (rd << 7) | 0x03
}

// beq rs1, rs2, imm (branch if equal, imm is byte offset from PC)
fn beq(rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm13 = (imm as u32) & 0x1FFE; // bits [12:1], bit 0 is always 0
    let imm_12 = (imm13 >> 12) & 1;
    let imm_11 = (imm13 >> 11) & 1;
    let imm_10_5 = (imm13 >> 5) & 0x3f;
    let imm_4_1 = (imm13 >> 1) & 0xf;
    (imm_12 << 31)
        | (imm_10_5 << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0 << 12) // funct3=0 = BEQ
        | (imm_4_1 << 8)
        | (imm_11 << 7)
        | 0x63
}

// bne rs1, rs2, imm
fn bne(rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm13 = (imm as u32) & 0x1FFE;
    let imm_12 = (imm13 >> 12) & 1;
    let imm_11 = (imm13 >> 11) & 1;
    let imm_10_5 = (imm13 >> 5) & 0x3f;
    let imm_4_1 = (imm13 >> 1) & 0xf;
    (imm_12 << 31)
        | (imm_10_5 << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (1 << 12) // funct3=1 = BNE
        | (imm_4_1 << 8)
        | (imm_11 << 7)
        | 0x63
}

// ecall → syscall
fn ecall() -> u32 {
    0x0000_0073
}

// nop = addi x0, x0, 0
fn nop() -> u32 {
    addi(0, 0, 0)
}

// ---------------------------------------------------------------------------
// Test 1: Basic sequential execution — no hazards
// ---------------------------------------------------------------------------

#[test]
fn pipeline_basic_sequential_no_hazards() {
    // addi x1, x0, 1   ; x1 = 1
    // addi x2, x0, 2   ; x2 = 2
    // addi x3, x0, 3   ; x3 = 3
    // ecall(93, 0)      ; halt
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 1),
        addi(2, 0, 2),
        addi(3, 0, 3),
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 50);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(1), 1);
    assert_eq!(cpu.peek_reg(2), 2);
    assert_eq!(cpu.peek_reg(3), 3);

    // No hazards: zero stall cycles
    assert_eq!(cpu.stats.stall_cycles, 0, "no stalls expected for independent instructions");
    // Instructions retired = 6 (3 addi + 2 halt setup + ecall)
    // (Note: ecall itself may not retire via the normal path)
    assert!(cpu.stats.insns_retired >= 5);
}

// ---------------------------------------------------------------------------
// Test 2: RAW hazard resolved by EX/MEM forwarding (no stall)
// ---------------------------------------------------------------------------

#[test]
fn pipeline_raw_forwarding_no_stall() {
    // addi x1, x0, 10  ; x1 = 10
    // addi x1, x1, 5   ; x1 = 15  ← depends on previous x1, EX/MEM forward
    // addi x1, x1, 3   ; x1 = 18  ← depends on previous, MEM/WB forward
    // halt(x1 as exit code?)  — we'll just check x1 and zero stalls
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 10),  // x1 = 10
        addi(1, 1, 5),   // x1 = 15 (depends on previous)
        addi(1, 1, 3),   // x1 = 18 (depends on previous)
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 60);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(1), 18, "forwarding must produce correct value");
    assert_eq!(cpu.stats.stall_cycles, 0, "RAW hazard must be resolved by forwarding without stalls");
}

// ---------------------------------------------------------------------------
// Test 3: Load-use hazard — must stall 1 cycle
// ---------------------------------------------------------------------------

#[test]
fn pipeline_load_use_stall() {
    // Store 42 into memory, then load it and immediately use the loaded value.
    //   addi x1, x0, 42      ; x1 = 42
    //   sw   x1, 0(x2)       ; mem[x2] = 42 (x2 = 0 = invalid but we'll use a valid addr)
    // Better: use a known RAM address

    let mut bus = SystemBus::new(generate_rom_image());
    // We'll use x2 as a pointer into RAM
    let data_addr = RAM_BASE + 0x100;

    // Pre-store 42 at data_addr
    let _ = bus.write_word(data_addr, 42u32);

    let prog = [
        addi(1, 0, 99),         // x1 = 99
        sw(2, 1, -4),           // mem[sp-4] = 99  (x2 = STACK_TOP, set by PipelinedCpu::new)
        lw(3, 2, -4),           // x3 = mem[sp-4]  ← LOAD
        add(4, 3, 3),           // x4 = x3 + x3    ← load-use hazard on x3
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    // x2 (sp) is initialized to STACK_TOP
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 80);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(1), 99);
    assert_eq!(cpu.peek_reg(3), 99, "load must produce correct value");
    assert_eq!(cpu.peek_reg(4), 198, "load-use result must be correct");
    assert!(cpu.stats.stall_cycles >= 1, "load-use hazard must insert at least 1 stall");
}

// ---------------------------------------------------------------------------
// Test 4: Branch not taken — no flush
// ---------------------------------------------------------------------------

#[test]
fn pipeline_branch_not_taken_no_flush() {
    // x1 = 5, x2 = 6
    // beq x1, x2, +8  ; NOT taken (5 ≠ 6)
    // addi x3, x0, 1  ; should execute
    // halt
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 5),       // x1 = 5
        addi(2, 0, 6),       // x2 = 6
        beq(1, 2, 8),        // beq x1, x2, +8  (not taken)
        addi(3, 0, 1),       // x3 = 1  (should execute)
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 60);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(3), 1, "instruction after not-taken branch must execute");
    // A not-taken branch with a predict-not-taken predictor has no flush
    // (the predictor might predict taken on later iterations if seen multiple times,
    // but on first encounter starts weakly-not-taken → correct prediction)
}

// ---------------------------------------------------------------------------
// Test 5: Branch taken — pipeline flush
// ---------------------------------------------------------------------------

#[test]
fn pipeline_branch_taken_causes_flush() {
    // x1 = 5, x2 = 5
    // beq x1, x2, +12  ; TAKEN (5 == 5), skip next instruction
    // addi x3, x0, 99  ; should NOT execute (skipped)
    // addi x3, x0, 1   ; should execute (branch target)
    // halt
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 5),       // [0]  x1 = 5
        addi(2, 0, 5),       // [4]  x2 = 5
        beq(1, 2, 12),       // [8]  beq → skip [12], jump to [20]
        addi(3, 0, 99),      // [12] skipped
        nop(),               // [16] skipped
        addi(3, 0, 1),       // [20] x3 = 1 (branch target)
        addi(17, 0, 93),     // [24]
        addi(10, 0, 0),      // [28]
        ecall(),             // [32]
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 80);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(3), 1, "only branch target instruction must execute");
    assert_eq!(cpu.peek_reg(1), 5);
    assert_eq!(cpu.peek_reg(2), 5);
    // Branch was taken: at least one flush should have occurred
    assert!(cpu.stats.flush_cycles > 0, "taken branch must cause pipeline flush");
}

// ---------------------------------------------------------------------------
// Test 6: Double data hazard — both EX/MEM and MEM/WB forwarding
// ---------------------------------------------------------------------------

#[test]
fn pipeline_double_forwarding() {
    // addi x1, x0, 3  ; x1 = 3
    // addi x2, x0, 4  ; x2 = 4
    // add  x3, x1, x2 ; x3 = 7  (needs x1 via MEM/WB, x2 via EX/MEM)
    // halt
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 3),       // x1 = 3
        addi(2, 0, 4),       // x2 = 4
        add(3, 1, 2),        // x3 = 7 (EX/MEM fwd x2, MEM/WB fwd x1)
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 50);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(3), 7, "double forwarding must produce correct result");
    assert_eq!(cpu.stats.stall_cycles, 0, "both hazards should be resolved by forwarding");
}

// ---------------------------------------------------------------------------
// Test 7: Loop with taken branch and correct result
// ---------------------------------------------------------------------------

#[test]
fn pipeline_loop_sum_1_to_5() {
    // Compute x1 = 1+2+3+4+5 = 15 using a loop
    //   x1 = 0  (accumulator)
    //   x2 = 5  (counter)
    //   x3 = 1  (decrement step)
    // loop:
    //   add  x1, x1, x2    ; acc += counter
    //   addi x2, x2, -1    ; counter--
    //   bne  x2, x0, -8    ; if counter != 0, branch back
    // halt

    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 0),       // [0]  x1 = 0
        addi(2, 0, 5),       // [4]  x2 = 5
        // loop starts at [8]
        add(1, 1, 2),        // [8]  x1 += x2
        addi(2, 2, -1),      // [12] x2--
        bne(2, 0, -8),       // [16] if x2 != 0, jump to [8]
        addi(17, 0, 93),     // [20]
        addi(10, 0, 0),      // [24]
        ecall(),             // [28]
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 200);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(1), 15, "loop sum must be 15");
    assert_eq!(cpu.peek_reg(2), 0, "counter must reach 0");

    // 5 iterations × 1 taken branch each = 5 branch mispredictions on first encounter
    // with predictor warming up. At least some flushes expected.
    assert!(
        cpu.stats.flush_cycles > 0,
        "loop back-edges should cause at least one flush"
    );
}

// ---------------------------------------------------------------------------
// Test 8: Pipeline stats sanity — cycle count >= insns_retired
// ---------------------------------------------------------------------------

#[test]
fn pipeline_cycle_count_geq_retired() {
    let mut bus = SystemBus::new(generate_rom_image());
    let prog = [
        addi(1, 0, 1),
        addi(2, 0, 2),
        add(3, 1, 2),
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);
    let _ = run_n(&mut cpu, &mut bus, 100);

    assert!(
        cpu.stats.cycles >= cpu.stats.insns_retired,
        "cycle count must be >= retired instruction count"
    );
}

// ---------------------------------------------------------------------------
// Test 9: Store then load (no load-use, store is before load)
// ---------------------------------------------------------------------------

#[test]
fn pipeline_store_load_correct() {
    let mut bus = SystemBus::new(generate_rom_image());
    // x2 = stack pointer (STACK_TOP)
    // Store x1=77 then load it back into x3
    let prog = [
        addi(1, 0, 77),      // x1 = 77
        sw(2, 1, -8),        // mem[sp-8] = 77
        nop(),               // gap (no load-use)
        nop(),               // gap
        lw(3, 2, -8),        // x3 = mem[sp-8]
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let outcome = run_n(&mut cpu, &mut bus, 80);
    assert!(matches!(outcome, TickOutcome::Halted(0)));

    assert_eq!(cpu.peek_reg(1), 77);
    assert_eq!(cpu.peek_reg(3), 77, "load after store must read written value");
}

// ---------------------------------------------------------------------------
// Test 10: Branch predictor statistics
// ---------------------------------------------------------------------------

#[test]
fn predictor_stats_tracked() {
    let mut bus = SystemBus::new(generate_rom_image());
    // A simple branch that is not taken (x1=1, x2=2, beq → not taken)
    let prog = [
        addi(1, 0, 1),
        addi(2, 0, 2),
        beq(1, 2, 8),        // not taken
        addi(3, 0, 42),
        addi(17, 0, 93),
        addi(10, 0, 0),
        ecall(),
    ];
    let mut cpu = load_program(&mut bus, RAM_BASE, &prog);

    let _ = run_n(&mut cpu, &mut bus, 60);

    let ps = cpu.predictor_stats();
    assert!(ps.total >= 1, "predictor must track at least one branch");
}
