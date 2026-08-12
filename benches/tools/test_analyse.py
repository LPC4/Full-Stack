#!/usr/bin/env python3
"""Unit tests for the pure formula functions in analyse.py."""

import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import analyse


def sample(**overrides):
    rec = {
        "benchmark": "bench",
        "group": "opt",
        "variant": "default",
        "config": {"memory_latency": "off", "input_size": 1},
        "checksum_ok": True,
        "exit_code": 0,
        "steps": 100,
        "cycles": 150,
        "insns_retired": 100,
        "stall_cycles": 30,
        "memory_stall_cycles": 10,
        "flush_cycles": 5,
        "branches_seen": 40,
        "branches_mispredicted": 4,
        "compile_time_ms": 10.0,
        "wall_time_ms": 20.0,
        "cache": {
            level: {"read_hits": 90, "read_misses": 10, "write_hits": 8, "write_misses": 2}
            for level in ("l1", "l2", "l3")
        },
    }
    for key, val in overrides.items():
        if key in ("variant", "benchmark"):
            rec[key] = val
        elif key in ("memory_latency", "input_size"):
            rec["config"][key] = val
        else:
            rec[key] = val
    return rec


class Formulas(unittest.TestCase):
    def test_safe_div_zero_denominator(self):
        self.assertIsNone(analyse.safe_div(5, 0))

    def test_cpi(self):
        self.assertAlmostEqual(analyse.cpi(150, 100), 1.5)

    def test_hit_rate(self):
        self.assertAlmostEqual(analyse.hit_rate(90, 10), 0.9)

    def test_hit_rate_no_accesses(self):
        self.assertIsNone(analyse.hit_rate(0, 0))

    def test_speedup_improvement_is_above_one(self):
        self.assertAlmostEqual(analyse.speedup(200, 100), 2.0)

    def test_baseline_prefers_default(self):
        self.assertEqual(
            analyse.baseline_variant({"opt_on", "default", "opt_off"}), "default"
        )

    def test_baseline_ab_pair(self):
        self.assertEqual(analyse.baseline_variant({"opt_on", "opt_off"}), "opt_off")
        self.assertEqual(
            analyse.baseline_variant({"regalloc_on", "regalloc_off"}), "regalloc_off"
        )
        self.assertEqual(analyse.baseline_variant({"fp_kept", "fp_omitted"}), "fp_kept")


class Aggregation(unittest.TestCase):
    def test_medians_over_samples(self):
        records = [sample(cycles=100), sample(cycles=300), sample(cycles=200)]
        rows = analyse.aggregate(records)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["cycles"], 200)
        self.assertEqual(rows[0]["samples"], 3)

    def test_derived_metrics(self):
        rows = analyse.aggregate([sample()])
        row = rows[0]
        self.assertAlmostEqual(row["cpi"], 1.5)
        self.assertAlmostEqual(row["stall_frac"], 30 / 150)
        self.assertAlmostEqual(row["mispredict_rate"], 0.1)
        self.assertAlmostEqual(row["l1_read_hit_rate"], 0.9)

    def test_speedup_vs_baseline(self):
        records = [
            sample(variant="opt_off", cycles=300),
            sample(variant="opt_on", cycles=100),
        ]
        by_variant = {r["variant"]: r for r in analyse.aggregate(records)}
        self.assertAlmostEqual(by_variant["opt_off"]["speedup_vs_baseline"], 1.0)
        self.assertAlmostEqual(by_variant["opt_on"]["speedup_vs_baseline"], 3.0)

    def test_latency_ratio_on_realistic_row(self):
        records = [
            sample(memory_latency="off", cycles=100),
            sample(memory_latency="realistic", cycles=250),
        ]
        rows = analyse.aggregate(records)
        by_latency = {r["memory_latency"]: r for r in rows}
        self.assertIsNone(by_latency["off"]["latency_cycle_ratio"])
        self.assertAlmostEqual(by_latency["realistic"]["latency_cycle_ratio"], 2.5)

    def test_checksum_failure_propagates(self):
        records = [sample(), sample(checksum_ok=False)]
        rows = analyse.aggregate(records)
        self.assertFalse(rows[0]["checksum_ok"])


if __name__ == "__main__":
    unittest.main()
