use asm_to_binary::assembler::Assembler;
use asm_to_binary::rv_instruction::RvInstruction;
use riscv_model::InstructionId;
use virtual_machine::cpu::retirement::RETIRE_EVENT_VERSION;
use virtual_machine::cpu::{TrapStage, traps};
use virtual_machine::{FunctionalMachine, FunctionalStepOutcome, RunOutcome};

fn assemble(source: &str) -> asm_to_binary::AssembledOutput {
    let tokens = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if let Some(name) = line.strip_suffix(':') {
                RvInstruction::Label(name.to_owned())
            } else if line.starts_with('.') {
                RvInstruction::Directive(line.to_owned())
            } else {
                RvInstruction::Directive(format!("\t{line}"))
            }
        })
        .collect::<Vec<_>>();
    Assembler::assemble(&tokens).expect("assembly should succeed")
}

#[test]
fn functional_step_reports_committed_register_effects() {
    let output = assemble(
        "
        .section .text
        .globl _start
        _start:
            addi a0, zero, 7
            li a7, 93
            ecall
        ",
    );
    let mut machine = FunctionalMachine::new(&output);

    let FunctionalStepOutcome::Continue(event) = machine.step().expect("step should succeed")
    else {
        panic!("first instruction must not halt");
    };
    assert_eq!(event.version, RETIRE_EVENT_VERSION);
    assert_eq!(event.sequence, 0);
    assert_eq!(event.instruction, Some(InstructionId::Addi));
    assert!(event.retired);
    assert_eq!(event.integer_writes.len(), 1);
    assert_eq!(event.integer_writes[0].index, 10);
    assert_eq!(event.integer_writes[0].value, 7);
    assert_eq!(event.next_pc, event.pc + 4);
}

#[test]
fn functional_hosted_program_traps_and_exits() {
    let output = assemble(
        "
        .section .text
        .globl _start
        _start:
            li a7, 93
            li a0, 7
            ecall
        ",
    );
    let mut machine = FunctionalMachine::new(&output);
    let result = machine.run(100);

    assert_eq!(result.termination, RunOutcome::Halted(7));
}

#[test]
fn functional_ecall_reports_a_precise_non_retired_trap() {
    let output = assemble(
        "
        .section .text
        .globl _start
        _start:
            ecall
        ",
    );
    let mut machine = FunctionalMachine::new(&output);

    let FunctionalStepOutcome::Continue(event) = machine.step().expect("step should succeed")
    else {
        panic!("trap entry must not halt");
    };
    let trap = event.trap.expect("ecall must report its trap");
    assert!(!event.retired);
    assert_eq!(event.instruction, Some(InstructionId::Ecall));
    assert_eq!(trap.cause, traps::CAUSE_ECALL_M);
    assert_eq!(trap.stage, TrapStage::Writeback);
    assert_eq!(event.next_pc, virtual_machine::rom::M_TRAP_ADDR);
    assert_eq!(machine.architectural_state().csrs().instret, 0);
}

#[test]
fn functional_kernel_boots_through_rom_and_exits_from_supervisor_mode() {
    let output = assemble(
        "
        .section .text
        .globl _kernel_start
        _kernel_start:
            li a7, 93
            li a0, 42
            ecall
        ",
    );
    let mut machine = FunctionalMachine::new_kernel(&output);
    let result = machine.run(100);

    assert_eq!(result.termination, RunOutcome::Halted(42));
}

#[test]
fn functional_trace_records_physical_memory_writes() {
    let output = assemble(
        "
        .section .text
        .globl _start
        _start:
            li t0, 0x80001000
            li t1, 42
            sd t1, 0(t0)
            li a7, 93
            li a0, 0
            ecall
        ",
    );
    let mut machine = FunctionalMachine::new(&output);
    let mut write = None;

    for _ in 0..8 {
        let outcome = machine.step().expect("step should succeed");
        let event = match outcome {
            FunctionalStepOutcome::Continue(event)
            | FunctionalStepOutcome::Halted { event, .. } => event,
        };
        if event.memory_write.is_some() {
            write = event.memory_write;
            break;
        }
    }

    let write = write.expect("store must appear in the trace");
    assert_eq!(write.physical_address, 0x8000_1000);
    assert_eq!(write.size, 8);
    assert_eq!(write.value, 42);
}
