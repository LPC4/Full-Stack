use super::boot::USER_CODE_VA;
use super::device_io::append_terminal_bytes;
use super::stepping::step_over_target_for_words;
use super::*;
use crate::runconfig::RunConfig;
use crate::session::SessionEngine;
use crate::workspace::{MemoryFs, VPath};
use asm_to_binary::riscv::rv64i::{Addi, Auipc, Jal, Jalr};
use asm_to_binary::traits::Instruction as _;

const LONG_BUDGET: Duration = Duration::from_secs(5);

fn build_artifacts(source: &str) -> BuildArtifacts {
    let mut fs = MemoryFs::new();
    fs.write_text(
        &VPath::parse("demo/demo.build").expect("path"),
        "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"hosted\"\n",
    )
    .expect("write manifest");
    fs.write_text(&VPath::parse("demo/demo.hll").expect("path"), source)
        .expect("write source");
    let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));
    SessionEngine::new(&fs)
        .build_config(&config)
        .expect("build config")
        .artifacts
}

fn build_asm_artifacts(source: &str) -> BuildArtifacts {
    let mut fs = MemoryFs::new();
    fs.write_text(
        &VPath::parse("demo/demo.build").expect("path"),
        "name = \"demo\"\ntarget = \"freestanding\"\nentry = \"_start\"\nroot = \"demo.s\"\n",
    )
    .expect("write manifest");
    fs.write_text(&VPath::parse("demo/demo.s").expect("path"), source)
        .expect("write source");
    let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));
    SessionEngine::new(&fs)
        .build_config(&config)
        .expect("build config")
        .artifacts
}

fn drive_to_completion(live: &mut LiveMachine) {
    for _ in 0..1_000 {
        if live.state != MachineState::Running {
            return;
        }
        live.drive(10_000, LONG_BUDGET);
    }
    panic!("machine did not finish within the test budget");
}

#[test]
fn hosted_program_exits_through_drive() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(7));
    assert!(live.steps > 0);
    let result = live.result();
    assert_eq!(result.exit_code, Some(7));
    assert_eq!(result.steps, live.steps);
    assert!(result.registers.is_some());
}

#[test]
fn time_travel_restores_earlier_cpu_state() {
    let artifacts = build_artifacts(
        "main: () -> i32 {\n    a: i32 = 1\n    a = a + 10\n    a = a + 100\n    return a\n}",
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.timeline.set_enabled(true);

    // Step a few instructions; each pause captures a frame.
    live.step_instruction();
    live.step_instruction();
    let early_frame = live.timeline.len() - 1;
    let early_pc = live.registers().pc;
    let early_x = live.registers().x;

    // Advance further so the live pc moves on.
    for _ in 0..4 {
        live.step_instruction();
    }
    assert_ne!(live.registers().pc, early_pc, "pc should have advanced");
    assert!(live.timeline.len() > early_frame + 1);

    // Rewind to the earlier frame; CPU state must match exactly.
    assert!(live.restore_timepoint(early_frame));
    assert_eq!(live.registers().pc, early_pc);
    assert_eq!(live.registers().x, early_x);
    assert_eq!(live.state, MachineState::Paused);
}

#[test]
fn drive_respects_the_step_budget() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    let stepped = live.drive(3, LONG_BUDGET);
    assert_eq!(stepped, 3);
    assert_eq!(live.state, MachineState::Running);
    assert_eq!(live.steps, 3);
}

#[test]
fn pause_blocks_drive_and_step_one_advances() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    assert_eq!(live.drive(100, LONG_BUDGET), 0);
    assert_eq!(live.steps, 0);
    live.step_one();
    live.step_one();
    assert_eq!(live.steps, 2);
    assert_eq!(live.state, MachineState::Paused);
    live.resume();
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(7));
}

#[test]
fn call_stack_reports_pc_and_the_calling_frames() {
    let artifacts = build_artifacts(
        "inner: () -> i32 { return 5 }\n\
             outer: () -> i32 { return inner() + 1 }\n\
             main: () -> i32 { return outer() }",
    );
    let inner_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("demo__inner")
            .expect("inner symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.breakpoints.insert(inner_pc);
    for _ in 0..1_000 {
        if live.state != MachineState::Running {
            break;
        }
        live.drive(1_000, LONG_BUDGET);
    }
    assert_eq!(live.state, MachineState::Paused);

    let frames = live.call_stack(8);
    assert_eq!(frames[0], inner_pc);
    let names: Vec<&str> = frames
        .iter()
        .filter_map(|&pc| live.symbol_near(pc).map(|(name, _)| name))
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("demo__outer")),
        "outer missing from {names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("main")),
        "main missing from {names:?}"
    );
}

#[test]
fn breakpoint_pauses_at_the_same_pc_and_step_count() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );

    let run_to_breakpoint = |artifacts: &BuildArtifacts| {
        let mut live = LiveMachine::new_hosted(artifacts).expect("boot");
        live.breakpoints.insert(main_pc);
        for _ in 0..1_000 {
            if live.state != MachineState::Running {
                break;
            }
            live.drive(1_000, LONG_BUDGET);
        }
        live
    };

    let first = run_to_breakpoint(&artifacts);
    assert_eq!(first.state, MachineState::Paused);
    assert_eq!(first.registers().pc, main_pc);

    let second = run_to_breakpoint(&artifacts);
    assert_eq!(second.steps, first.steps, "breakpoint hit is deterministic");

    // Resuming steps off the breakpoint instead of re-triggering it.
    let mut live = first;
    live.resume();
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(7));
}

#[test]
fn terminal_bytes_honor_backspace_and_carriage_return() {
    let mut log = String::new();
    append_terminal_bytes(&mut log, b"/$ helq");
    append_terminal_bytes(&mut log, b"\x08 \x08p\r\n");
    assert_eq!(log, "/$ help\n");
    // Backspace never erases past a line break.
    append_terminal_bytes(&mut log, b"\x08\x08x");
    assert_eq!(log, "/$ help\nx");
}

#[test]
fn step_cycles_advances_and_records_the_previous_registers() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.step_cycles(5);
    assert_eq!(live.steps, 5);
    assert_eq!(live.state, MachineState::Paused);
    let prev = live.prev_step_registers().expect("prev registers");
    assert_ne!(prev.pc, live.registers().pc, "pc moved across the steps");
    live.resume();
    assert!(live.prev_step_registers().is_none(), "cleared on resume");
}

#[test]
fn cycle_and_instruction_steps_are_distinct() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();

    live.step_cycle();
    assert_eq!(live.steps, 1, "cycle step is one pipeline tick");
    let retired_after_cycle = live.vm.insns_retired();

    live.step_instruction();
    assert_eq!(
        live.vm.insns_retired(),
        retired_after_cycle + 1,
        "instruction step waits for one retirement"
    );
    assert!(
        live.steps > 2,
        "instruction stepping may consume pipeline fill cycles"
    );
}

#[test]
fn step_over_classifies_direct_and_indirect_calls() {
    let pc = 0x8000_1000;
    let addi = Addi::new(5, 5, 1).encode();
    let direct_call = Jal::new(1, 0x40).encode();
    let jump = Jal::new(0, 0x40).encode();
    let indirect_call = Jalr::new(1, 5, 0).encode();
    let ret = Jalr::new(0, 1, 0).encode();

    assert_eq!(
        step_over_target_for_words(pc, direct_call, None),
        Some(pc + 4)
    );
    assert_eq!(
        step_over_target_for_words(pc, indirect_call, None),
        Some(pc + 4)
    );
    assert_eq!(step_over_target_for_words(pc, addi, None), None);
    assert_eq!(step_over_target_for_words(pc, jump, None), None);
    assert_eq!(step_over_target_for_words(pc, ret, None), None);
}

#[test]
fn step_over_classifies_auipc_jalr_call_pair() {
    let pc = 0x8000_1000;
    let auipc_ra = Auipc::new(1, 0).encode();
    let jalr_ra = Jalr::new(1, 1, 0).encode();
    let jalr_other = Jalr::new(1, 5, 0).encode();

    assert_eq!(
        step_over_target_for_words(pc, auipc_ra, Some(jalr_ra)),
        Some(pc + 8)
    );
    assert_eq!(
        step_over_target_for_words(pc, auipc_ra, Some(jalr_other)),
        None
    );
}

#[test]
fn line_step_retires_a_self_jump_once() {
    let artifacts = build_asm_artifacts(".globl _start\n_start:\ndone:\n    j done\n");
    let done_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("done")
            .expect("done symbol"),
    );
    let mut live = LiveMachine::new_freestanding(&artifacts).expect("boot");
    live.pause();
    assert!(live.has_line_table(), "assembly build carries a line table");
    assert_eq!(live.registers().pc, done_pc);

    let retired = live.vm.insns_retired();
    live.step_line_in();

    assert_eq!(live.state, MachineState::Paused);
    assert_eq!(live.registers().pc, done_pc);
    assert_eq!(live.vm.insns_retired(), retired + 1);
}

#[test]
fn line_step_wall_budget_stops_an_indirect_same_line_loop() {
    let artifacts =
        build_asm_artifacts(".globl _start\n_start:\n    la ra, spin\nspin:\n    ret\n");
    let spin_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("spin")
            .expect("spin symbol"),
    );
    let mut live = LiveMachine::new_freestanding(&artifacts).expect("boot");
    live.run_to(spin_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);

    let steps = live.steps;
    live.step_line_impl_with_budget(false, Duration::ZERO);

    assert_eq!(live.state, MachineState::Paused);
    assert_eq!(live.registers().pc, spin_pc);
    assert_eq!(live.steps - steps, CLOCK_CHECK_INTERVAL);
}

#[test]
fn breakpoint_specs_resolve_against_runtime_symbols() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let live = LiveMachine::new_hosted(&artifacts).expect("boot");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );

    assert_eq!(live.resolve_breakpoint("main").expect("resolve"), main_pc);
    assert_eq!(
        live.resolve_breakpoint("main+0x10").expect("resolve"),
        main_pc + 0x10
    );
    let spec = live
        .breakpoint_spec_for_addr(main_pc + 0x10)
        .expect("format spec");
    assert_eq!(
        live.resolve_breakpoint(&spec).expect("resolve formatted"),
        main_pc + 0x10
    );
    assert!(live.resolve_breakpoint("missing").is_err());
}

#[test]
fn run_to_stops_at_the_target_and_does_not_stick() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);
    assert_eq!(live.registers().pc, main_pc);
    // The one-shot breakpoint is consumed: resuming runs to completion.
    live.resume();
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(7));
}

#[test]
fn symbols_and_memory_reads_serve_the_debugger() {
    let artifacts = build_artifacts("main: () -> i32 { return 7 }");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let live = LiveMachine::new_hosted(&artifacts).expect("boot");
    assert_eq!(live.symbol_at(main_pc), Some("main"));
    let (name, offset) = live.symbol_near(main_pc).expect("nearest symbol");
    assert_eq!((name, offset), ("main", 0));
    let word = live.read_virtual(main_pc, 4);
    assert_ne!(word, vec![0u8; 4], "code bytes are readable at main");
    assert_eq!(live.read_physical(main_pc, 4), word, "identity-mapped");
}

#[test]
fn line_table_maps_runtime_pc_to_source_lines() {
    let artifacts = build_artifacts("main: () -> i32 {\n    return 7\n}");
    let live = LiveMachine::new_hosted(&artifacts).expect("boot");
    assert!(live.has_line_table(), "hosted build carries a line table");

    // Round-trip a real table entry through the runtime-pc mapping the
    // debugger uses (this build also links stdlib, so entries are merged).
    let entry = artifacts
        .linked
        .line_table_iter()
        .next()
        .expect("at least one line entry");
    let file = artifacts
        .linked
        .file_name(entry.file_id)
        .expect("file name")
        .to_owned();
    let pc = LiveMachine::runtime_address(entry.addr);
    assert_eq!(live.source_for_pc(pc), Some((file.as_str(), entry.line)));
    assert_eq!(
        live.pc_for_source(&file, entry.line),
        Some(LiveMachine::runtime_address(
            artifacts
                .linked
                .addr_for_source(&file, entry.line)
                .expect("addr for source"),
        )),
    );
}

#[test]
fn step_line_advances_to_a_different_source_line() {
    let artifacts =
        build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);

    let origin = live
        .source_for_pc(live.registers().pc)
        .map(|(_, line)| line)
        .expect("main pc has a source line");
    live.step_line();
    assert!(live.state.is_alive(), "step_line paused, not ran away");
    let landed = live
        .source_for_pc(live.registers().pc)
        .map(|(_, line)| line)
        .expect("landed pc has a source line");
    assert_ne!(landed, origin, "stepped onto a different source line");
}

#[test]
fn step_line_captures_a_time_travel_frame() {
    let artifacts =
        build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.timeline.set_enabled(true);
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);

    let before = live.timeline.len();
    live.step_line();
    assert_eq!(
        live.timeline.len(),
        before + 1,
        "step_line left a scrubber tick where it paused"
    );
    assert_eq!(
        live.timeline.retired_at(live.timeline.len() - 1),
        Some(live.vm.insns_retired()),
    );
}

#[test]
fn restore_timepoint_rebases_history_and_keeps_capturing() {
    let artifacts =
        build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.timeline.set_enabled(true);
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    for _ in 0..3 {
        live.step_instruction();
    }
    let frames = live.timeline.len();
    assert!(frames >= 4, "expected a frame per pause, got {frames}");

    assert!(live.restore_timepoint(1));
    assert_eq!(
        live.vm.insns_retired(),
        live.timeline.retired_at(1).expect("frame 1 retired count")
    );
    // Scrubbing alone drops nothing; the stale future goes on the next capture.
    assert_eq!(live.timeline.len(), frames);
    live.step_instruction();
    assert_eq!(
        live.timeline.len(),
        3,
        "truncated to frame 1 plus the new frame"
    );
    assert_eq!(
        live.timeline.retired_at(2),
        Some(live.vm.insns_retired()),
        "capture resumed from the rewound retired counter"
    );
}

#[test]
fn step_line_steps_over_calls_without_pausing_in_the_callee() {
    let artifacts = build_artifacts(
        "add: (a: i32, b: i32) -> i32 {\n    c := a + b\n    return c\n}\n\nmain: () -> i32 {\n    x := add(1, 2)\n    y := x + 4\n    return x + y\n}",
    );
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);

    let origin = live
        .source_for_pc(live.registers().pc)
        .map(|(_, line)| line)
        .expect("main pc has a source line");
    assert!(origin >= 6, "paused in main (line {origin}), not add");
    live.step_line();
    assert!(live.state.is_alive(), "step_line paused, not ran away");
    let landed = live
        .source_for_pc(live.registers().pc)
        .map(|(_, line)| line)
        .expect("landed pc has a source line");
    assert_ne!(landed, origin, "stepped onto a different source line");
    assert!(landed >= 6, "landed in main (line {landed}), not in add");
}

#[test]
fn step_line_in_pauses_on_the_callees_first_line() {
    let artifacts = build_artifacts(
        "add: (a: i32, b: i32) -> i32 {\n    c := a + b\n    return c\n}\n\nmain: () -> i32 {\n    x := add(1, 2)\n    y := x + 4\n    return x + y\n}",
    );
    let main_pc = LiveMachine::runtime_address(
        artifacts
            .linked
            .symbol_address("main")
            .expect("main symbol"),
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    live.pause();
    live.run_to(main_pc);
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Paused);

    live.step_line_in();
    assert!(live.state.is_alive(), "step_line_in paused, not ran away");
    let landed = live
        .source_for_pc(live.registers().pc)
        .map(|(_, line)| line)
        .expect("landed pc has a source line");
    assert!(landed <= 4, "landed inside add (line {landed}), not main");
}

#[test]
fn kernel_session_reads_user_virtual_addresses_through_the_mmu() {
    use crate::runconfig::RunKind;
    use crate::workspace::sample_workspace;

    let fs = sample_workspace().expect("sample workspace");
    let store = crate::runconfig::RunConfigStore::load(&fs).expect("run config store");
    let config = store
        .configs
        .iter()
        .find(|config| config.kind == RunKind::KernelBoot)
        .expect("kernel config");
    let boot = SessionEngine::new(&fs)
        .boot_kernel_config(config)
        .expect("boot kernel config");
    let mut live = LiveMachine::new_kernel(
        &boot.build.artifacts,
        Some(&boot.init),
        Some(&boot.fs_image),
        "",
    )
    .expect("live kernel");
    for _ in 0..1_000 {
        live.drive(100_000, LONG_BUDGET);
        if live.uart_log.contains("HLL shell ready") {
            break;
        }
        assert!(live.state.is_alive(), "kernel died: {:?}", live.state);
    }
    assert!(live.uart_log.contains("HLL shell ready"), "shell booted");

    // The shell (pid 1) runs at USER_CODE_VA behind Sv39; the debugger read
    // must translate, not treat the VA as physical (which reads zeros).
    let pa = live.translate(USER_CODE_VA).expect("user code VA maps");
    assert_ne!(pa, USER_CODE_VA, "user code is not identity-mapped");
    let virt = live.read_virtual(USER_CODE_VA, 4);
    assert_ne!(virt, vec![0u8; 4], "instruction bytes visible via the MMU");
    assert_eq!(virt, live.read_physical(pa, 4), "walk matches physical");
}

// A raw-asm root boots bare-metal and exits through the ROM ecall handler;
// the session must reach Exited(0) without any hosted runtime.
#[test]
fn freestanding_asm_session_runs_to_exit() {
    let mut fs = MemoryFs::new();
    fs.write_text(
        &VPath::parse("asm/exit.build").expect("path"),
        "name = \"exit\"\ntarget = \"freestanding\"\nroot = \"exit.s\"\nentry = \"_start\"\n",
    )
    .expect("write manifest");
    fs.write_text(
        &VPath::parse("asm/exit.s").expect("path"),
        ".globl _start\n_start:\n  li a0, 0\n  li a7, 93\n  ecall\n",
    )
    .expect("write source");
    let config = RunConfig::hosted("Exit", VPath::parse("asm/exit.build").expect("path"));
    let artifacts = SessionEngine::new(&fs)
        .build_config(&config)
        .expect("build config")
        .artifacts;
    let mut live = LiveMachine::new_freestanding(&artifacts).expect("boot");
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(0));
}

#[test]
fn uart_output_lands_in_the_log_and_stop_keeps_state() {
    let artifacts = build_artifacts(
        "console := import(\"console\")\n\
             main: () -> i32 {\n    console.println(\"live hello\")\n    return 0\n}",
    );
    let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
    drive_to_completion(&mut live);
    assert_eq!(live.state, MachineState::Exited(0));
    assert!(
        live.uart_log.contains("live hello"),
        "uart drained into log"
    );

    let mut stopped = LiveMachine::new_hosted(&artifacts).expect("boot");
    stopped.drive(3, LONG_BUDGET);
    stopped.stop();
    assert_eq!(stopped.state, MachineState::Stopped);
    assert_eq!(stopped.result().exit_code, None);
    assert_eq!(
        stopped.drive(100, LONG_BUDGET),
        0,
        "stopped machines stay put"
    );
}
