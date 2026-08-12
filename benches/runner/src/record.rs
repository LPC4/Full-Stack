//! JSONL record construction. Derived metrics live in tools/analyse.py only.

use crate::harness::Sample;
use crate::suite::{BenchDef, Knobs};
use serde_json::{Value, json};

pub const SCHEMA_VERSION: u32 = 2;

pub struct RecordContext<'a> {
    pub suite: &'a str,
    pub bench: &'a BenchDef,
    pub variant: &'a str,
    pub knobs: Knobs,
    pub memory_latency: &'a str,
    pub input_size: u64,
}

pub fn to_json_line(
    ctx: &RecordContext<'_>,
    sample_idx: u64,
    s: &Sample,
    checksum_ok: bool,
    compile_time_ms: f64,
) -> Value {
    let cache_level = |c: &crate::harness::CacheLevelStats| {
        json!({
            "read_hits": c.read_hits,
            "read_misses": c.read_misses,
            "write_hits": c.write_hits,
            "write_misses": c.write_misses,
        })
    };
    json!({
        "schema_version": SCHEMA_VERSION,
        "suite": ctx.suite,
        "benchmark": ctx.bench.name,
        "group": ctx.bench.group,
        "variant": ctx.variant,
        "config": {
            "ir_opt": ctx.knobs.ir_opt,
            "peephole": ctx.knobs.peephole,
            "regalloc": ctx.knobs.regalloc,
            "omit_frame_pointer": ctx.knobs.omit_frame_pointer,
            "memory_latency": ctx.memory_latency,
            "input_size": ctx.input_size,
        },
        "sample": sample_idx,
        "exit_code": s.exit_code,
        "checksum_ok": checksum_ok,
        "checksum": s.uart,
        "steps": s.steps,
        "cycles": s.cycles,
        "insns_retired": s.insns_retired,
        "stall_cycles": s.stall_cycles,
        "memory_stall_cycles": s.memory_stall_cycles,
        "flush_cycles": s.flush_cycles,
        "branches_seen": s.branches_seen,
        "branches_mispredicted": s.branches_mispredicted,
        "cache": {
            "l1": cache_level(&s.cache[0]),
            "l2": cache_level(&s.cache[1]),
            "l3": cache_level(&s.cache[2]),
        },
        "compile_time_ms": compile_time_ms,
        "wall_time_ms": s.wall_time_ms,
    })
}
