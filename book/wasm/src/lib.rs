//! Runs book assembly snippets on the real VM, and re-highlights edited source.

use asm_to_binary::{AssembledOutput, Assembler, parse_asm_source};

// File name the snippet is registered under, so the line table can be read back.
const SNIPPET: &str = "snippet";
use serde::Serialize;
use virtual_machine::RunResult;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

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

/// Gives a fragment an entry point and a SYSCON halt so the VM can run it.
/// The halt needs a scratch register, so `t6` is clobbered.
pub fn wrap_snippet(source: &str) -> String {
    format!(
        ".globl _start\n_start:\n{source}\n\
                 li   t6, 0x10010000\n\
                 sd   zero, 0(t6)\n"
    )
}

fn needs_wrapping(source: &str) -> bool {
    !source.contains("_start")
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
    let program = if bare || !needs_wrapping(source) {
        source.to_owned()
    } else {
        wrap_snippet(source)
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
    let RunResult {
        steps,
        uart_output,
        outcome,
    } = vm.run(max_steps);

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
    syntax_highlight::html::highlight_html(lang, source)
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
}
