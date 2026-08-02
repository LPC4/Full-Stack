//! Runs Guide assembly snippets on the real VM, and re-highlights edited source.

use asm_to_binary::{AssembledOutput, Assembler, parse_asm_source};
use serde::Serialize;
use virtual_machine::RunResult;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// File name the snippet is registered under, so the line table can be read back.
const SNIPPET: &str = "snippet";
const SNIPPET_DONE: &str = "__full_stack_guide_snippet_done";

const ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

#[derive(Serialize, Default)]
pub struct Register {
    pub name: &'static str,
    pub value: String,
}

#[derive(Serialize, Default)]
pub struct Outcome {
    pub error: Option<String>,
    pub uart: String,
    pub exit_code: Option<i32>,
    pub steps: u64,
    pub max_steps_reached: bool,
    pub pc: String,
    pub registers: Vec<Register>,
}

impl Outcome {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            error: Some(message.into()),
            ..Default::default()
        }
    }
}

/// Gives a fragment an entry point and a register-neutral completion marker.
pub fn wrap_snippet(source: &str) -> String {
    format!(
        ".globl _start\n_start:\n{source}\n\
         {SNIPPET_DONE}:\n\
                 nop\n\
         __full_stack_guide_snippet_park:\n\
                 j __full_stack_guide_snippet_park\n"
    )
}

fn run_to_marker(vm: &mut VirtualMachine, marker_pc: u64, max_steps: u64) -> RunResult {
    let mut steps = 0;
    let mut outcome = StepOutcome::Continue;

    for _ in 0..max_steps {
        steps += 1;
        match vm.step() {
            Ok(StepOutcome::Continue) => {
                let marker_retired =
                    vm.pipeline_snapshot().stages[4].is_some_and(|(pc, _)| pc == marker_pc);
                if marker_retired {
                    outcome = StepOutcome::Halted(0);
                    break;
                }
            }
            Ok(StepOutcome::Halted(code)) => {
                outcome = StepOutcome::Halted(code);
                break;
            }
            Err(_) => {
                outcome = StepOutcome::Halted(-1);
                break;
            }
        }
    }

    let uart_output = String::from_utf8_lossy(&vm.uart_output()).into_owned();
    RunResult {
        steps,
        uart_output,
        outcome,
    }
}

fn needs_wrapping(source: &str) -> bool {
    !source.lines().any(|line| {
        line.split(['#', ';'])
            .next()
            .unwrap_or("")
            .trim_start()
            .starts_with("_start:")
    })
}

// True if the line should have produced machine code.
fn is_code_line(line: &str) -> bool {
    let body = line.split(['#', ';']).next().unwrap_or("").trim();
    let body = match body.split_once(':') {
        Some((_, rest)) => rest.trim(),
        None => body,
    };
    !body.is_empty() && !body.starts_with('.')
}

/// Lines that produced no machine code. The assembler only warns on those, so
/// without this check a typo would silently assemble to a program that does nothing.
fn unrecognised_lines(program: &str, assembled: &AssembledOutput) -> Vec<usize> {
    let emitted: std::collections::HashSet<usize> = assembled
        .line_table_iter()
        .map(|entry| entry.line as usize)
        .collect();

    program
        .lines()
        .enumerate()
        .filter(|(index, line)| is_code_line(line) && !emitted.contains(&(index + 1)))
        .map(|(index, _)| index + 1)
        .collect()
}

/// Assembles and runs `source`, returning what the reader should see.
pub fn run(source: &str, max_steps: u64, bare: bool) -> Outcome {
    let wrapped = !bare && needs_wrapping(source);
    let program = if wrapped {
        wrap_snippet(source)
    } else {
        source.to_owned()
    };

    let tokens = parse_asm_source(Some(SNIPPET), &program);
    let assembled = match Assembler::assemble(&tokens) {
        Ok(assembled) => assembled,
        Err(error) => return Outcome::failed(error.message),
    };

    let skipped = unrecognised_lines(&program, &assembled);
    if !skipped.is_empty() {
        let lines: Vec<&str> = program.lines().collect();
        let detail = skipped
            .iter()
            .take(3)
            .map(|n| format!("  line {n}: {}", lines[n - 1].trim()))
            .collect::<Vec<_>>()
            .join("\n");
        return Outcome::failed(format!("could not assemble:\n{detail}"));
    }

    let mut vm = VirtualMachine::new(&assembled);
    let result = if wrapped {
        let marker = assembled.symbol_address(SNIPPET_DONE);
        let start = assembled.symbol_address("_start");
        let marker_pc = marker
            .zip(start)
            .and_then(|(marker, start)| marker.checked_sub(start))
            .and_then(|offset| vm.peek_pc().checked_add(offset));
        let Some(marker_pc) = marker_pc else {
            return Outcome::failed("sandbox completion marker is missing");
        };
        run_to_marker(&mut vm, marker_pc, max_steps)
    } else {
        vm.run(max_steps)
    };
    let RunResult {
        steps,
        uart_output,
        outcome,
    } = result;

    let registers = vm.peek_all_xregs();
    Outcome {
        error: None,
        uart: uart_output,
        exit_code: match outcome {
            StepOutcome::Halted(code) => Some(code as i32),
            StepOutcome::Continue => None,
        },
        steps,
        max_steps_reached: matches!(outcome, StepOutcome::Continue),
        pc: format!("{:#x}", vm.peek_pc()),
        registers: ABI_NAMES
            .iter()
            .zip(registers)
            .map(|(name, value)| Register {
                name,
                value: format!("{value:#x}"),
            })
            .collect(),
    }
}

// --- wasm bindings ---

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_asm(source: &str, max_steps: u32, bare: bool) -> JsValue {
    let outcome = run(source, u64::from(max_steps), bare);
    serde_wasm_bindgen::to_value(&outcome).unwrap_or(JsValue::NULL)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn highlight_html(lang: &str, source: &str) -> String {
    fs_utils::syntax_highlight::html::highlight_html(lang, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEPS: u64 = 2_000_000;

    // Mirrors programs/lessons/01_sum: the sum of 1..10 is 55, shown as 0x37.
    #[test]
    fn runs_a_snippet_and_reports_registers() {
        let source = "  li   t0, 0\n  li   t1, 1\n  li   t2, 10\nloop:\n  add  t0, t0, t1\n  addi t1, t1, 1\n  ble  t1, t2, loop\n";
        let outcome = run(source, STEPS, false);

        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let t0 = outcome
            .registers
            .iter()
            .find(|r| r.name == "t0")
            .expect("t0 present");
        assert_eq!(t0.value, "0x37");
        assert!(!outcome.max_steps_reached, "the wrapper should halt it");
        assert_eq!(outcome.exit_code, Some(0));
    }

    #[test]
    fn wrapper_preserves_the_last_temporary_register() {
        let outcome = run("  li t6, 15\n", STEPS, false);

        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let t6 = outcome
            .registers
            .iter()
            .find(|register| register.name == "t6")
            .expect("t6 is always reported");
        assert_eq!(t6.value, "0xf");
        assert_eq!(outcome.exit_code, Some(0));
    }

    // The real lesson file already has an entry point, so it must not be wrapped.
    #[test]
    fn runs_a_whole_lesson_program_unwrapped() {
        let source = include_str!("../../../programs/lessons/01_sum/sum.s");
        let outcome = run(source, 100_000, true);

        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let t0 = outcome
            .registers
            .iter()
            .find(|r| r.name == "t0")
            .expect("t0 is always reported");
        assert_eq!(t0.value, "0x37");
        // It spins in `j done`, so it never halts on its own.
        assert!(outcome.max_steps_reached);
    }

    #[test]
    fn a_snippet_that_already_has_an_entry_point_is_left_alone() {
        let source = ".globl _start\n_start:\n  li t0, 7\n  li t6, 0x10010000\n  sd zero, 0(t6)\n";
        let outcome = run(source, STEPS, false);
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(outcome.exit_code, Some(0));
    }

    #[test]
    fn mentioning_start_does_not_suppress_the_wrapper() {
        let outcome = run(
            "# _start is supplied by the sandbox\n  li t0, 7\n",
            STEPS,
            false,
        );
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(outcome.exit_code, Some(0));
    }

    #[test]
    fn assembler_errors_come_back_as_data() {
        let outcome = run("  li t0, \n  nonsense q1, q2\n", STEPS, false);
        assert!(outcome.error.is_some(), "expected an assembler error");
        assert!(outcome.registers.is_empty());
    }

    #[test]
    fn uart_writes_reach_the_reader() {
        let source = "  li   t0, 0x10000000\n  li   t1, 72\n  sb   t1, 0(t0)\n  li   t1, 105\n  sb   t1, 0(t0)\n";
        let outcome = run(source, STEPS, false);
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(outcome.uart, "Hi");
    }

    // A runaway loop must stop at the budget rather than hang the page.
    #[test]
    fn infinite_loops_stop_at_the_step_limit() {
        let outcome = run("spin:\n  j spin\n", 50_000, false);
        assert!(outcome.error.is_none());
        assert!(outcome.max_steps_reached);
        assert_eq!(outcome.steps, 50_000);
    }

    #[test]
    fn every_runnable_guide_example_executes_and_matches_expectations() {
        let markdown = include_str!("../../src/written.md");
        let mut runnable = false;
        let mut bare = false;
        let mut expected = Vec::new();
        let mut source = String::new();
        let mut count = 0;
        let mut expectation_count = 0;

        for line in markdown.lines() {
            if !runnable && line.starts_with("```asm,runnable") {
                runnable = true;
                bare = line.starts_with("```asm,runnable=bare");
                expected = line
                    .split_whitespace()
                    .find_map(|part| part.strip_prefix("expect="))
                    .into_iter()
                    .flat_map(|values| values.split('|'))
                    .map(|value| value.split_once(':').expect("name:value expectation"))
                    .map(|(name, value)| {
                        (
                            name.to_owned(),
                            value
                                .parse::<u64>()
                                .expect("decimal expected register value"),
                        )
                    })
                    .collect();
                source.clear();
            } else if runnable && line == "```" {
                let outcome = run(&source, 100_000, bare);
                assert!(
                    outcome.error.is_none(),
                    "example {count}: {:?}",
                    outcome.error
                );
                assert_ne!(outcome.exit_code, Some(-1), "example {count} trapped");
                if source.contains(".asciz \"Hello, world!") {
                    assert_eq!(
                        outcome.uart, "Hello, world!\n",
                        "pc={}, steps={}, exit={:?}, limited={}",
                        outcome.pc, outcome.steps, outcome.exit_code, outcome.max_steps_reached
                    );
                } else if source.contains("# 'H'") {
                    assert_eq!(outcome.uart, "Hi\n");
                }
                for (name, expected_value) in &expected {
                    let register = outcome
                        .registers
                        .iter()
                        .find(|register| register.name == name)
                        .unwrap_or_else(|| panic!("example {count}: missing register {name}"));
                    let actual = u64::from_str_radix(
                        register
                            .value
                            .strip_prefix("0x")
                            .expect("hex register prefix"),
                        16,
                    )
                    .expect("hex register value");
                    assert_eq!(actual, *expected_value, "example {count}: register {name}");
                    expectation_count += 1;
                }
                count += 1;
                runnable = false;
            } else if runnable {
                source.push_str(line);
                source.push('\n');
            }
        }

        assert!(!runnable, "unterminated runnable assembly fence");
        assert!(count > 0, "the Guide should contain runnable examples");
        assert!(
            expectation_count > 0,
            "the Guide should declare expected values"
        );
    }
}
