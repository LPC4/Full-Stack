# Full-Stack

[![Demo](https://img.shields.io/badge/Demo-GitHub_Pages-5e8c61?logo=github)](https://lpc4.github.io/Full-Stack/)
[![License](https://img.shields.io/badge/License-MIT_%2F_Apache--2.0-5e8c61)](#license)

Full-Stack is a self-contained compiler pipeline for a small systems language (HLL).
It carries source all the way to machine code and runs the result on a built-in
RISC-V CPU, with every stage inspectable in a graphical UI:

```
HLL source -> IR -> RISC-V assembly -> ELF object -> virtual machine
```

The whole toolchain is written in Rust and runs either natively (egui desktop app
and a `fsc` CLI) or in the browser via WebAssembly. Three target modes are
supported: **hosted** (Linux syscall ABI), **freestanding** (bare-metal, no OS),
and **kernel** (an S-mode kernel with boot ROM, paging, processes, a filesystem,
and an interactive shell).


## Features

- **Complete front-to-back compiler**: lexer, parser, semantic analysis, typed SSA IR,
  register allocation + slot coloring, RV64IMAFD code generation with peephole
  optimization, assembler, and an object linker that emits ELF-64 with relocations.
- **Per-file compilation**: each HLL source (stdlib modules and user code alike)
  compiles to its own `.o` and is linked with full relocation, exactly like a real
  toolchain. The stdlib uses a distinct string-literal prefix so its rodata labels
  never collide with user code at link time.
- **A cycle-accurate VM**: 5-stage in-order pipeline (IF/ID/EX/MEM/WB) with data
  forwarding, load-use hazard detection, and 2-bit branch prediction over a
  three-level write-back cache hierarchy, plus an Sv39 MMU, M/S/U privilege modes,
  CLINT/PLIC interrupt controllers, and NS16550A UART.
- **A bundled OS runtime**: M-mode boot firmware (PMP, delegation, trap handlers),
  an S-mode paging kernel with round-robin scheduler, an inode-based read-write
  filesystem, and a shell with `ls`/`cd`/`cat`/`run`/`edit`/...
- **Every stage visualised**: tokens, AST, IR, assembly, CFG, memory map, cache
  state, disassembly, stack, registers, and a cycle-stepping debugger showing the
  pipeline, registers, caches, and I/O.
- **Three target modes**:
  - **Hosted** - Linux RV64 syscall ABI (write, exit, brk, mmap, ...).
  - **Freestanding** - bare-metal, no OS dependencies.
  - **Kernel** - full kernel with paging, processes, filesystem, and shell.
- **IR-level optimization**: optional local constant folding and dead-code elimination.


## Architecture

```
HLL Source
  -> Lexer / Parser        tokens, AST
  -> Semantic Analysis     type checking, diagnostics
  -> IR Compiler           typed SSA IR
  -> RISC-V Emitter        register allocation, slot coloring, RV64IMAFD assembly
  -> Assembler             per-file .o objects (.text/.data/.rodata/.bss + symbols)
  -> Object Linker         symbol resolution + relocation -> ELF-64
  -> Virtual Machine       5-stage pipelined CPU
```

Each HLL file is compiled independently to its own object, then linked. No source
concatenation happens before assembly. The VM loads the linked image (or a kernel
built from the `os-runtime` sources) and steps it one CPU cycle at a time.

See the [specifications](#documentation) for the full detail of each stage.


## Getting started

```sh
# Native desktop app (egui GUI)
cargo build --release
cargo run --release

# CLI only (fsc)
cargo build --release --bin fsc
cargo run --release --bin fsc -- help

# Run the test suite
cargo test

# Web build (requires trunk: cargo install trunk)
trunk serve            # dev server with hot-reload
trunk build --release  # static bundle in dist/
```

> The browser build runs the compiler and UI client-side, but does not execute the
> VM. Use the native build to run programs. A live build is hosted at
> [lpc4.github.io/Full-Stack](https://lpc4.github.io/Full-Stack/).


## Usage

### GUI

Launch the desktop app with `cargo run --release`.

### CLI (`fsc`)

```sh
cargo build --release --bin fsc

fsc hll-to-ir  program.hll -o program.ir          # compile to IR
fsc hll-to-asm program.hll -o program.s           # compile to assembly
fsc hll-to-asm program.hll --emit-o -o program.o  # compile to relocatable object
fsc link       main.hll utils.hll -o program.elf  # compile and link multiple sources
fsc run        program.hll                        # compile and run on the VM
fsc run        program.s                          # load raw assembly text
fsc run        kernel.elf                         # load a pre-linked ELF
```

## The language

HLL is a small systems language built around explicit, predictable memory access:

- `T*` is a pointer and is never implicitly dereferenced, use `@ptr` to read
  or write through it, and `&var` to take an address.
- Structs, arrays, generics, and inline aggregate returns via multiple-return-value
  structs.
- `defer` for deterministic cleanup, and `new` / `free` for manual memory
  management.
- Compile-time evaluation of pure functions, loops, and recursion.
- `asm { }` blocks for inline RISC-V assembly.
- C interop through `external` declarations.
- `assert`, `panic`, and `print` built-in for hosted mode.

The full grammar and semantics are in the
[language specification](crates/hll-to-ir/_LANG_SPECIFICATIONS.md).


## Repository layout

| Path | Contents |
|------|----------|
| `src/` | Application entry point, egui UI, compilation pipeline, machine-window runner |
| `crates/hll-to-ir/` | Lexer, parser, semantic analysis, IR compiler, stdlib bundles |
| `crates/ir-to-asm/` | IR to RISC-V assembly: register allocation, slot coloring, peephole |
| `crates/asm-to-binary/` | Assembler, linker, ELF output (executables and relocatable objects) |
| `crates/virtual-machine/` | VM: 5-stage CPU pipeline, caches, MMU, devices, bus |
| `crates/os-runtime/` | Boot firmware, kernel sources, standard library, user programs |
| `programs/` | Example HLL programs and the golden compiler test suite |
| `tests/` | Rust integration tests (VM execution, compiler suite, kernel boots) |


## Documentation

Each crate has a specification covering its design and contract.

| Area | Document |
|------|----------|
| HLL language | [`_LANG_SPECIFICATIONS.md`](crates/hll-to-ir/_LANG_SPECIFICATIONS.md) |
| IR design | [`_IR_SPECIFICATIONS.md`](crates/ir-to-asm/_IR_SPECIFICATIONS.md) |
| RISC-V backend | [`_RISCV_SPECIFICATIONS.md`](crates/asm-to-binary/_RISCV_SPECIFICATIONS.md) |
| VM and CPU | [`_VM_SPECIFICATION.md`](crates/virtual-machine/_VM_SPECIFICATION.md) |
| OS and kernel runtime | [`_OS_SPECIFICATION.md`](crates/os-runtime/_OS_SPECIFICATION.md) |


## Testing

```sh
cargo test
cargo test -- --nocapture   # show UART output
```

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE),
at your option.
