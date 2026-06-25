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
and a `fsc` CLI) or in the browser via WebAssembly. The kernel target boots a real
S-mode operating system on the VM: paging, processes, a filesystem, an interactive
shell, a line editor, and an assembler that runs *inside* the VM so you can write,
assemble, and run a program without ever leaving the machine.


## Highlights

- **A self-hosting toolchain inside the VM.** Boot the kernel, drop into the shell, write
  an `.hll` or `.s` file in the in-VM editor, compile it with `cc`, assemble it with `as`,
  link it with `ld`, and `run` the result, all without leaving the guest. The compiler (`/bin/cc`),
  assembler (`/bin/as`), and linker (`/bin/ld`) are themselves HLL programs compiled by
  this toolchain and executed as user processes.
- **A real interactive OS.** An S-mode kernel with Sv39 paging, per-process address
  spaces, a round-robin preemptive scheduler, an inode read-write filesystem, and a
  shell with `ls`/`cd`/`cat`/`edit`/`cc`/`as`/`ld`/`run`/file management. Foreground
  programs return to the prompt on exit and can be interrupted with Ctrl-C.
- **A complete front-to-back compiler.** Lexer, parser, semantic analysis, typed SSA
  IR, register allocation + slot coloring, RV64IMAFD code generation with an optional
  peephole pass, an assembler, and an object linker that emits ELF-64 with relocations.
- **A cycle-stepped VM with real I/O.** A 5-stage pipelined RV64IMAFD core driving a
  UART console, a keyboard event device, and a 320x240 framebuffer, enough to run a
  spinning cube you steer with WASD, a live Mandelbrot renderer, and Conway's Game of Life.
- **Everything visualised.** Tokens, AST, IR, assembly, CFG, memory map, cache state,
  disassembly, stack, registers, and a cycle-stepping debugger over the live pipeline,
  plus OS-level panels: a process inspector with per-process stack view, a live syscall
  trace, and a filesystem inspector with a tree view and file preview.


## What you can do in the booted OS

Boot the kernel target (the GUI's machine window, or `fsc run kernel.elf`) and you land
at a shell prompt running as pid 1:

```
$ ls
/bin   
/home
$ cd home/src
$ edit hello.hll        ; line editor: append, insert, substitute, delete, write
$ cc hello.hll hello.s  ; compile HLL -> assembly, inside the VM
$ as hello.s hello.o    ; assemble -> a relocatable object
$ as stdlib.s stdlib.o  ; assemble the tiny stdlib it links against
$ ld stdlib.o hello.o hello  ; link the objects -> a runnable ELF
$ run hello             ; exec it as a child process; the shell reaps it
HLL0
Y
[exit 36]
$ as array.s array.elf  ; as can also wrap a standalone .s straight into a runnable ELF
$ run array.elf
[exit 42]
$ cube                  ; spinning wireframe cube in the framebuffer tab (WASD to rotate)
$ mandelbrot            ; Mandelbrot set rendered to the framebuffer
$ life                  ; Conway's Game of Life on a toroidal grid
```

The shell, editor (`edit`), compiler (`cc`), assembler (`as`), and linker (`ld`) are
ordinary HLL programs in `crates/os-runtime/user/`, compiled by this pipeline and
installed into the filesystem image. Nothing about them is privileged, they reach the
kernel only through `ecall`.


## Features

- **Complete front-to-back compiler**: lexer, parser, semantic analysis, typed SSA IR,
  register allocation + slot coloring, RV64IMAFD code generation with peephole
  optimization, assembler, and an object linker that emits ELF-64 with relocations.
- **Per-file compilation**: each HLL source (stdlib modules and user code alike)
  compiles to its own `.o` and is linked with full relocation, exactly like a real
  toolchain. The stdlib uses a distinct string-literal prefix so its rodata labels
  never collide with user code at link time.
- **A cycle-stepped VM**: 5-stage in-order pipeline (IF/ID/EX/MEM/WB) with data
  forwarding, load-use hazard detection, and 2-bit branch prediction over a
  three-level write-back cache hierarchy, plus an Sv39 MMU, M/S/U privilege modes,
  CLINT/PLIC interrupt controllers, an NS16550A UART, a keyboard event device, and a
  linear framebuffer.
- **A bundled OS runtime**: M-mode boot firmware (PMP, delegation, trap handlers),
  an S-mode paging kernel with a round-robin scheduler, an inode-based read-write
  filesystem, and a shell with an in-VM editor, compiler, assembler, and linker.
- **Self-hosting in the guest**: a full toolchain runs as userspace programs inside the
  VM. `/bin/cc` compiles an HLL subset to assembly, `/bin/as` assembles to objects or a
  runnable ELF, and `/bin/ld` links objects into an ELF, closing the source-to-binary
  loop without the host toolchain.
- **Every stage visualised**: tokens, AST, IR, assembly, CFG, memory map, cache
  state, disassembly, stack, registers, and a cycle-stepping debugger showing the
  pipeline, registers, caches, and I/O. OS-level panels add a process inspector with
  a per-process stack view, a live syscall trace, and a filesystem tree inspector.
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

Launch the desktop app with `cargo run --release`. The machine window boots the kernel,
shows the UART console and the framebuffer side by side, and forwards your keyboard to
the guest (text to the UART, key events to the keyboard device when the framebuffer tab
is focused). A read-only debugger panel exposes the live pipeline, caches, and registers.

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

- `T*` is a pointer and is never implicitly dereferenced; use `@ptr` to read or write
  through it, and `&var` to take an address.
- Structs, enums, arrays, slices, generics, and inline aggregate returns via
  multiple-return-value structs.
- Struct literals use `.field = value` (a leading `:` introduces a type, never a value),
  with omitted fields zero-filled and contextual inference of the struct type.
- `enum`/`match` with exhaustiveness checking, and a `?` operator for error propagation.
- `defer` for deterministic cleanup, and `new` / `free` for manual memory management.
- Compile-time evaluation of pure functions, loops, and recursion.
- `asm { }` blocks for inline RISC-V assembly.
- C interop through `external` declarations.
- `assert`, `panic`, and `print` built-in for hosted mode.

The language (HLL v2) is intentionally minimal and considered feature-complete: it has
`if`/`while`/`for`/`break`/`continue`/`defer`, functions, structs, enums, `match`, fixed
arrays, slices and ranges, pointers, `as` casts, compound assignment, the `?` operator,
`import`/`export`, and floats. The full grammar and semantics are in the
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

Each crate also has a `README.md` with its flow, public API, and module layout.


## Testing

```sh
cargo test
cargo test -- --nocapture   # show UART output
```

The suite spans unit tests, golden IR/assembly snapshots in `programs/test/`, VM
execution tests that compile HLL and assert on UART output, and kernel integration
tests that boot the shell, assemble and run a program in the guest, and verify a clean
exit.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE),
at your option.
</content>
</invoke>
