#!/usr/bin/env python3
"""Aggregate bench-runner JSONL into summary.csv with derived metrics.

Raw counters come from the runner; every derived metric (CPI, rates, speedups,
latency ratios) is computed here and only here. Formula functions are pure so
test_analyse.py can exercise them directly.
"""

import argparse
import csv
import glob
import json
import statistics
import sys
from pathlib import Path

# Baseline preference: "default" wins, otherwise the canonical A/B baseline.
BASELINE_ORDER = ["default", "opt_off", "regalloc_off", "peephole_off", "fp_kept"]

RAW_FIELDS = [
    "steps",
    "cycles",
    "insns_retired",
    "stall_cycles",
    "memory_stall_cycles",
    "flush_cycles",
    "branches_seen",
    "branches_mispredicted",
    "compile_time_ms",
    "wall_time_ms",
]
CACHE_FIELDS = ["read_hits", "read_misses", "write_hits", "write_misses"]


def safe_div(num, den):
    """num/den, or None when the denominator is zero (blank cell in the CSV)."""
    if not den:
        return None
    return num / den


def cpi(cycles, insns):
    return safe_div(cycles, insns)


def rate(part, whole):
    return safe_div(part, whole)


def hit_rate(hits, misses):
    return safe_div(hits, hits + misses)


def speedup(baseline_cycles, variant_cycles):
    """How many times faster than the baseline; >1 means an improvement."""
    return safe_div(baseline_cycles, variant_cycles)


def baseline_variant(variants):
    for name in BASELINE_ORDER:
        if name in variants:
            return name
    return sorted(variants)[0]


def median_of(samples, field):
    return statistics.median(s[field] for s in samples)


def load_records(paths):
    records = []
    for path in paths:
        with open(path, encoding="utf-8") as fh:
            for line_no, line in enumerate(fh, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    records.append(json.loads(line))
                except json.JSONDecodeError as err:
                    sys.exit(f"{path}:{line_no}: bad JSON: {err}")
    return records


def group_key(rec):
    cfg = rec["config"]
    return (rec["benchmark"], rec["variant"], cfg["memory_latency"], cfg["input_size"])


def aggregate(records):
    """One summary row per (benchmark, variant, latency, input_size): medians
    of the raw counters plus per-group derived metrics."""
    groups = {}
    for rec in records:
        groups.setdefault(group_key(rec), []).append(rec)

    rows = {}
    for key, samples in sorted(groups.items()):
        bench, variant, latency, input_size = key
        row = {
            "benchmark": bench,
            "group": samples[0].get("group", ""),
            "variant": variant,
            "memory_latency": latency,
            "input_size": input_size,
            "samples": len(samples),
            "checksum_ok": all(s["checksum_ok"] for s in samples),
            "exit_code": max(s["exit_code"] for s in samples),
        }
        for field in RAW_FIELDS:
            row[field] = median_of(samples, field)
        for level in ("l1", "l2", "l3"):
            for field in CACHE_FIELDS:
                row[f"{level}_{field}"] = statistics.median(
                    s["cache"][level][field] for s in samples
                )

        row["cpi"] = cpi(row["cycles"], row["insns_retired"])
        row["stall_frac"] = rate(row["stall_cycles"], row["cycles"])
        row["mem_stall_frac"] = rate(row["memory_stall_cycles"], row["cycles"])
        row["flush_frac"] = rate(row["flush_cycles"], row["cycles"])
        row["mispredict_rate"] = rate(
            row["branches_mispredicted"], row["branches_seen"]
        )
        for level in ("l1", "l2", "l3"):
            row[f"{level}_read_hit_rate"] = hit_rate(
                row[f"{level}_read_hits"], row[f"{level}_read_misses"]
            )
            row[f"{level}_write_hit_rate"] = hit_rate(
                row[f"{level}_write_hits"], row[f"{level}_write_misses"]
            )
        rows[key] = row

    attach_speedups(rows)
    attach_latency_ratios(rows)
    return [rows[key] for key in sorted(rows)]


def attach_speedups(rows):
    """Speedup vs the bench's baseline variant, within one latency/size cell."""
    by_cell = {}
    for (bench, variant, latency, size), row in rows.items():
        by_cell.setdefault((bench, latency, size), {})[variant] = row
    for variants in by_cell.values():
        base = variants[baseline_variant(variants)]
        for row in variants.values():
            row["speedup_vs_baseline"] = speedup(base["cycles"], row["cycles"])


def attach_latency_ratios(rows):
    """realistic/off cycle ratio, stored on the realistic row of each pair."""
    for (bench, variant, latency, size), row in rows.items():
        row["latency_cycle_ratio"] = None
        if latency == "realistic":
            off = rows.get((bench, variant, "off", size))
            if off is not None:
                row["latency_cycle_ratio"] = safe_div(row["cycles"], off["cycles"])


def write_summary(rows, out_dir):
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / "summary.csv"
    if not rows:
        sys.exit("no records to summarize")
    fieldnames = list(rows[0].keys())
    with open(out_path, "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {
                    k: (f"{v:.6g}" if isinstance(v, float) else v)
                    for k, v in row.items()
                }
            )
    return out_path


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", help="JSONL files or globs")
    parser.add_argument("--output", required=True, help="output directory")
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="exit 1 if any sample has checksum_ok=false",
    )
    args = parser.parse_args(argv)

    paths = []
    for pattern in args.inputs:
        matched = sorted(glob.glob(pattern))
        paths.extend(matched if matched else [pattern])
    records = load_records(paths)
    if not records:
        sys.exit("no records found")

    rows = aggregate(records)
    out_path = write_summary(rows, Path(args.output))
    print(f"wrote {out_path} ({len(rows)} rows from {len(records)} samples)")

    bad = [r for r in rows if not r["checksum_ok"]]
    if bad:
        for r in bad:
            print(
                f"CHECKSUM FAIL: {r['benchmark']}/{r['variant']} "
                f"latency={r['memory_latency']} size={r['input_size']}",
                file=sys.stderr,
            )
        if args.fail_on_regression:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
