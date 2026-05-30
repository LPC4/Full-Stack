# hll-to-ir

**Stages 1 and 2 of the Full-Stack compiler pipeline.**

Front end for the HLL systems language. Takes HLL source text and produces a typed,
SSA-form `IrProgram` ready for the `ir-to-asm` back end. This crate owns lexing,
parsing, semantic analysis, IR lowering, and the per-mode stdlib bundling.

## Flow

```
HLL source
  -> Lexer              characters -> tokens
  -> Parser             tokens -> AST
  -> Semantic analysis  type checking, symbol resolution, diagnostics
  -> IR compiler        AST -> IrProgram (typed SSA)
```

The stdlib is compiled per `TargetMode` (see below) and lowered through the same path,
so library and user code share one IR representation.

## Public API

```rust
use hll_to_ir::{HllCompiler, CompileConfig, TargetMode, IrProgram};

let config = CompileConfig { mode: TargetMode::Hosted, ..Default::default() };
let compiler = HllCompiler::new(config);
let output = compiler.compile(source)?;   // HllOutput { ir, diagnostics, ... }
let ir: &IrProgram = &output.ir;
```

`TargetMode` selects which stdlib bundle is linked in and how the runtime is built:

| Mode | Entry point | Runtime / syscalls |
|------|-------------|--------------------|
| `Hosted` | `_start` | Linux-style `ecall` (write 64, exit 93) |
| `Freestanding` | `_start` | bare-metal, direct UART MMIO, no syscalls |
| `Kernel` | `_kernel_start` | S-mode kernel bundle, no Linux syscalls |

The IR types (`IrProgram`, `IrFunction`, `IrBlock`, `IrInstruction`, `IrType`, ...) are
re-exported from the crate root so the back end can consume them directly.

## Module layout

```
src/
  compiler/   lexer-to-IR lowering, semantic analysis, symbol and type tables
  ir/         IR data model: program, functions, blocks, instructions, values, types
```

Lexer, parser, AST, the top-level `HllCompiler`, and the stdlib bundler live alongside
these as sibling modules.

## Standard library

The HLL stdlib sources are not stored in this crate. They live in `os-runtime`
(`crates/os-runtime/stdlib` and `crates/os-runtime/kernel`) and are pulled in as
compile-time string constants. `stdlib.rs` defines the per-mode link order and the
shared type prelude (`Str`, `HeapBlock`). Each module is compiled independently so
that `.ir`, `.s`, and `.o` artifacts exist per source file rather than as one bundle.
