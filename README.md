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

- Complete front-to-back compiler: lexer, parser, semantic analysis, typed SSA IR,
  register allocation, RV64IMAFD code generation, assembler, and an object linker
  that emits ELF-64.
- Per-file compilation: each source compiles to its own `.o` and is linked with
  relocation, exactly like a real toolchain.
- A cycle-accurate VM: 5-stage in-order pipeline (IF/ID/EX/MEM/WB) with data
  forwarding, load-use hazard detection, and 2-bit branch prediction over a
  three-level write-back cache hierarchy, plus an Sv39 MMU and M/S/U privilege modes.
- A bundled OS runtime: boot firmware, a paging kernel, a round-robin scheduler, an
  inode-based read-write filesystem, and a shell with `ls`/`cd`/`cat`/`run`/`edit`.
- Every stage is visualised: tokens, AST, IR, assembly, CFG, memory map, and a
  cycle-stepping debugger showing the pipeline, registers, caches, and I/O.


## Architecture

```
HLL Source
  -> Lexer / Parser        tokens, AST
  -> Semantic Analysis     type checking, diagnostics
  -> IR Compiler           typed SSA IR
  -> RISC-V Emitter        register allocation, RV64IMAFD assembly
  -> Assembler             per-file .o objects (.text/.data/.rodata/.bss + symbols)
  -> ObjectLinker          symbol resolution + relocation -> ELF-64
  -> Virtual Machine       5-stage pipelined CPU
```

Each HLL file (the standard library modules and user sources alike) is compiled
independently to its own object, then linked. The stdlib is compiled with a
distinct string-literal prefix so its rodata labels never collide with user code at
link time. The VM loads the linked image (or a kernel built from the `os-runtime`
sources) and steps it one CPU cycle at a time.

See the [specifications](#documentation) for the full detail of each stage.


## Getting started

Requires a recent stable Rust toolchain.

```sh
# Native desktop app (egui GUI)
cargo build --release
cargo run --release

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

A minimal HLL program:

```hll
type Point = { x: f32, y: f32 }

scale: (p: Point*, factor: f32) -> f32 {
    @p.x = @p.x * factor
    @p.y = @p.y * factor
    return @p.x + @p.y
}

main: () -> i32 {
    p: Point* = new(Point)
    @p = { .x = 3.0, .y = 4.0 }
    scale(p, 2.0)
    free(p)
    return 0
}
```

The GUI compiles and runs programs interactively. The `fsc` CLI offers the same
pipeline without the UI:

```sh
cargo build --release --bin fsc

fsc hll-to-ir  program.hll              # compile to IR (stdout)
fsc hll-to-asm program.hll -o out.s     # compile to RISC-V assembly
fsc link       main.hll utils.hll -o program.elf
fsc run        program.hll              # compile and run on the VM
fsc run        program.hll --mode freestanding
```

### Subcommands

| Command | Description |
|---------|-------------|
| `fsc hll-to-ir <file.hll>` | Compile HLL source to IR |
| `fsc hll-to-asm <file.hll>` | Compile HLL source to RISC-V assembly |
| `fsc link <file.hll>...` | Compile and link multiple sources into an ELF |
| `fsc run <file>` | Compile and run through the built-in VM |
| `fsc help` | Show usage |

### Options

| Flag | Description |
|------|-------------|
| `-o, --output <path>` | Write output to a file instead of stdout |
| `-m, --mode <hosted\|freestanding>` | Target mode (default: hosted) |
| `--emit-o` | For `hll-to-asm`, emit a relocatable `.o` |
| `--max-steps <n>` | VM step limit for `run` (default: 50000000) |


## The language

HLL is a small systems language built around explicit, predictable memory access.

- `T*` is a pointer and is never implicitly dereferenced: use `@ptr` to read or
  write through it and `&var` to take an address.
- Structs, arrays, generics, and inline aggregates (multiple returns via structs).
- `defer` for deterministic cleanup, and `new` / `free` for manual memory.
- Compile-time evaluation of pure functions, loops, and recursion.
- C interop through `external` declarations.

The full grammar and semantics are in the
[language specification](crates/hll-to-ir/_LANG_SPECIFICATIONS.md).


## Repository layout

| Path | Contents |
|------|----------|
| `src/` | Application entry point, egui UI, and the compilation pipeline |
| `crates/hll-to-ir/` | Lexer, parser, semantic analysis, IR compiler |
| `crates/ir-to-asm/` | IR to RISC-V assembly lowering |
| `crates/asm-to-binary/` | Assembler, linker, ELF output |
| `crates/virtual-machine/` | VM: CPU pipeline, caches, MMU, devices |
| `crates/os-runtime/` | Kernel sources, standard library, boot firmware |
| `programs/` | Example programs and the golden compiler test suite |
| `tests/` | Rust integration tests |


## Documentation

Each crate has a README describing its API, and a specification with the full detail.

| Area | Specification |
|------|---------------|
| HLL language | [`_LANG_SPECIFICATIONS.md`](crates/hll-to-ir/_LANG_SPECIFICATIONS.md) |
| IR design | [`_IR_SPECIFICATIONS.md`](crates/ir-to-asm/_IR_SPECIFICATIONS.md) |
| RISC-V backend | [`_RISCV_SPECIFICATIONS.md`](crates/asm-to-binary/_RISCV_SPECIFICATIONS.md) |
| VM and CPU | [`_VM_SPECIFICATION.md`](crates/virtual-machine/_VM_SPECIFICATION.md) |
| OS and kernel runtime | [`_OS_SPECIFICATION.md`](crates/os-runtime/_OS_SPECIFICATION.md) |


## Testing

Golden-file tests compare generated IR and assembly against expected snapshots.
Integration tests compile and execute programs through the full pipeline and assert
on exit codes and UART output, including the kernel boot path.

```sh
cargo test
cargo test -- --nocapture   # show UART output
```


## Contributing

Pull requests are welcome. For larger changes, please open an issue first to discuss
the approach. Keep comments ASCII-only and follow the existing style.


## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE),
at your option.
