# hll-to-ir

**Stage 1 & 2 of the Full-Stack compiler pipeline.**

Compiles HLL (High Level Language) source code through lexing, parsing, semantic analysis, and IR lowering, producing an `IrProgram` in SSA form.

## What it does

```
HLL Source
  → Lexer         → tokens
  → Parser        → AST
  → Semantic      → diagnostics
  → IR Compiler   → IrProgram
```

## Public API

```rust
use hll_to_ir::{CompilationPipeline, TargetMode, IrProgram};

let pipeline = CompilationPipeline::new(TargetMode::Hosted);
let ir = pipeline.compile(source)?;
```

`TargetMode` controls which stdlib bundle is linked:

| Mode | Entry point | Syscalls |
|------|-------------|----------|
| `Hosted` | `_start` | Linux (`ecall`) |
| `Freestanding` | user-defined | none |

## Standard library

Platform files live under `platform/` and are embedded at compile time:

```
platform/
  stdlib/
    common/   types.hll  memory_allocator.hll  string_utils.hll  mem.hll  klog.hll
    hosted/   runtime.hll
    freestanding/  runtime.hll  console.hll
  kernel/     runtime_kernel.hll
```

See `src/stdlib.rs` for how these are bundled per target mode.
