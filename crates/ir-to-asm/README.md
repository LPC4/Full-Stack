# ir-to-asm

**Stage 3 of the Full-Stack compiler pipeline.**

Lowers an `IrProgram` (SSA IR) to RISC-V RV64 assembly, producing a `Vec<RvInstruction>` token stream.

## What it does

```
IrProgram
  -> Linear-scan register allocator
  -> Stack frame layout
  -> RV64 instruction selection
  -> Vec<RvInstruction>
```

## Public API

```rust
use ir_to_asm::CompilerRv64;
use hll_to_ir::IrProgram;
use asm_to_binary::RvInstruction;

let mut compiler = CompilerRv64::new();
let tokens: Vec<RvInstruction> = compiler.compile(&ir_program);
```