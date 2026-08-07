//! Compiles and runs Guide snippets on the real VM, and re-highlights edited source.

use asm_to_binary::{AssembledOutput, Assembler, ObjectLinker, parse_asm_source};
use hll_to_ir::imports::ModuleAlias;
use hll_to_ir::{CompileConfig, HllCompiler, IrType, TargetMode};
use ir_to_asm::CompilerRv64;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use virtual_machine::RunResult;
use virtual_machine::bus::ELF_LOAD_BASE;
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

fn outcome_from_result(vm: &VirtualMachine, result: RunResult) -> Outcome {
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
    outcome_from_result(&vm, result)
}

type HostedObjects = Vec<(String, AssembledOutput)>;
type HostedResult = Result<HostedObjects, String>;

thread_local! {
    static HOSTED_STDLIB: RefCell<Option<HostedResult>> = const { RefCell::new(None) };
}

fn module_member_aliases(
    alias: &str,
    key: &str,
    exports: &HashSet<String>,
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut sorted_exports: Vec<&String> = exports.iter().collect();
    sorted_exports.sort();
    for prefix in [alias, key] {
        let prefix = prefix.trim_end_matches(".hll");
        if prefix.is_empty() {
            continue;
        }
        let prefix = format!("{prefix}_");
        for export in &sorted_exports {
            let Some(short) = export.strip_prefix(&prefix) else {
                continue;
            };
            if short.is_empty() {
                continue;
            }
            match aliases.get(short) {
                Some(existing) if existing != *export => {
                    aliases.remove(short);
                }
                Some(_) => {}
                None => {
                    aliases.insert(short.to_owned(), (*export).clone());
                }
            }
        }
    }
    aliases
}

fn resolve_stdlib_imports(
    source: &str,
    modules: &[(&str, &str)],
) -> Result<(Option<String>, HashMap<String, ModuleAlias>), String> {
    let mut prelude = String::new();
    let mut aliases = HashMap::new();
    for (alias, path) in hll_to_ir::imports::collect_module_imports(source)? {
        let key = path.trim_end_matches(".hll");
        let Some((module_name, module_source)) =
            modules.iter().find(|(module_name, _)| *module_name == key)
        else {
            return Err(format!(
                "HLL sandbox cannot resolve hosted standard-library module `{path}`"
            ));
        };
        let exports = hll_to_ir::imports::collect_exports(module_source)?;
        prelude.push_str(&hll_to_ir::imports::extract_interface(module_source)?);
        aliases.insert(
            alias.clone(),
            ModuleAlias {
                prefix: String::new(),
                mangled: HashSet::new(),
                namespaced: hll_to_ir::imports::collect_exported_interfaces(module_source)?,
                member_aliases: module_member_aliases(&alias, module_name, &exports),
                declared: hll_to_ir::imports::collect_declaration_names(module_source)?,
                exports,
            },
        );
    }
    Ok(((!prelude.is_empty()).then_some(prelude), aliases))
}

fn compile_hll_object(
    name: &str,
    source: &str,
    modules: &[(&str, &str)],
    type_prelude: &[(String, IrType)],
) -> Result<(AssembledOutput, bool), String> {
    let (source_prelude, module_aliases) = resolve_stdlib_imports(source, modules)?;
    let compiler = HllCompiler::new(CompileConfig {
        target: TargetMode::Hosted,
        strict: true,
        string_prefix: Some(format!("__guide_{name}_str_")),
        type_prelude: type_prelude.to_vec(),
        source_prelude,
        module_aliases,
        source_file: Some(name.to_owned()),
        ..CompileConfig::default()
    });
    let output = compiler.compile(source).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render(name))
            .collect::<Vec<_>>()
            .join("\n\n")
    })?;
    let has_main = output
        .ir
        .functions
        .iter()
        .any(|function| function.name == "main");
    let (_, tokens) = CompilerRv64::new().compile_with_tokens(&output.ir);
    let mut assembled = Assembler::assemble(&tokens).map_err(|error| error.message)?;
    let globals: Vec<String> = assembled
        .symbols_iter()
        .filter(|(symbol, _)| !symbol.starts_with('.') && !symbol.contains("__"))
        .map(|(symbol, _)| symbol.to_owned())
        .collect();
    for symbol in globals {
        assembled.mark_entry_global(&symbol);
    }
    Ok((assembled, has_main))
}

fn compile_hosted_stdlib() -> HostedResult {
    let modules = hll_to_ir::stdlib::get_stdlib_modules_for_mode(TargetMode::Hosted);
    let type_prelude = hll_to_ir::stdlib::get_stdlib_type_prelude();
    modules
        .iter()
        .map(|(name, source)| {
            compile_hll_object(name, source, &modules, &type_prelude)
                .map(|(object, _)| ((*name).to_owned(), object))
        })
        .collect()
}

fn hosted_stdlib() -> HostedResult {
    HOSTED_STDLIB.with(|cache| {
        cache
            .borrow_mut()
            .get_or_insert_with(compile_hosted_stdlib)
            .clone()
    })
}

/// Compiles a standalone HLL program and runs its `main` return value as the exit code.
pub fn run_hll(source: &str, max_steps: u64) -> Outcome {
    let stdlib = match hosted_stdlib() {
        Ok(stdlib) => stdlib,
        Err(error) => return Outcome::failed(error),
    };
    let modules = hll_to_ir::stdlib::get_stdlib_modules_for_mode(TargetMode::Hosted);
    let type_prelude = hll_to_ir::stdlib::get_stdlib_type_prelude();
    let (user, has_main) = match compile_hll_object(SNIPPET, source, &modules, &type_prelude) {
        Ok(output) => output,
        Err(error) => return Outcome::failed(error),
    };
    if !has_main {
        return Outcome::failed("HLL sandbox programs need a `main` function");
    }

    let mut link_modules: Vec<(&str, &AssembledOutput)> = stdlib
        .iter()
        .map(|(name, object)| (name.as_str(), object))
        .collect();
    link_modules.push((SNIPPET, &user));
    let mut assembled = match ObjectLinker::link(&link_modules) {
        Ok(assembled) => assembled,
        Err(error) => return Outcome::failed(error.message),
    };
    assembled.mark_entry_global("_start");
    let elf = assembled.to_elf(ELF_LOAD_BASE);
    let mut vm = match VirtualMachine::from_elf(&elf) {
        Ok(vm) => vm,
        Err(error) => return Outcome::failed(format!("could not load HLL program: {error}")),
    };
    let result = vm.run(max_steps);
    outcome_from_result(&vm, result)
}

/// Runs one of the source languages supported by editable Guide examples.
pub fn run_source(lang: &str, source: &str, max_steps: u64, bare: bool) -> Outcome {
    match lang {
        "asm" => run(source, max_steps, bare),
        "hll" => run_hll(source, max_steps),
        _ => Outcome::failed(format!("runnable `{lang}` blocks are not supported")),
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
pub fn run_code(lang: &str, source: &str, max_steps: u32, bare: bool) -> JsValue {
    let outcome = run_source(lang, source, u64::from(max_steps), bare);
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
    fn hll_runs_through_the_compiler_backend_and_vm() {
        let source = "main: () -> i32 {\n    return 20 + 5\n}\n";
        let outcome = run_hll(source, STEPS);

        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(outcome.exit_code, Some(25));
        let a0 = outcome
            .registers
            .iter()
            .find(|register| register.name == "a0")
            .expect("a0 is always reported");
        assert_eq!(a0.value, "0x19");
    }

    #[test]
    fn every_runnable_guide_example_executes_and_matches_expectations() {
        let markdown = include_str!("../../src/written.md");
        let mut runnable = false;
        let mut bare = false;
        let mut language = String::new();
        let mut expected = Vec::new();
        let mut source = String::new();
        let mut count = 0;
        let mut expectation_count = 0;

        for line in markdown.lines() {
            let fence = line.strip_prefix("```");
            if !runnable
                && fence.is_some_and(|info| {
                    (info.starts_with("asm,") || info.starts_with("hll,"))
                        && info.contains("runnable")
                })
            {
                runnable = true;
                let info = fence.expect("checked above");
                language = info.split(',').next().unwrap_or("").to_owned();
                bare = info.starts_with("asm,runnable=bare");
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
                let outcome = run_source(&language, &source, 100_000, bare);
                assert!(
                    outcome.error.is_none(),
                    "{language} example {count}: {:?}",
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
                } else if source.contains("arr: i32* = new(i32, count)") {
                    assert_eq!(outcome.uart, "150");
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

        assert!(!runnable, "unterminated runnable code fence");
        assert!(count > 0, "the Guide should contain runnable examples");
        assert!(
            expectation_count > 0,
            "the Guide should declare expected values"
        );
    }
}
