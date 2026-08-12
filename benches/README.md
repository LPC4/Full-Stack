# Benchmarks

The benchmark runner and its 15 HLL fixtures are workspace-owned. Generated raw data,
analysis, figures, and reports are written below `benches/results/`, which is ignored.

List the available benchmarks:

```text
cargo run -p benchmark-runner -- list
```

Run the two memory-latency smoke modes:

```text
cargo run --release -p benchmark-runner -- run --suite teaching --benchmark load_use_chain --memory-latency off --output benches/results/raw/smoke-single.jsonl
cargo run --release -p benchmark-runner -- run --suite teaching --benchmark load_use_chain --memory-latency realistic --output benches/results/raw/smoke-latency.jsonl
```

Omit `--benchmark` for a full suite run. Use `--samples 20 --warmups 3` for the repeated
base configuration used by scheduled CI. The scheduled workflow also runs the four kernel
benchmarks at `--input-size 2`, which gives the scaling figure a second measurement.

Analyze runs and verify the formulas:

```text
python benches/tools/analyse.py benches/results/raw/smoke-single.jsonl benches/results/raw/smoke-latency.jsonl --output benches/results/generated --fail-on-regression
python -m unittest benches/tools/test_analyse.py
```

Install the plotting requirements and generate exhibits, figures, and a report:

```text
python -m pip install -r benches/tools/requirements.txt
cargo run --release -p benchmark-runner -- exhibits --output benches/results/exhibits
python benches/tools/figures.py benches/results/generated/summary.csv
python benches/tools/report.py benches/results/generated/summary.csv --exhibits benches/results/exhibits
```

The figure command writes eight PNG/PDF pairs when the summary contains both latency modes,
all optimization variants, and at least two input sizes for the kernel benchmarks. Partial
smoke data produces only the applicable figures.
