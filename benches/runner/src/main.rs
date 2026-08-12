//! Workspace benchmark runner with run, list, and exhibits commands.

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "benchmark CLI reports progress and results directly"
)]

mod harness;
mod record;
mod suite;

use std::io::Write as _;
use std::process::ExitCode;
use virtual_machine::bus::MemoryLatencyConfig;

struct RunArgs {
    benchmark: Option<String>,
    samples: u64,
    warmups: u64,
    input_size: u64,
    memory_latency: String,
    output: String,
}

fn usage() -> String {
    "benchmark-runner subcommands:\n\
     run --suite teaching [--benchmark NAME] [--samples N] [--warmups N]\n\
    \x20    [--input-size N] [--memory-latency off|realistic] --output FILE.jsonl\n\
     list\n\
     exhibits --output DIR\n"
        .to_owned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => parse_run(&args[1..]).and_then(|a| cmd_run(&a)),
        Some("list") => {
            cmd_list();
            Ok(())
        }
        Some("exhibits") => parse_output_dir(&args[1..]).and_then(|d| cmd_exhibits(&d)),
        _ => Err(usage()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

// --- Argument parsing (hand-rolled, no deps) ---

fn parse_run(args: &[String]) -> Result<RunArgs, String> {
    let mut out = RunArgs {
        benchmark: None,
        samples: 1,
        warmups: 0,
        input_size: 1,
        memory_latency: "off".to_owned(),
        output: String::new(),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--suite" => {
                let v = value("--suite")?;
                if v != suite::SUITE_NAME {
                    return Err(format!("unknown suite `{v}` (only `teaching`)"));
                }
            }
            "--benchmark" => out.benchmark = Some(value("--benchmark")?),
            "--samples" => out.samples = parse_count(&value("--samples")?)?,
            "--warmups" => {
                out.warmups = value("--warmups")?
                    .parse()
                    .map_err(|_error| "bad --warmups")?;
            }
            "--input-size" => out.input_size = parse_count(&value("--input-size")?)?,
            "--memory-latency" => {
                let v = value("--memory-latency")?;
                if v != "off" && v != "realistic" {
                    return Err(format!("--memory-latency must be off|realistic, got `{v}`"));
                }
                out.memory_latency = v;
            }
            "--output" => out.output = value("--output")?,
            other => return Err(format!("unknown flag `{other}`\n\n{}", usage())),
        }
    }
    if out.output.is_empty() {
        return Err("run requires --output FILE.jsonl".to_owned());
    }
    Ok(out)
}

fn parse_count(v: &str) -> Result<u64, String> {
    match v.parse::<u64>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!("`{v}` is not a positive integer")),
    }
}

fn parse_output_dir(args: &[String]) -> Result<String, String> {
    match args {
        [flag, dir] if flag == "--output" => Ok(dir.clone()),
        _ => Err("exhibits requires --output DIR".to_owned()),
    }
}

// --- list ---

fn cmd_list() {
    for b in suite::BENCHES {
        let variants: Vec<&str> = b.variants.iter().map(|v| v.name).collect();
        println!(
            "{:<22} group={:<9} base_n={:<8} variants=[{}]",
            b.name,
            b.group,
            b.base_n,
            variants.join(", ")
        );
    }
}

// --- run ---

fn latency_config(name: &str) -> MemoryLatencyConfig {
    if name == "realistic" {
        MemoryLatencyConfig::realistic()
    } else {
        MemoryLatencyConfig::disabled()
    }
}

fn cmd_run(args: &RunArgs) -> Result<(), String> {
    let benches: Vec<&suite::BenchDef> = match &args.benchmark {
        Some(name) => vec![suite::find(name).ok_or_else(|| format!("unknown benchmark `{name}`"))?],
        None => suite::BENCHES.iter().collect(),
    };

    if let Some(parent) = std::path::Path::new(&args.output).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create output dir: {e}"))?;
    }
    let file = std::fs::File::create(&args.output).map_err(|e| format!("open output: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    let latency = latency_config(&args.memory_latency);
    let mut any_failed = false;

    for bench in benches {
        let n = bench.base_n * args.input_size;
        let src_path = suite::source_path(bench.file);
        let raw = std::fs::read_to_string(&src_path)
            .map_err(|e| format!("read {}: {e}", src_path.display()))?;
        let src = suite::substitute_n(&raw, n);

        // Run every variant, then cross-check checksums before writing records.
        let mut variant_runs = Vec::new();
        for variant in bench.variants {
            eprintln!("[bench] {} / {} (n={n})", bench.name, variant.name);
            let compiled = harness::compile_with_knobs(&src, variant.knobs)
                .map_err(|e| format!("{}/{}: {e}", bench.name, variant.name))?;
            for _ in 0..args.warmups {
                harness::run_sample(&compiled.assembled, latency, bench.max_steps);
            }
            let samples: Vec<harness::Sample> = (0..args.samples)
                .map(|_| harness::run_sample(&compiled.assembled, latency, bench.max_steps))
                .collect();
            variant_runs.push((variant, compiled.compile_time_ms, samples));
        }

        // Primary correctness check: all variants agree on checksum and exit 0.
        let reference = variant_runs[0].2[0].uart.clone();
        let bench_ok = variant_runs.iter().all(|(_, _, samples)| {
            samples
                .iter()
                .all(|s| s.halted && s.exit_code == 0 && s.uart == reference)
        });
        if !bench_ok {
            any_failed = true;
            eprintln!(
                "[FAIL] {}: checksum/exit mismatch across variants",
                bench.name
            );
            for (variant, _, samples) in &variant_runs {
                for s in samples {
                    eprintln!(
                        "       {}: halted={} exit={} checksum={:?}",
                        variant.name, s.halted, s.exit_code, s.uart
                    );
                }
            }
        }

        for (variant, compile_time_ms, samples) in &variant_runs {
            let ctx = record::RecordContext {
                suite: suite::SUITE_NAME,
                bench,
                variant: variant.name,
                knobs: variant.knobs,
                memory_latency: &args.memory_latency,
                input_size: args.input_size,
            };
            for (idx, s) in samples.iter().enumerate() {
                let line = record::to_json_line(&ctx, idx as u64, s, bench_ok, *compile_time_ms);
                writeln!(writer, "{line}").map_err(|e| format!("write output: {e}"))?;
            }
        }
    }

    writer.flush().map_err(|e| format!("flush output: {e}"))?;
    if any_failed {
        Err("one or more benchmarks failed the checksum/exit check".to_owned())
    } else {
        eprintln!("[done] results in {}", args.output);
        Ok(())
    }
}

// --- exhibits ---

// Keep A/B compiler exhibits beside the measured variant data.
fn cmd_exhibits(dir: &str) -> Result<(), String> {
    use full_stack::compilation_pipeline::CompilationPipeline;
    use hll_to_ir::OptOptions;

    std::fs::create_dir_all(dir).map_err(|e| format!("create {dir}: {e}"))?;
    for bench in suite::BENCHES.iter().filter(|b| b.variants.len() > 1) {
        let raw = std::fs::read_to_string(suite::source_path(bench.file))
            .map_err(|e| format!("read {}: {e}", bench.file))?;
        // Small fixed N keeps exhibit dumps readable; codegen shape is N-independent.
        let src = suite::substitute_n(&raw, 16);
        for variant in bench.variants {
            let mut pipeline = CompilationPipeline::new();
            pipeline.set_write_artifacts(false);
            pipeline.set_peephole(variant.knobs.peephole);
            pipeline.set_register_allocation(variant.knobs.regalloc);
            pipeline.set_omit_frame_pointer(variant.knobs.omit_frame_pointer);
            if variant.knobs.ir_opt {
                pipeline.set_optimize(OptOptions::all());
            }
            let result = pipeline
                .compile(&src)
                .map_err(|e| format!("{}/{}: compile failed: {e:?}", bench.name, variant.name))?;
            let ir_text = result.ir_program.to_string();
            let asm_text = pipeline
                .compile_ir_to_assembly(&result.ir_program)
                .map_err(|e| format!("{}/{}: codegen failed: {e}", bench.name, variant.name))?;
            let stem = format!("{}/{}.{}", dir, bench.name, variant.name);
            std::fs::write(format!("{stem}.ir"), &ir_text)
                .map_err(|e| format!("write {stem}.ir: {e}"))?;
            std::fs::write(format!("{stem}.s"), &asm_text)
                .map_err(|e| format!("write {stem}.s: {e}"))?;
            eprintln!("[exhibit] {stem}.ir / .s");
        }
    }
    Ok(())
}
