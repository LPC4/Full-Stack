<div align="center">

<img src="assets/icon/icon.svg" alt="Full-Stack icon" width="128" />

# Full‑Stack

### Interactive compiler pipeline, from source to machine code

[![Demo](https://img.shields.io/badge/Demo-GitHub_Pages-5e8c61?logo=github)](https://lpc4.github.io/Full-Stack/)
[![Rust](https://img.shields.io/badge/Rust-1.92+-5e8c61?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT_%2F_Apache--2.0-5e8c61)](LICENSE)
[![RISC‑V](https://img.shields.io/badge/RISC‑V-RV64IMAFD-5e8c61?logo=riscv)](https://riscv.org)

</div>

---

## Overview

Full‑Stack is a **self‑contained compiler pipeline** for a custom systems language.  
Every stage -- lexing, parsing, semantic analysis, IR generation, register allocation, RISC‑V code emission, and two-pass assembly to machine code -- runs directly in the browser (or natively) and is **visualised in real time**.

Runtime execution uses the built-in VM on native desktop builds. The WebAssembly build is currently for compilation and visualization only.

The pipeline compiles HLL source all the way to **RV64IMAFD machine code** (ELF-ready section blobs).  
Execution uses a built-in **5-stage pipelined CPU** with data forwarding, load-use hazard detection, and 2-bit branch prediction.  
All components are written in Rust and exposed through an egui interface.

---

## Compiler pipeline

```
HLL Source
  → Lexer / Parser        tokens, AST
  → Semantic Analysis     diagnostics
  → IR Compiler           typed SSA IR
  → RISC-V Emitter        Vec<RvInstruction>  →  assembly text
  → Two-pass Assembler    AssembledOutput  (.text / .data / .rodata / .bss + symbol table)
  → VM                    5-stage pipelined CPU
```

| Stage | View | What you see |
|-------|------|--------------|
| **Source** | `Source` | Syntax‑highlighted editor for HLL programs |
| **Tokens** | `Tokens` | Raw token stream from the lexer |
| **AST** | `AST` | Abstract syntax tree (pretty‑printed) |
| **IR** | `IR` | Typed, SSA‑form intermediate representation |
| **Assembly** | `Assembly` | Generated RISC‑V assembly text (RV64IMAFD) |
| **Stack** | `Stack` | Stack frame layout, saved registers, locals per function |
| **CFG** | `CFG` | Control-flow graph |
| **Memory map** | `Memory Map` | Section layout and symbol addresses |

All panels are resizable and rearrangeable; the layout persists across sessions.

---

## Debug session

Starting a debug session compiles the current program and loads it into the built-in VM on native desktop builds.  
Step through execution one pipeline cycle at a time and inspect the full machine state.

| Panel | What you see                                                  |
|-------|---------------------------------------------------------------|
| **Pipeline** | Waterfall diagram -- branch prediction accuracy in the footer |
| **CPU State** | All 32 integer and 32 FP registers; highlighted on change     |
| **Disassembly** | Disassembled `.text` with the current PC marker               |
| **Memory** | Raw memory bytes with jump presets for each section           |
| **Cache** | L1/L2/L3 hit-rate and access count statistics                 |
| **I/O** | UART output from `ecall` write / putchar syscalls             |

---

## CPU: 5-stage pipelined RV64IMAFD

The built-in virtual machine implements a classic in-order scalar pipeline:

```
IF  →  ID  →  EX  →  MEM  →  WB
```

**Hazard handling**

| Hazard | Mechanism |
|--------|-----------|
| RAW (register-to-register) | EX/MEM→EX and MEM/WB→EX forwarding |
| Load-use | 1-cycle bubble; pipeline stalls (IF held, bubble injected after ID) |
| Branch mispredict | 2-cycle flush; IF and ID squashed, fetch redirected |

**Branch prediction**: 2-bit bimodal predictor with Branch Target Buffer (BTB).

---

## Live version

No install required -- the compiler and UI run client-side via WebAssembly.

Note: browser builds do not currently run the VM. For execution, use the native desktop build.

<p align="center">
  <a href="https://lpc4.github.io/Full-Stack/">
    <img src="assets/readme/demo.png" alt="Full‑Stack demo" width="85%" />
  </a>
</p>

**[Open the live app →](https://lpc4.github.io/Full-Stack/)**

---

## The language

The project includes a small systems language called **HLL** (High‑Level Language).  
It was designed to make memory operations completely explicit and predictable.

- **`T*` is a pointer, never implicitly dereferenced.**  
  Use `@ptr` to read/write, `&var` to take an address.
- **Structs, arrays, generics, and inline aggregates** (multiple returns via structs).
- **`defer`** for deterministic cleanup.
- **Compile‑time evaluation** -- pure functions, loops, recursion all resolved at build time.
- **Manual memory management** with `new`/`free`.
- **C interop** via `external` declarations.

A small example:

```hll
type Point = { x: f32, y: f32 }

calc_offset: (p: Point*, shift: f32) -> f32 {
    @p.x = @p.x + shift
    @p.y = @p.y + shift
    return @p.x * @p.y
}

main: () -> i32 {
    p: Point* = new(Point)
    @p = { .x = 3.0, .y = 4.0 }
    result: f32 = calc_offset(p, 1.0)
    free(p)
    return 0
}
```

For the full specification, see the [language reference](src/1_high_level_language/_LANG_SPECIFICATIONS.md).

---

## Documentation

- [Language specification](src/1_high_level_language/_LANG_SPECIFICATIONS.md)
- [IR design](src/2_intermediate_language/_IR_SPECIFICATIONS.md)
- [RISC‑V backend](src/3_assembly_language/_RISCV_SPECIFICATIONS.md)
- [VM / CPU specification](src/4_virtual_machine/_VM_SPECIFICATION.md)

---

## Build & run

```bash
# Native desktop build (egui GUI)
cargo build --release

# Web (WebAssembly) build -- requires trunk
trunk build --release
trunk serve          # dev server with hot-reload

# Run all tests
cargo test
```

---

## Testing

Golden‑file tests compare generated IR and assembly against expected snapshots.  
Integration tests compile and execute HLL programs through the full pipeline and assert on exit codes and UART output.

```bash
cargo test
cargo test -- --nocapture   # full output
```

---

## Contributing

Pull requests are welcome. For larger changes, please open an issue first to discuss the approach.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/your-change`)
3. Commit with clear messages
4. Push and open a PR

---

## License

Dual‑licensed under MIT and Apache 2.0 -- see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

---

<div align="center">
  <sub>Built with Rust and <a href="https://github.com/emilk/egui">egui</a></sub>
</div>
