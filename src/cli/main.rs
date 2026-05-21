//! `fsc` -- Full Stack CLI
//!
//! Subcommands:
//!   hll-to-ir  Compile an HLL source file and print the IR.
//!   hll-to-asm Compile an HLL source file and print the RISC-V assembly.
//!   run        Compile (or load) a program and execute it in the VM.
//!   help       Print usage.

#![allow(
    clippy::print_stdout,
    reason = "CLI binary writes program output to stdout"
)]
#![allow(
    clippy::print_stderr,
    reason = "CLI binary writes errors and diagnostics to stderr"
)]
#![warn(rust_2018_idioms)]

use asm_to_binary::AssembledOutput;
use asm_to_binary::rv_instruction::RvInstruction;
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::get_stdlib_source_for_mode;
use std::fmt;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum CliError {
    Usage(String),
    Io(std::io::Error),
    Compile(String),
    Assemble(String),
    Timeout(u64),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(msg) => write!(f, "{msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Compile(msg) => write!(f, "compilation error: {msg}"),
            Self::Assemble(msg) => write!(f, "assembler error: {msg}"),
            Self::Timeout(steps) => write!(f, "execution timed out after {steps} steps"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

// ---------------------------------------------------------------------------
// Argument model
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Subcommand {
    HllToIr,
    HllToAsm,
    Run,
    Help,
}

#[derive(Debug)]
struct Args {
    subcmd: Subcommand,
    input: Option<String>,
    output: Option<String>,
    mode: TargetMode,
    max_steps: u64,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    match run_cli() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("fsc: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<ExitCode, CliError> {
    let args = parse_args()?;
    match args.subcmd {
        Subcommand::HllToIr => cmd_hll_to_ir(&args),
        Subcommand::HllToAsm => cmd_hll_to_asm(&args),
        Subcommand::Run => cmd_run(&args),
        Subcommand::Help => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args() -> Result<Args, CliError> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = raw.iter();

    let subcmd = match iter.next().map(|s| s.as_str()) {
        Some("hll-to-ir") => Subcommand::HllToIr,
        Some("hll-to-asm") => Subcommand::HllToAsm,
        Some("run") => Subcommand::Run,
        Some("help" | "--help" | "-h") | None => Subcommand::Help,
        Some(other) => {
            return Err(CliError::Usage(format!(
                "unknown subcommand `{other}`\n\nRun `fsc help` for usage."
            )));
        }
    };

    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut mode = TargetMode::Hosted;
    let mut max_steps = 50_000_000u64;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                let path = iter
                    .next()
                    .ok_or_else(|| CliError::Usage("--output requires a file path".to_owned()))?;
                output = Some(path.clone());
            }
            "--mode" | "-m" => {
                let val = iter.next().ok_or_else(|| {
                    CliError::Usage("--mode requires `hosted` or `freestanding`".to_owned())
                })?;
                mode = parse_mode(val.as_str())?;
            }
            "--max-steps" => {
                let val = iter.next().ok_or_else(|| {
                    CliError::Usage("--max-steps requires a positive integer".to_owned())
                })?;
                max_steps = val
                    .parse::<u64>()
                    .map_err(|_| CliError::Usage(format!("`{val}` is not a valid step count")))?;
            }
            other if !other.starts_with('-') => {
                input = Some(other.to_owned());
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unknown option `{other}`\n\nRun `fsc help` for usage."
                )));
            }
        }
    }

    Ok(Args {
        subcmd,
        input,
        output,
        mode,
        max_steps,
    })
}

fn parse_mode(s: &str) -> Result<TargetMode, CliError> {
    match s {
        "hosted" => Ok(TargetMode::Hosted),
        "freestanding" => Ok(TargetMode::Freestanding),
        other => Err(CliError::Usage(format!(
            "unknown mode `{other}`; expected `hosted` or `freestanding`"
        ))),
    }
}

// ---------------------------------------------------------------------------
// hll-to-ir
// ---------------------------------------------------------------------------

fn cmd_hll_to_ir(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_input(args)?;
    let src = fs::read_to_string(input)?;

    let mut pipeline = make_pipeline(args.mode, "_u_");
    pipeline.set_run_semantic_analysis(false);

    let result = pipeline
        .compile(&src)
        .map_err(|e| CliError::Compile(e.to_string()))?;

    let ir_text = result.ir_program.to_string();

    write_or_print(args.output.as_deref(), &ir_text)?;

    for diag in &result.diagnostics {
        eprintln!("warning: {}", diag.format_full());
    }

    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// hll-to-asm
// ---------------------------------------------------------------------------

fn cmd_hll_to_asm(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_input(args)?;
    let src = fs::read_to_string(input)?;

    let mut pipeline = make_pipeline(args.mode, "_u_");
    pipeline.set_run_semantic_analysis(false);

    let result = pipeline
        .compile(&src)
        .map_err(|e| CliError::Compile(e.to_string()))?;

    let asm_text = pipeline.compile_ir_to_assembly(&result.ir_program);

    write_or_print(args.output.as_deref(), &asm_text)?;

    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_input(args)?;

    let assembled = if has_extension(input, "s") {
        eprintln!(
            "fsc: note: loading `.s` as assembly text; \
             instructions not in the built-in parser may be silently skipped"
        );
        assemble_from_s_file(input, args.mode)?
    } else {
        assemble_from_hll_file(input, args.mode)?
    };

    let mut vm = VirtualMachine::new(&assembled);
    let result = vm.run(args.max_steps);

    print!("{}", result.uart_output);

    match result.outcome {
        StepOutcome::Halted(code) => {
            eprintln!("fsc: program exited with code {code}");
            #[allow(
                clippy::cast_possible_truncation,
                reason = "POSIX exit codes are 0-255"
            )]
            Ok(ExitCode::from((code & 0xFF) as u8))
        }
        StepOutcome::Continue => Err(CliError::Timeout(args.max_steps)),
    }
}

// ---------------------------------------------------------------------------
// Compilation helpers
// ---------------------------------------------------------------------------

/// Compile an HLL source file with the stdlib and return assembled output.
fn assemble_from_hll_file(path: &str, mode: TargetMode) -> Result<AssembledOutput, CliError> {
    let src = fs::read_to_string(path)?;
    compile_and_link(&src, mode)
}

/// Parse a `.s` text file as assembly tokens, link with stdlib, and assemble.
fn assemble_from_s_file(path: &str, mode: TargetMode) -> Result<AssembledOutput, CliError> {
    let asm_text = fs::read_to_string(path)?;
    let stdlib_tokens = compile_stdlib_tokens(mode)?;
    let user_tokens = asm_text_to_tokens(&asm_text);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);

    let pipeline = make_pipeline(mode, "_u_");
    pipeline
        .assemble_linked(&linked)
        .map_err(|e| CliError::Assemble(e.to_string()))
}

/// Compile HLL source → IR → tokens, then link with stdlib and assemble.
fn compile_and_link(src: &str, mode: TargetMode) -> Result<AssembledOutput, CliError> {
    let stdlib_tokens = compile_stdlib_tokens(mode)?;

    let mut user_pipeline = make_pipeline(mode, "_u_");
    user_pipeline.set_run_semantic_analysis(false);

    let user_result = user_pipeline
        .compile(src)
        .map_err(|e| CliError::Compile(e.to_string()))?;

    let (_, user_tokens) =
        user_pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);

    user_pipeline
        .assemble_linked(&linked)
        .map_err(|e| CliError::Assemble(e.to_string()))
}

/// Compile the platform stdlib for `mode` and return the assembly token stream.
fn compile_stdlib_tokens(mode: TargetMode) -> Result<Vec<RvInstruction>, CliError> {
    let stdlib_src = get_stdlib_source_for_mode(mode);
    let mut stdlib_pipeline = make_pipeline(mode, "_s_");
    stdlib_pipeline.set_run_semantic_analysis(false);

    let stdlib_result = stdlib_pipeline
        .compile(&stdlib_src)
        .map_err(|e| CliError::Compile(format!("stdlib: {e}")))?;

    let (_, tokens) = stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);

    Ok(tokens)
}

/// Wrap each line of assembly text in a `Directive` token.
///
/// The assembler's pass-0 parser recognises labels (`name:`), section
/// directives (`.text`, `.asciz`, …) and most instruction mnemonics from
/// lines that begin with a tab or space.  Unrecognised mnemonics become
/// no-op comments.
fn asm_text_to_tokens(text: &str) -> Vec<RvInstruction> {
    text.lines()
        .map(|line| RvInstruction::Directive(line.to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------
// Pipeline factory
// ---------------------------------------------------------------------------

fn make_pipeline(mode: TargetMode, string_prefix: &str) -> CompilationPipeline {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(mode);
    p.set_string_prefix(Some(string_prefix.to_owned()));
    p
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

fn require_input<'a>(args: &'a Args) -> Result<&'a str, CliError> {
    args.input.as_deref().ok_or_else(|| {
        CliError::Usage("an input file is required\n\nRun `fsc help` for usage.".to_owned())
    })
}

fn has_extension(path: &str, ext: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn write_or_print(output: Option<&str>, content: &str) -> Result<(), CliError> {
    match output {
        Some(path) => Ok(fs::write(path, content)?),
        None => {
            println!("{content}");
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

fn print_help() {
    println!(
        "Full Stack CLI\n\
         \n\
         USAGE\n\
         \n\
             fsc <subcommand> [options]\n\
         \n\
         SUBCOMMANDS\n\
         \n\
             hll-to-ir  <input.hll>   Compile HLL source to IR (printed to stdout)\n\
             hll-to-asm <input.hll>   Compile HLL source to RISC-V assembly\n\
             run        <input>       Compile and run through the VM\n\
             help                     Show this message\n\
         \n\
         OPTIONS\n\
         \n\
             -o, --output <path>          Write output to <path> instead of stdout\n\
             -m, --mode   hosted|freestanding\n\
                                          Target mode (default: hosted)\n\
                 --max-steps <n>          VM step limit for `run` (default: 50000000)\n\
         \n\
         EXAMPLES\n\
         \n\
             fsc hll-to-ir  program.hll\n\
             fsc hll-to-ir  program.hll -o program.ir\n\
             fsc hll-to-asm program.hll -o program.s\n\
             fsc run        program.hll\n\
             fsc run        program.s   --max-steps 1000000\n\
             fsc run        program.hll --mode freestanding\n\
         \n\
         NOTE\n\
         \n\
             `run program.s` parses the assembly text with a built-in subset parser.\n\
             Some instructions may be silently skipped.  Prefer `run program.hll`\n\
             for reliable results.\n\
        "
    );
}
