//! Compile + run harness, mirroring `tests/integration/vm_execution.rs`.

use crate::suite::Knobs;
use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::{OptOptions, TargetMode};
use std::sync::OnceLock;
use std::time::Instant;
use virtual_machine::bus::MemoryLatencyConfig;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// Stdlib stays constant so measured deltas come only from the benchmark module.
fn cached_stdlib_objs() -> &'static [(String, AssembledOutput)] {
    static STDLIB: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    STDLIB.get_or_init(|| {
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
            .expect("stdlib compile failed")
    })
}

pub struct Compiled {
    pub assembled: AssembledOutput,
    pub compile_time_ms: f64,
}

/// Compile one bench source with the variant's knobs and link against the
/// cached default stdlib.
pub fn compile_with_knobs(src: &str, knobs: Knobs) -> Result<Compiled, String> {
    let start = Instant::now();
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_peephole(knobs.peephole);
    pipeline.set_register_allocation(knobs.regalloc);
    pipeline.set_omit_frame_pointer(knobs.omit_frame_pointer);
    if knobs.ir_opt {
        pipeline.set_optimize(OptOptions::all());
    }

    let user_result = pipeline
        .compile(src)
        .map_err(|e| format!("compile failed: {e:?}"))?;
    let (_, user_tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&user_result.ir_program)
        .map_err(|e| format!("codegen failed: {e}"))?;
    let user_obj = pipeline
        .assemble(&user_tokens)
        .map_err(|e| format!("assemble failed: {e:?}"))?;

    let mut modules: Vec<(&str, &AssembledOutput)> = cached_stdlib_objs()
        .iter()
        .map(|(n, o)| (n.as_str(), o))
        .collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .map_err(|e| format!("link failed: {e:?}"))?;

    Ok(Compiled {
        assembled,
        compile_time_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

pub struct CacheLevelStats {
    pub read_hits: u64,
    pub read_misses: u64,
    pub write_hits: u64,
    pub write_misses: u64,
}

pub struct Sample {
    pub exit_code: i64,
    pub halted: bool,
    pub uart: String,
    pub steps: u64,
    pub cycles: u64,
    pub insns_retired: u64,
    pub stall_cycles: u64,
    pub memory_stall_cycles: u64,
    pub flush_cycles: u64,
    pub branches_seen: u64,
    pub branches_mispredicted: u64,
    pub cache: [CacheLevelStats; 3],
    pub wall_time_ms: f64,
}

/// Run one fresh VM over the linked image and extract every counter we record.
pub fn run_sample(
    assembled: &AssembledOutput,
    latency: MemoryLatencyConfig,
    max_steps: u64,
) -> Sample {
    let start = Instant::now();
    let mut vm = VirtualMachine::new(assembled);
    vm.set_memory_latency_config(latency);
    let run = vm.run(max_steps);
    let wall_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    let (halted, exit_code) = match run.outcome {
        StepOutcome::Halted(code) => (true, code),
        StepOutcome::Continue => (false, -1),
    };
    let stats = vm.pipeline_stats();
    let (l1, l2, l3) = vm.get_cache_stats();
    let level = |s: &virtual_machine::memory::cache::CacheStats| CacheLevelStats {
        read_hits: s.read_hits,
        read_misses: s.read_misses,
        write_hits: s.write_hits,
        write_misses: s.write_misses,
    };

    Sample {
        exit_code,
        halted,
        uart: run.uart_output.trim().to_owned(),
        steps: run.steps,
        cycles: stats.cycles,
        insns_retired: stats.insns_retired,
        stall_cycles: stats.stall_cycles,
        memory_stall_cycles: stats.memory_stall_cycles,
        flush_cycles: stats.flush_cycles,
        branches_seen: stats.branches_seen,
        branches_mispredicted: stats.branches_mispredicted,
        cache: [level(&l1), level(&l2), level(&l3)],
        wall_time_ms,
    }
}
