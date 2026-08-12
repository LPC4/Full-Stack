#!/usr/bin/env python3
"""Render summary.csv into a markdown report with per-group tables."""

import argparse
import csv
import sys
from pathlib import Path

GROUPS = ["opt", "pipeline", "memory", "kernel"]

COLUMNS = [
    ("variant", "variant", "{}"),
    ("memory_latency", "latency", "{}"),
    ("input_size", "size", "{:.0f}"),
    ("cycles", "cycles", "{:,.0f}"),
    ("insns_retired", "insns", "{:,.0f}"),
    ("cpi", "CPI", "{:.3f}"),
    ("stall_frac", "stall%", "{:.1%}"),
    ("flush_frac", "flush%", "{:.1%}"),
    ("mispredict_rate", "mispredict", "{:.1%}"),
    ("l1_read_hit_rate", "L1 read hit", "{:.1%}"),
    ("speedup_vs_baseline", "speedup", "{:.2f}x"),
    ("latency_cycle_ratio", "lat ratio", "{:.2f}x"),
]


def fmt_cell(row, key, fmt):
    val = row.get(key)
    if val in (None, ""):
        return "-"
    if fmt == "{}":
        return str(val)
    return fmt.format(float(val))


def render(rows, exhibits_dir):
    lines = ["# Teaching benchmark report", ""]
    bad = [r for r in rows if r.get("checksum_ok") != "True"]
    status = "all checksums OK" if not bad else f"{len(bad)} row(s) FAILED checksum"
    lines += [f"Rows: {len(rows)}. Status: **{status}**.", ""]

    for group in GROUPS:
        group_rows = [r for r in rows if r.get("group") == group]
        if not group_rows:
            continue
        lines += [f"## {group}", ""]
        header = ["benchmark"] + [label for _, label, _ in COLUMNS]
        lines.append("| " + " | ".join(header) + " |")
        lines.append("|" + "---|" * len(header))
        for row in group_rows:
            cells = [row["benchmark"]] + [
                fmt_cell(row, key, fmt) for key, _, fmt in COLUMNS
            ]
            lines.append("| " + " | ".join(cells) + " |")
        lines.append("")

    if exhibits_dir and exhibits_dir.is_dir():
        dumps = sorted(p.name for p in exhibits_dir.iterdir() if p.suffix in (".ir", ".s"))
        if dumps:
            lines += ["## Exhibits", ""]
            lines += [f"- `{name}`" for name in dumps]
            lines.append("")

    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("summary", help="path to summary.csv")
    parser.add_argument("--exhibits", default=None, help="exhibits directory")
    parser.add_argument("--output", default=None, help="output .md (default: report.md next to summary)")
    args = parser.parse_args(argv)

    summary = Path(args.summary)
    with open(summary, encoding="utf-8") as fh:
        rows = list(csv.DictReader(fh))
    exhibits = Path(args.exhibits) if args.exhibits else None
    out_path = Path(args.output) if args.output else summary.parent / "report.md"
    out_path.write_text(render(rows, exhibits), encoding="utf-8")
    print(f"wrote {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
