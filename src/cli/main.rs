//! `fsc`: Full Stack CLI
//!
//! Subcommands:
//!   build      Compile an HLL source or `.build` manifest.
//!   link       Compile and link multiple HLL source files into an ELF.
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
#![expect(
    clippy::map_err_ignore,
    reason = "CLI maps parse failures to stable user-facing diagnostics"
)]
#![warn(rust_2018_idioms)]

use asm_to_binary::AssembledOutput;
use asm_to_binary::rv_instruction::RvInstruction;
use full_stack::build::{BuildExecutor, BuildManifest, BuildSource, ImportClosurePolicy};
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use full_stack::runconfig::{BuildRef, RunConfig, RunKind, run_config_from_manifest};
use full_stack::session::{BuildOutcome, LiveMachine, MachineState, SessionEngine};
use full_stack::workspace::{NativeFs, VPath};
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// --- Error type ---

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
            Self::Assemble(msg) => write!(f, "linker/assembler error: {msg}"),
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

// --- Argument model ---

#[derive(Debug)]
enum Subcommand {
    Build,
    Link,
    HllToIr,
    HllToAsm,
    Run,
    Help,
}

#[derive(Debug)]
struct Args {
    subcmd: Subcommand,
    inputs: Vec<String>,
    output: Option<String>,
    mode: TargetMode,
    max_steps: u64,
    emit_object: bool,
}

impl Args {
    fn first_input(&self) -> Option<&str> {
        self.inputs.first().map(|s| s.as_str())
    }
}

// --- Entry point ---

fn main() -> ExitCode {
    run_cli().unwrap_or_else(|err| {
        eprintln!("fsc: {err}");
        ExitCode::FAILURE
    })
}

fn run_cli() -> Result<ExitCode, CliError> {
    let args = parse_args()?;
    match args.subcmd {
        Subcommand::Build => cmd_build(&args),
        Subcommand::Link => cmd_link(&args),
        Subcommand::HllToIr => cmd_hll_to_ir(&args),
        Subcommand::HllToAsm => cmd_hll_to_asm(&args),
        Subcommand::Run => cmd_run(&args),
        Subcommand::Help => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
    }
}

// --- Argument parsing ---

fn parse_args() -> Result<Args, CliError> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = raw.iter();

    let subcmd = match iter.next().map(|s| s.as_str()) {
        Some("link") => Subcommand::Link,
        Some("build") => Subcommand::Build,
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

    let mut inputs: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut mode = TargetMode::Hosted;
    let mut max_steps = 50_000_000u64;
    let mut emit_object = false;

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
            "--emit-o" => {
                emit_object = true;
            }
            other if !other.starts_with('-') => {
                inputs.push(other.to_owned());
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
        inputs,
        output,
        mode,
        max_steps,
        emit_object,
    })
}

fn parse_mode(s: &str) -> Result<TargetMode, CliError> {
    match s {
        "hosted" => Ok(TargetMode::Hosted),
        "freestanding" => Ok(TargetMode::Freestanding),
        "kernel" => Ok(TargetMode::Kernel),
        other => Err(CliError::Usage(format!(
            "unknown mode `{other}`; expected `hosted`, `freestanding`, or `kernel`"
        ))),
    }
}

// --- build ---

fn cmd_build(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_single_input(args)?;
    let (fs, config) = config_for_cli_input(input, args.mode)?;
    let engine = SessionEngine::new(&fs);
    let outcome = engine.build_config(&config).map_err(map_session_error)?;

    if let Some(output_path) = args.output.as_deref() {
        fs::write(output_path, linked_elf_bytes(&outcome))?;
        eprintln!("fsc: wrote linked ELF to `{output_path}`");
    }
    print_build_summary(&outcome);
    Ok(ExitCode::SUCCESS)
}

// --- link ---

fn cmd_link(args: &Args) -> Result<ExitCode, CliError> {
    if args.inputs.is_empty() {
        return Err(CliError::Usage(
            "`link` requires at least one input file\n\nRun `fsc help` for usage.".to_owned(),
        ));
    }

    if args.inputs.len() == 1 && has_extension(&args.inputs[0], "build") {
        let (fs, config) = config_for_cli_input(&args.inputs[0], args.mode)?;
        let engine = SessionEngine::new(&fs);
        let outcome = engine.build_config(&config).map_err(map_session_error)?;
        let elf_bytes = linked_elf_bytes(&outcome);
        let output_path = args.output.as_deref().unwrap_or("out.elf");
        fs::write(output_path, &elf_bytes)?;
        eprintln!("fsc: wrote linked ELF to `{output_path}`");
        return Ok(ExitCode::SUCCESS);
    }

    let first = args.first_input().expect("inputs checked above");
    let stem = source_stem(first, "linked");
    let mut manifest = BuildManifest::hosted(stem.clone(), "");
    manifest.target = args.mode;
    manifest.root = BuildSource::path(stem.clone(), first);
    if args.mode == TargetMode::Kernel {
        manifest.import_closure = ImportClosurePolicy::Enabled {
            mangle_symbols: false,
        };
    }
    if args.inputs.len() > 1 {
        return Err(CliError::Usage(
            "multiple HLL inputs are no longer linked positionally; use a .build root with \
             import(...) dependencies"
                .to_owned(),
        ));
    }

    let artifacts = BuildExecutor::build(&manifest).map_err(|e| match e {
        full_stack::build::BuildError::Compile(err) => CliError::Compile(err.to_string()),
        other => CliError::Assemble(other.to_string()),
    })?;
    let pipeline = make_pipeline(args.mode, "_u_");
    let linked = artifacts.linked;
    let elf_bytes = linked.to_elf_with_entry(
        pipeline.effective_load_base(),
        pipeline.effective_entry_point(),
    );

    let output_path = args.output.as_deref().unwrap_or("out.elf");
    fs::write(output_path, &elf_bytes)?;
    eprintln!("fsc: wrote linked ELF to `{output_path}`");

    Ok(ExitCode::SUCCESS)
}

// --- hll-to-ir ---

fn cmd_hll_to_ir(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_single_input(args)?;
    let src = fs::read_to_string(input)?;

    let mut pipeline = make_pipeline(args.mode, "_u_");
    pipeline.set_run_semantic_analysis(false);
    pipeline.set_artifact_stem(Some(source_stem(input, "module")));
    pipeline.set_current_source_path(Some(input.to_owned()));

    let result = pipeline
        .compile(&src)
        .map_err(|e| CliError::Compile(e.to_string()))?;

    let ir_text = result.ir_program.to_string();

    write_or_print(args.output.as_deref(), &ir_text)?;

    for diag in &result.diagnostics {
        eprintln!("{}", diag.render(input));
    }

    Ok(ExitCode::SUCCESS)
}

// --- hll-to-asm ---

fn cmd_hll_to_asm(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_single_input(args)?;
    let src = fs::read_to_string(input)?;

    let mut pipeline = make_pipeline(args.mode, "_u_");
    pipeline.set_run_semantic_analysis(false);
    pipeline.set_artifact_stem(Some(source_stem(input, "module")));
    pipeline.set_current_source_path(Some(input.to_owned()));

    let result = pipeline
        .compile(&src)
        .map_err(|e| CliError::Compile(e.to_string()))?;
    let (_, tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&result.ir_program)
        .map_err(|error| CliError::Compile(error.to_string()))?;
    let assembled = pipeline
        .assemble(&tokens)
        .map_err(|e| CliError::Assemble(e.to_string()))?;

    if args.emit_object {
        let object_name = Path::new(input)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("module.o");
        let object_bytes = assembled.to_object(object_name);
        let output_path = args.output.as_deref().ok_or_else(|| {
            CliError::Usage("--emit-o requires --output <path> to write the object file".to_owned())
        })?;
        fs::write(output_path, object_bytes)?;
    } else {
        let asm_text = pipeline
            .compile_ir_to_assembly(&result.ir_program)
            .map_err(|error| CliError::Compile(error.to_string()))?;
        write_or_print(args.output.as_deref(), &asm_text)?;
    }

    Ok(ExitCode::SUCCESS)
}

// --- run ---

fn cmd_run(args: &Args) -> Result<ExitCode, CliError> {
    let input = require_single_input(args)?;

    let assembled = if has_extension(input, "s") {
        eprintln!(
            "fsc: note: loading `.s` as assembly text; \
             instructions not in the built-in parser may be silently skipped"
        );
        assemble_from_s_file(input, args.mode)?
    } else {
        return run_cli_config(input, args.mode, args.max_steps);
    };

    let mut vm = VirtualMachine::new(&assembled);
    let result = vm.run(args.max_steps);

    print!("{}", result.uart_output);

    match result.outcome {
        StepOutcome::Halted(code) => {
            eprintln!("fsc: program exited with code {code}");
            Ok(ExitCode::from((code & 0xFF) as u8))
        }
        StepOutcome::Continue => Err(CliError::Timeout(args.max_steps)),
    }
}

// --- Compilation helpers ---

fn run_cli_config(input: &str, mode: TargetMode, max_steps: u64) -> Result<ExitCode, CliError> {
    let (fs, config) = config_for_cli_input(input, mode)?;
    let engine = SessionEngine::new(&fs);
    match config.kind {
        RunKind::Hosted => {
            let outcome = engine
                .run_hosted_config(&config, max_steps)
                .map_err(map_session_error)?;
            print!("{}", outcome.vm.uart_output);
            if outcome.vm.max_steps_reached {
                return Err(CliError::Timeout(max_steps));
            }
            let code = outcome.vm.exit_code.unwrap_or(1);
            eprintln!("fsc: program exited with code {code}");
            Ok(ExitCode::from((code & 0xFF) as u8))
        }
        RunKind::KernelBoot => run_kernel_config(&engine, &config, max_steps),
        RunKind::Freestanding => {
            let outcome = engine.build_config(&config).map_err(map_session_error)?;
            let mut vm = VirtualMachine::new(&outcome.artifacts.linked);
            let result = vm.run(max_steps);
            print!("{}", result.uart_output);
            match result.outcome {
                StepOutcome::Halted(code) => {
                    eprintln!("fsc: program exited with code {code}");
                    Ok(ExitCode::from((code & 0xFF) as u8))
                }
                StepOutcome::Continue => Err(CliError::Timeout(max_steps)),
            }
        }
    }
}

fn run_kernel_config(
    engine: &SessionEngine<'_>,
    config: &RunConfig,
    max_steps: u64,
) -> Result<ExitCode, CliError> {
    let boot = engine
        .boot_kernel_config(config)
        .map_err(map_session_error)?;
    eprintln!(
        "fsc: built boot FS: {} entries, {} byte image",
        boot.fs_entries.len(),
        boot.fs_image.len()
    );
    let mut live = LiveMachine::new_kernel(
        &boot.build.artifacts,
        Some(&boot.init),
        Some(&boot.fs_image),
        &config.run.autorun,
    )
    .map_err(CliError::Assemble)?;

    while live.steps < max_steps && matches!(live.state, MachineState::Running) {
        let remaining = max_steps - live.steps;
        live.drive(
            remaining.min(config.run.max_steps_per_frame.max(1)),
            Duration::from_secs(1),
        );
    }

    print!("{}", live.uart_log);
    match live.state {
        MachineState::Exited(code) => {
            eprintln!("fsc: kernel exited with code {code}");
            Ok(ExitCode::from((code & 0xFF) as u8))
        }
        MachineState::Faulted(msg) => Err(CliError::Assemble(format!("kernel faulted: {msg}"))),
        MachineState::Stopped | MachineState::Paused => Ok(ExitCode::SUCCESS),
        MachineState::Running => Err(CliError::Timeout(max_steps)),
    }
}

fn config_for_cli_input(input: &str, mode: TargetMode) -> Result<(NativeFs, RunConfig), CliError> {
    let (fs, path) = cli_workspace_for_input(input)?;
    let config = if has_extension(input, "build") {
        run_config_from_manifest(&fs, path).map_err(CliError::Io)?
    } else {
        let kind = match mode {
            TargetMode::Hosted => RunKind::Hosted,
            TargetMode::Freestanding => RunKind::Freestanding,
            TargetMode::Kernel => RunKind::KernelBoot,
        };
        let mut config = RunConfig::hosted(source_stem(input, "program"), VPath::root());
        config.kind = kind;
        config.build = BuildRef::SourceWithDefaults {
            root: path,
            target: mode,
        };
        config
    };
    Ok((fs, config))
}

fn cli_workspace_for_input(input: &str) -> Result<(NativeFs, VPath), CliError> {
    let cwd = std::env::current_dir()?;
    let path = Path::new(input);
    let (root, relative) = if path.is_absolute() {
        let canonical = path.canonicalize()?;
        let cwd = cwd.canonicalize()?;
        if let Ok(relative) = canonical.strip_prefix(&cwd) {
            (cwd, relative.to_path_buf())
        } else {
            let parent = canonical.parent().ok_or_else(|| {
                CliError::Usage(format!("`{input}` has no parent workspace directory"))
            })?;
            let file_name = canonical.file_name().ok_or_else(|| {
                CliError::Usage(format!("`{input}` does not name a regular input file"))
            })?;
            (parent.to_path_buf(), PathBuf::from(file_name))
        }
    } else {
        (cwd, PathBuf::from(input))
    };
    let text = relative.to_string_lossy().replace('\\', "/");
    let vpath = VPath::parse(&text)
        .map_err(|err| CliError::Usage(format!("invalid workspace path: {err}")))?;
    Ok((NativeFs::open(root)?, vpath))
}

fn map_session_error(err: full_stack::session::SessionEngineError) -> CliError {
    match err {
        full_stack::session::SessionEngineError::Build(full_stack::build::BuildError::Compile(
            err,
        )) => CliError::Compile(err.to_string()),
        full_stack::session::SessionEngineError::Build(err) => CliError::Assemble(err.to_string()),
        full_stack::session::SessionEngineError::Io(err) => CliError::Io(err),
        other => CliError::Assemble(other.to_string()),
    }
}

fn linked_elf_bytes(outcome: &BuildOutcome) -> Vec<u8> {
    outcome
        .artifacts
        .linked
        .to_elf_with_entry(outcome.link_layout.load_base, &outcome.entry)
}

fn print_build_summary(outcome: &BuildOutcome) {
    eprintln!(
        "fsc: built `{}` for {:?} ({} units)",
        outcome.manifest_name,
        outcome.target,
        outcome.artifacts.units.len()
    );
    if let Some(dir) = &outcome.artifact_dir {
        eprintln!("fsc: wrote artifacts under `{}`", dir.display());
    }
}

fn assemble_from_s_file(path: &str, mode: TargetMode) -> Result<AssembledOutput, CliError> {
    let asm_text = fs::read_to_string(path)?;
    let stdlib_objects = compile_stdlib_objects(mode)?;
    let user_tokens = asm_text_to_tokens(&asm_text);
    let mut pipeline = make_pipeline(mode, "_u_");
    pipeline.set_artifact_stem(Some(source_stem(path, "module")));

    let user_obj = pipeline
        .assemble_named(&source_stem(path, "module"), &user_tokens)
        .map_err(|e| CliError::Assemble(format!("user object assembly failed: {e}")))?;

    let mut modules: Vec<(&str, &AssembledOutput)> = stdlib_objects
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));

    pipeline
        .link_assembled_objects_named(&source_stem(path, "module"), &modules)
        .map_err(|e| CliError::Assemble(e.to_string()))
}

// No source concatenation: each HLL file becomes its own object.
fn compile_stdlib_objects(mode: TargetMode) -> Result<Vec<(String, AssembledOutput)>, CliError> {
    let mut pipeline = make_pipeline(mode, "_s_");
    pipeline.set_run_semantic_analysis(false);
    pipeline.set_type_prelude(get_stdlib_type_prelude());
    if mode == TargetMode::Kernel {
        pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
    }

    let modules: Vec<(&str, &str)> = get_stdlib_modules_for_mode(mode);
    let objs = pipeline
        .compile_modules(&modules)
        .map_err(|e| CliError::Compile(format!("stdlib: {e}")))?;

    let named: Vec<(String, AssembledOutput)> = modules
        .into_iter()
        .map(|(n, _)| n.to_owned())
        .zip(objs)
        .collect();

    Ok(named)
}

// Pass-0 recognises labels, section directives, and known mnemonics on indented
// lines; unrecognised mnemonics become no-op comments.
fn asm_text_to_tokens(text: &str) -> Vec<RvInstruction> {
    text.lines()
        .map(|line| RvInstruction::Directive(line.to_owned()))
        .collect()
}

// --- Pipeline factory ---

fn make_pipeline(mode: TargetMode, string_prefix: &str) -> CompilationPipeline {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(mode);
    p.set_string_prefix(Some(string_prefix.to_owned()));
    p
}

fn source_stem(path: &str, default: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(default)
        .to_owned()
}

// --- I/O helpers ---

fn require_single_input(args: &Args) -> Result<&str, CliError> {
    if args.inputs.len() > 1 {
        return Err(CliError::Usage(format!(
            "expected exactly one input file, got {}\n\nRun `fsc help` for usage.",
            args.inputs.len()
        )));
    }
    require_any_input(args)
}

fn require_any_input(args: &Args) -> Result<&str, CliError> {
    args.first_input().ok_or_else(|| {
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
    if let Some(path) = output {
        Ok(fs::write(path, content)?)
    } else {
        println!("{content}");
        Ok(())
    }
}

// --- Help ---

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
             build       <input>       Build an HLL source or .build manifest\n\
             link        <file.hll>...     Compile and link multiple HLL sources into an ELF\n\
             hll-to-ir  <input.hll>   Compile HLL source to IR (printed to stdout)\n\
             hll-to-asm <input.hll>   Compile HLL source to RISC-V assembly\n\
             run        <input>       Compile and run through the VM\n\
             help                     Show this message\n\
         \n\
         OPTIONS\n\
         \n\
             -o, --output <path>          Write output to <path> instead of stdout\n\
         \n\
             -m, --mode   hosted|freestanding|kernel\n\
                                           Target mode (default: hosted)\n\
         \n\
                 --emit-o                 For `hll-to-asm`, emit a relocatable `.o` file\n\
                 --max-steps <n>          VM step limit for `run` (default: 50000000)\n\
         \n\
         EXAMPLES\n\
         \n\
             fsc build      program.hll\n\
             fsc build      program.build -o program.elf\n\
             fsc build      crates/os-runtime/kernel/kernel.build\n\
         \n\
             fsc hll-to-ir  program.hll\n\
             fsc hll-to-ir  program.hll -o program.ir\n\
         \n\
             fsc hll-to-asm program.hll\n\
             fsc hll-to-asm program.hll -o program.s\n\
             fsc hll-to-asm program.hll --emit-o -o program.o\n\
         \n\
             fsc link       main.hll utils.hll -o program.elf\n\
             fsc link       main.hll lib1.hll lib2.hll --mode freestanding -o kernel.elf\n\
         \n\
             fsc run        program.hll\n\
             fsc run        program.hll --max-steps 1000000\n\
             fsc run        program.hll --mode freestanding\n\
             fsc run        crates/os-runtime/kernel/kernel.build --max-steps 50000000\n\
             fsc run        program.s\n\
         \n\
         LINK\n\
         \n\
             `link` compiles each input HLL file independently, assembles them\n\
             into separate object files, links them together alongside the stdlib\n\
             for the target mode, and writes a standalone ELF executable.\n\
         \n\
         NOTE\n\
         \n\
             `run program.s` parses the assembly text with a built-in subset parser.\n\
             Some instructions may be silently skipped.  Prefer `run program.hll`\n\
             for reliable results.\n\
         "
    );
}
