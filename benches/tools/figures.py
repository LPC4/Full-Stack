#!/usr/bin/env python3
"""Render teaching figures from summary.csv (matplotlib, Agg backend).

Each figure is saved as both .pdf (for LaTeX) and .png (for quick viewing).
Colors follow the validated reference palette: categorical hues are assigned
by entity in fixed slot order, sequential ramps encode ordered magnitude,
and text/grid chrome stays in ink tokens, never series colors.
"""

import argparse
import csv
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FormatStrFormatter

# Reference palette (light mode): categorical slots + sequential blue ramp.
SLOT = {
    "blue": "#2a78d6",
    "aqua": "#1baf7a",
    "yellow": "#eda100",
    "green": "#008300",
    "violet": "#4a3aa7",
    "red": "#e34948",
}
SEQ_BLUE = ["#86b6ef", "#2a78d6", "#104281"]  # ordinal ramp: L1, L2, L3
SURFACE = "#fcfcfb"
INK = "#0b0b0b"
INK_2 = "#52514e"
MUTED = "#898781"
GRID = "#e1e0d9"
BASELINE = "#c3c2b7"

# Stall taxonomy used by the CPI breakdown. stall_cycles includes memory
# stalls in the runner's accounting, so hazard = stall - memory.
CPI_SEGMENTS = [
    ("hazard stall", SLOT["yellow"]),
    ("memory stall", SLOT["violet"]),
    ("branch flush", SLOT["red"]),
]


def load_summary(path):
    with open(path, encoding="utf-8") as fh:
        rows = list(csv.DictReader(fh))
    for row in rows:
        for key, val in row.items():
            if key in ("benchmark", "group", "variant", "memory_latency"):
                continue
            if val == "" or val is None:
                row[key] = None
            elif key == "checksum_ok":
                row[key] = val == "True"
            else:
                try:
                    row[key] = float(val)
                except ValueError:
                    pass
    return rows


def style_axes(ax):
    ax.set_facecolor(SURFACE)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(BASELINE)
    ax.tick_params(colors=MUTED, labelsize=9)
    ax.yaxis.grid(True, color=GRID, linewidth=0.6)
    ax.set_axisbelow(True)


def new_fig(width=5.6, height=3.3):
    fig, ax = plt.subplots(figsize=(width, height), facecolor=SURFACE)
    style_axes(ax)
    return fig, ax


def set_title(ax, text):
    ax.set_title(text, color=INK, fontsize=11, loc="left", pad=12)


def save(fig, out_dir, name):
    for ext in ("pdf", "png"):
        fig.savefig(
            out_dir / f"{name}.{ext}",
            dpi=200,
            bbox_inches="tight",
            facecolor=SURFACE,
        )
    plt.close(fig)
    print(f"figure: {name}")


def pick(rows, **want):
    out = []
    for row in rows:
        if all(row.get(k) == v for k, v in want.items()):
            out.append(row)
    return out


def annotate(ax, x, y, text, dy=3):
    ax.annotate(
        text,
        (x, y),
        textcoords="offset points",
        xytext=(0, dy),
        ha="center",
        fontsize=9,
        color=INK_2,
    )


def bar_with_labels(ax, labels, values, color, fmt="{:.2f}"):
    bars = ax.bar(labels, values, color=color, width=0.55, edgecolor=SURFACE)
    for rect, val in zip(bars, values):
        annotate(ax, rect.get_x() + rect.get_width() / 2, rect.get_height(), fmt.format(val))
    return bars


def legend_below(fig, ncols):
    fig.legend(
        frameon=False,
        fontsize=8,
        labelcolor=INK_2,
        ncols=ncols,
        loc="lower center",
        bbox_to_anchor=(0.5, -0.12),
    )


# --- Figure 1: optimization speedups ---


def fig_opt_speedup(rows, out_dir):
    """Improved-variant speedup vs its baseline, sorted largest first."""
    pairs = [
        ("const_fold_dce", "opt_on", "const fold\n+ DCE"),
        ("regalloc_pressure", "regalloc_on", "register\nallocation"),
        ("peephole_addressing", "peephole_on", "peephole"),
        ("frame_pointer_calls", "fp_omitted", "omit frame\npointer"),
    ]
    entries = []
    for bench, variant, label in pairs:
        found = pick(
            rows, benchmark=bench, variant=variant, memory_latency="off", input_size=1.0
        )
        if found and found[0]["speedup_vs_baseline"] is not None:
            entries.append((label, found[0]["speedup_vs_baseline"]))
    if not entries:
        return False
    entries.sort(key=lambda e: e[1], reverse=True)
    labels = [e[0] for e in entries]
    values = [e[1] for e in entries]
    fig, ax = new_fig()
    bar_with_labels(ax, labels, values, SLOT["blue"], fmt="{:.2f}x")
    ax.axhline(1.0, color=BASELINE, linewidth=1)
    ax.set_ylabel("speedup in cycles (pass off / pass on)", color=INK_2, fontsize=9)
    set_title(ax, "Effect of each optimization pass, measured in isolation")
    save(fig, out_dir, "opt_speedup")
    return True


# --- Figure 2: CPI breakdown ---


def fig_cpi_breakdown(rows, out_dir):
    """Stacked CPI overhead above the 1.0 in-order floor, flat vs realistic.

    Two panels (small multiples) because the overheads differ by ~20x:
    forcing one scale would flatten the flat-latency panel to nothing.
    """
    benches = [
        ("load_use_chain", "load-use\nchain"),
        ("load_use_spaced", "load-use\nspaced"),
        ("branch_predictable", "branch\npredictable"),
        ("branch_random", "branch\nrandom"),
    ]
    panels = [("off", "flat latency"), ("realistic", "realistic latency")]
    data = {}  # (latency, bench) -> (segments, total_cpi)
    for latency, _ in panels:
        for bench, _ in benches:
            found = pick(
                rows,
                benchmark=bench,
                variant="default",
                memory_latency=latency,
                input_size=1.0,
            )
            if not found:
                continue
            row = found[0]
            cyc = row["cycles"]
            if not cyc:
                continue
            per_insn = row["cpi"] / cyc
            mem = row["memory_stall_cycles"]
            hazard = max(row["stall_cycles"] - mem, 0.0)
            flush = row["flush_cycles"]
            data[(latency, bench)] = (
                [part * per_insn for part in (hazard, mem, flush)],
                row["cpi"],
            )
    if not data:
        return False
    fig, axes = plt.subplots(
        1, 2, figsize=(7.0, 3.4), facecolor=SURFACE, gridspec_kw={"wspace": 0.32}
    )
    for ax, (latency, panel_title) in zip(axes, panels):
        style_axes(ax)
        labels = [label for bench, label in benches if (latency, bench) in data]
        stacks = [data[(latency, bench)][0] for bench, _ in benches if (latency, bench) in data]
        totals = [data[(latency, bench)][1] for bench, _ in benches if (latency, bench) in data]
        bottoms = [0.0] * len(labels)
        for idx, (segment, color) in enumerate(CPI_SEGMENTS):
            heights = [stack[idx] for stack in stacks]
            ax.bar(
                labels,
                heights,
                bottom=bottoms,
                color=color,
                width=0.6,
                label=segment if ax is axes[1] else None,
                edgecolor=SURFACE,
                linewidth=1.5,
            )
            bottoms = [b + h for b, h in zip(bottoms, heights)]
        for x, (top, total) in enumerate(zip(bottoms, totals)):
            annotate(ax, x, top, f"CPI {total:.2f}")
        ax.set_ylim(0, max(bottoms) * 1.25)
        ax.tick_params(axis="x", labelsize=7.5)
        ax.set_title(panel_title, color=INK_2, fontsize=9, pad=6)
    axes[0].set_ylabel("stall + flush cycles per instruction", color=INK_2, fontsize=9)
    fig.suptitle(
        "Where the extra cycles go (CPI above the in-order floor)",
        color=INK,
        fontsize=11,
        x=0.02,
        y=1.02,
        ha="left",
    )
    legend_below(fig, ncols=3)
    save(fig, out_dir, "cpi_breakdown")
    return True


# --- Figure 2b: the optimization matrix ---


def fig_opt_matrix(rows, out_dir):
    """Heatmap: each pass (rows) x what it changed (columns), as percent
    improvement. Diverging color: blue = better, red = worse, gray = zero."""
    from matplotlib.colors import LinearSegmentedColormap, TwoSlopeNorm

    pairs = [
        ("regalloc_pressure", "regalloc_off", "regalloc_on", "register allocation"),
        ("const_fold_dce", "opt_off", "opt_on", "const fold + DCE"),
        ("peephole_addressing", "peephole_off", "peephole_on", "peephole"),
        ("frame_pointer_calls", "fp_kept", "fp_omitted", "omit frame pointer"),
    ]
    metrics = [
        ("cycles", "cycles\nsaved"),
        ("insns_retired", "instructions\nremoved"),
        ("cpi", "CPI\nimprovement"),
    ]
    grid, row_labels = [], []
    for bench, off_v, on_v, label in pairs:
        off = pick(rows, benchmark=bench, variant=off_v, memory_latency="off", input_size=1.0)
        on = pick(rows, benchmark=bench, variant=on_v, memory_latency="off", input_size=1.0)
        if not off or not on:
            continue
        cells = []
        for key, _ in metrics:
            a, b = off[0][key], on[0][key]
            cells.append((1.0 - b / a) * 100.0 if a else 0.0)
        grid.append(cells)
        row_labels.append(label)
    if not grid:
        return False
    cmap = LinearSegmentedColormap.from_list(
        "diverge", [SLOT["red"], "#f0efec", SLOT["blue"]]
    )
    span = max(abs(v) for row in grid for v in row)
    norm = TwoSlopeNorm(vmin=-span, vcenter=0.0, vmax=span)
    fig, ax = plt.subplots(figsize=(5.6, 2.9), facecolor=SURFACE)
    ax.imshow(grid, cmap=cmap, norm=norm, aspect="auto")
    for i, row in enumerate(grid):
        for j, val in enumerate(row):
            strong = abs(val) > span * 0.55
            ax.text(
                j,
                i,
                f"{val:+.1f}%",
                ha="center",
                va="center",
                fontsize=10,
                color=SURFACE if strong else INK,
                fontweight="bold" if strong else "normal",
            )
    ax.set_xticks(range(len(metrics)), [label for _, label in metrics], fontsize=9)
    ax.set_yticks(range(len(row_labels)), row_labels, fontsize=9)
    ax.tick_params(colors=INK_2, length=0)
    for spine in ax.spines.values():
        spine.set_visible(False)
    ax.set_xticks([x - 0.5 for x in range(1, len(metrics))], minor=True)
    ax.set_yticks([y - 0.5 for y in range(1, len(row_labels))], minor=True)
    ax.grid(which="minor", color=SURFACE, linewidth=3)
    ax.tick_params(which="minor", length=0)
    set_title(ax, "The optimization matrix: what each pass actually changes")
    save(fig, out_dir, "opt_matrix")
    return True


# --- Figure 2c: CPI ladder across the whole suite ---


def fig_cpi_ladder(rows, out_dir):
    """Horizontal dot plot of CPI for every measured configuration, sorted.
    Two entities: default pipeline vs a deliberately disabled optimization."""
    disabled = {
        ("regalloc_pressure", "regalloc_off"): "regalloc pressure (regalloc off)",
        ("const_fold_dce", "opt_off"): "const fold bench (folding off)",
        ("peephole_addressing", "peephole_off"): "peephole bench (peephole off)",
        ("frame_pointer_calls", "fp_kept"): "leaf calls (fp kept)",
    }
    entries = []
    for row in rows:
        if row["memory_latency"] != "off" or row["input_size"] != 1.0:
            continue
        key = (row["benchmark"], row["variant"])
        if row["variant"] == "default":
            entries.append((row["benchmark"].replace("_", " "), row["cpi"], False))
        elif key in disabled:
            entries.append((disabled[key], row["cpi"], True))
    if not entries:
        return False
    entries.sort(key=lambda e: e[1])
    labels = [e[0] for e in entries]
    values = [e[1] for e in entries]
    flags = [e[2] for e in entries]
    fig, ax = plt.subplots(figsize=(5.8, 0.28 * len(entries) + 1.2), facecolor=SURFACE)
    style_axes(ax)
    ax.yaxis.grid(False)
    ax.xaxis.grid(True, color=GRID, linewidth=0.6)
    ys = range(len(entries))
    ax.hlines(list(ys), 1.0, values, color=GRID, linewidth=1.2, zorder=1)
    for cat, color, label in [
        (False, SLOT["blue"], "default pipeline"),
        (True, SLOT["yellow"], "optimization disabled"),
    ]:
        xs = [v for v, f in zip(values, flags) if f is cat]
        yy = [y for y, f in zip(ys, flags) if f is cat]
        ax.scatter(xs, yy, s=48, color=color, edgecolor=SURFACE, zorder=3, label=label)
    ax.axvline(1.0, color=BASELINE, linewidth=1)
    ax.set_yticks(list(ys), labels, fontsize=8)
    ax.set_xlim(right=max(values) * 1.03)
    ax.set_xlabel("cycles per instruction (1.0 = ideal in-order)", color=INK_2, fontsize=9)
    ax.invert_yaxis()
    set_title(ax, "CPI across the suite: how far from the ideal pipeline")
    ax.legend(frameon=False, fontsize=8, labelcolor=INK_2, loc="upper right")
    save(fig, out_dir, "cpi_ladder")
    return True


# --- Figure 3: branch misprediction ---


def fig_branch_mispredict(rows, out_dir):
    benches = [
        ("branch_predictable", "predictable"),
        ("branch_random", "random (LCG bit)"),
        ("fib_recursive", "fib recursive"),
        ("strsearch", "string search"),
    ]
    entries = []
    for bench, label in benches:
        found = pick(
            rows,
            benchmark=bench,
            variant="default",
            memory_latency="off",
            input_size=1.0,
        )
        if found and found[0]["mispredict_rate"] is not None:
            entries.append((label, found[0]["mispredict_rate"] * 100.0))
    if not entries:
        return False
    entries.sort(key=lambda e: e[1])
    labels = [e[0] for e in entries]
    values = [e[1] for e in entries]
    fig, ax = new_fig()
    bar_with_labels(ax, labels, values, SLOT["blue"], fmt="{:.1f}%")
    ax.set_ylabel("branch mispredict rate (%)", color=INK_2, fontsize=9)
    set_title(ax, "Branch misprediction across branch behaviors")
    save(fig, out_dir, "branch_mispredict")
    return True


# --- Figure 4: cache miss rates ---


def fig_cache_miss_rates(rows, out_dir):
    """Grouped bars of read MISS rates: hit rates all sit near 100% and are
    indistinguishable; misses carry the whole access-pattern story."""
    benches = [
        ("mem_sequential", "sequential"),
        ("mem_strided", "strided (x8)"),
        ("mem_random", "random"),
    ]
    labels, levels = [], {"l1": [], "l2": [], "l3": []}
    for bench, label in benches:
        found = pick(
            rows,
            benchmark=bench,
            variant="default",
            memory_latency="realistic",
            input_size=1.0,
        ) or pick(
            rows,
            benchmark=bench,
            variant="default",
            memory_latency="off",
            input_size=1.0,
        )
        if not found:
            continue
        labels.append(label)
        for level in levels:
            hit = found[0][f"{level}_read_hit_rate"]
            levels[level].append((1.0 - (hit or 0.0)) * 100.0)
    if not labels:
        return False
    fig, ax = new_fig()
    width = 0.24
    xs = range(len(labels))
    for idx, (level, series) in enumerate(levels.items()):
        offset = (idx - 1) * width
        bars = ax.bar(
            [x + offset for x in xs],
            series,
            width=width - 0.03,
            color=SEQ_BLUE[idx],
            label=level.upper(),
            edgecolor=SURFACE,
        )
        for rect, val in zip(bars, series):
            annotate(
                ax, rect.get_x() + rect.get_width() / 2, rect.get_height(), f"{val:.2f}"
            )
    ax.set_xticks(list(xs), labels)
    ax.set_ylim(0, max(max(s) for s in levels.values()) * 1.25)
    ax.set_ylabel("read miss rate (%)", color=INK_2, fontsize=9)
    set_title(ax, "Cache read misses by access pattern (lower is better)")
    legend_below(fig, ncols=3)
    save(fig, out_dir, "cache_miss_rates")
    return True


# --- Figure 5: memory latency impact ---


def fig_memory_latency_impact(rows, out_dir):
    """Dot plot of cycle inflation for EVERY default bench, sorted: paying
    for memory is a whole-suite tax, not a memory-bench quirk."""
    entries = []
    for row in rows:
        if (
            row["variant"] != "default"
            or row["memory_latency"] != "realistic"
            or row["input_size"] != 1.0
            or row["latency_cycle_ratio"] is None
        ):
            continue
        entries.append((row["benchmark"].replace("_", " "), row["latency_cycle_ratio"]))
    if not entries:
        return False
    entries.sort(key=lambda e: e[1])
    labels = [e[0] for e in entries]
    values = [e[1] for e in entries]
    fig, ax = plt.subplots(figsize=(5.8, 0.28 * len(entries) + 1.2), facecolor=SURFACE)
    style_axes(ax)
    ax.yaxis.grid(False)
    ax.xaxis.grid(True, color=GRID, linewidth=0.6)
    ys = range(len(entries))
    ax.hlines(list(ys), 1.0, values, color=GRID, linewidth=1.2, zorder=1)
    ax.scatter(values, list(ys), s=48, color=SLOT["blue"], edgecolor=SURFACE, zorder=3)
    for y, val in zip(ys, values):
        ax.annotate(
            f"{val:.2f}x",
            (val, y),
            textcoords="offset points",
            xytext=(9, 0),
            va="center",
            fontsize=8,
            color=INK_2,
        )
    ax.axvline(1.0, color=BASELINE, linewidth=1)
    ax.set_yticks(list(ys), labels, fontsize=8)
    ax.set_xlim(1.0, max(values) * 1.12)
    ax.set_xlabel("cycles, realistic / flat latency", color=INK_2, fontsize=9)
    ax.invert_yaxis()
    set_title(ax, "Cost of realistic memory latency (same binaries rerun)")
    save(fig, out_dir, "memory_latency_impact")
    return True


# --- Figure 6: kernel scaling ---


def fig_kernel_scaling(rows, out_dir):
    """Cycles vs input size per kernel, direct-labeled at the line ends so
    near-coincident series stay tellable apart; skipped at a single size."""
    # strsearch is dashed: its line runs within a few percent of matmul on a
    # log axis, and the dash lets the covered line show through.
    kernels = [
        ("sieve", SLOT["blue"], "o", "-"),
        ("matmul", SLOT["aqua"], "s", "-"),
        ("fib_recursive", SLOT["yellow"], "^", "-"),
        ("strsearch", SLOT["green"], "D", (0, (4, 3))),
    ]
    fig, ax = new_fig(width=6.0)
    series = []
    for bench, color, marker, linestyle in kernels:
        found = pick(rows, benchmark=bench, variant="default", memory_latency="off")
        points = sorted(
            (r["input_size"], r["cycles"]) for r in found if r["cycles"] is not None
        )
        if len(points) < 2:
            continue
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        ax.plot(
            xs,
            ys,
            color=color,
            linewidth=2,
            linestyle=linestyle,
            marker=marker,
            markersize=5,
            markeredgecolor=SURFACE,
            markeredgewidth=0.8,
        )
        series.append((bench, xs[-1], ys[-1]))
    if not series:
        plt.close(fig)
        print("kernel_scaling: skipped (need >= 2 input sizes)")
        return False
    ax.set_xlabel("input size multiplier", color=INK_2, fontsize=9)
    ax.set_ylabel("cycles", color=INK_2, fontsize=9)
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.xaxis.set_major_formatter(FormatStrFormatter("%gx"))
    # Direct labels at the right edge, nudged apart in log space on collision.
    import math

    series.sort(key=lambda s: s[2])
    log_ys = [math.log10(y) for _, _, y in series]
    min_gap = 0.09
    for i in range(1, len(log_ys)):
        if log_ys[i] - log_ys[i - 1] < min_gap:
            log_ys[i] = log_ys[i - 1] + min_gap
    for (bench, x, _), ly in zip(series, log_ys):
        ax.annotate(
            bench.replace("_", " "),
            (x, 10**ly),
            textcoords="offset points",
            xytext=(8, 0),
            va="center",
            fontsize=9,
            color=INK_2,
            annotation_clip=False,
        )
    ax.set_ylim(top=10 ** (log_ys[-1] + 0.15))
    ax.set_xlim(right=max(x for _, x, _ in series) * 1.35)
    set_title(ax, "Kernel cycle counts across input sizes")
    save(fig, out_dir, "kernel_scaling")
    return True


FIGURES = [
    fig_opt_speedup,
    fig_opt_matrix,
    fig_cpi_breakdown,
    fig_cpi_ladder,
    fig_branch_mispredict,
    fig_cache_miss_rates,
    fig_memory_latency_impact,
    fig_kernel_scaling,
]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("summary", help="path to summary.csv")
    parser.add_argument(
        "--output",
        default=None,
        help="figure output dir (default: figures/ next to summary.csv)",
    )
    args = parser.parse_args(argv)

    summary = Path(args.summary)
    rows = load_summary(summary)
    out_dir = Path(args.output) if args.output else summary.parent / "figures"
    out_dir.mkdir(parents=True, exist_ok=True)

    made = sum(1 for fig in FIGURES if fig(rows, out_dir))
    print(f"{made}/{len(FIGURES)} figures written to {out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
