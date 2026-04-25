<div align="center">

# Full-Stack

[![Live Demo](https://img.shields.io/badge/Live%20Demo-GitHub%20Pages-2ea44f?logo=github)](https://lpc4.github.io/Full-Stack/)
[![Deploy Pages](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml/badge.svg)](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

<div align="center">

### An interactive compiler pipeline explorer
*From high-level language semantics down to IR, assembly, and machine execution*

**[🌐 Live Demo](https://lpc4.github.io/Full-Stack/)**

</div>

---

## Overview

This project implements a complete compilation pipeline for HLL, a systems programming language with a consistency-first memory model and explicit pointer semantics. The pipeline includes:

- **High-Level Language Frontend**: Lexer, recursive-descent parser, AST, semantic analysis, and HLL→IR lowering.
- **Intermediate Representation**: Strongly-typed, SSA-form IR with aggregate types, control flow, and snapshot-tested code generation.
- **Assembly Backend**: Growing RISC-V RV64IMAFD backend with full instruction encoding specifications.
- **Interactive UI**: Desktop and web application for live editing, compilation, and step-through visualization of each pipeline stage.
- **Test Infrastructure**: Golden tests, fixture programs, and reproducible compilation pipelines.

---

## Examples

All example programs, IR outputs, and interactive demonstrations are available in the live GitHub Pages deployment:

<div align="center">

**[https://lpc4.github.io/Full-Stack/](https://lpc4.github.io/Full-Stack/)**

</div>

---

## Quick Start

### Run Locally (Desktop)

```bash
cargo run --release
```

### Run in Browser (WASM)

```bash
# Install prerequisites
rustup target add wasm32-unknown-unknown
cargo install --locked trunk

# Serve with hot-reload
trunk serve
```

Open `http://127.0.0.1:8080` in your browser. Append `#dev` to bypass service-worker caching during development: `http://127.0.0.1:8080/#dev`

### Build Release Bundle

```bash
trunk build --release
# Output: dist/
```

---

## Testing

```bash
# Run all tests
cargo test

# Run with output capture
cargo test -- --nocapture

# Test specific module
cargo test -p full_stack intermediate_language
```

Golden tests reside in `tests/` and compare generated IR against expected outputs.

---

## Development

### Prerequisites
- Rust 1.75+ (via `rustup`)
- `trunk` (for WASM builds)
- A modern browser (for web target)

### Useful Commands

```bash
# Format code
cargo fmt --all

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Check WASM build
cargo check --target wasm32-unknown-unknown

# Profile compilation
cargo build --release --timings
```

---

## Documentation

- **Language Specification**: [`src/1_high_level_language/_LANG_SPECIFICATIONS.md`](src/1_high_level_language/_LANG_SPECIFICATIONS.md)
- **IR Specification**: [`src/2_intermediate_language/_IR_SPECIFICATIONS.md`](src/2_intermediate_language/_IR_SPECIFICATIONS.md)
- **RISC-V Backend**: [`src/3_assembly_language/_RISC_SPECIFICATIONS.md`](src/3_assembly_language/_RISC_SPECIFICATIONS.md)

---

## Contributing

Contributions are welcome. Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/your-feature`)
3. Commit changes with clear, descriptive messages
4. Open a Pull Request with a summary of changes

For significant changes, please open an issue first to discuss your approach.

---

<div align="center">

## License

MIT License — see [LICENSE](LICENSE) for details.

*Built with Rust, eframe, and egui.*

</div>