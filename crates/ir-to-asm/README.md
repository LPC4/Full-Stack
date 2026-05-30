# ir-to-asm

**Stage 3 of the Full-Stack compiler pipeline.**

Back end that lowers a typed SSA `IrProgram` into RV64IMAFD assembly. It performs
register allocation, lays out each function's stack frame, selects machine
instructions, and emits the `.text`, `.data`, and `.rodata` sections.

## Flow

```
IrProgram
  -> Register allocator   SSA virtual registers -> RV64 GPR/FPR (linear scan)
  -> Frame layout         spill slots, saved registers, locals per function
  -> Instruction select   IR ops -> RV64 instructions
  -> Emitter              assembly text and/or Vec<RvInstruction> tokens
```

## Public API

```rust
use ir_to_asm::CompilerRv64;
use hll_to_ir::IrProgram;
use asm_to_binary::RvInstruction;

let mut compiler = CompilerRv64::new();

// Assembly text:
let asm: String = compiler.compile(&ir_program);

// Or text plus the structured token stream for the assembler:
let (asm, tokens): (String, Vec<RvInstruction>) = compiler.compile_with_tokens(&ir_program);
```

`compile_with_tokens` returns the same `RvInstruction` token type that `asm-to-binary`
consumes, so stage 3 and stage 4 hand off without re-parsing text.

## Module layout

```
src/
  compiler/   register allocation, frame and function contexts, data section,
              instruction selection, and the assembly emitter
```
