# asm-to-binary

**Stage 4 of the Full-Stack compiler pipeline.**

Three-pass assembler that converts a `Vec<RvInstruction>` token stream (or raw RISC-V assembly text) into an `AssembledOutput` containing ELF sections and a symbol table.

## What it does

```
Vec<RvInstruction>
  → Pass 0 (parser)  - RvInstruction → Vec<AsmToken>
  → Pass 1 (layout)  - compute label byte addresses
  → Pass 2 (encode)  - emit bytes, resolve relocations
  → AssembledOutput  (sections + symbol table)
```

## Public API

```rust
use asm_to_binary::{Assembler, AssembledOutput, RvInstruction};

let output: AssembledOutput = Assembler::assemble(tokens)?;
```

`AssembledOutput` holds `Vec<SectionData>` and a `SymbolTable` with global/local label addresses.

## Instruction types

```rust
pub enum RvInstruction {
    Real(RealInstruction),
    Pseudo(PseudoInstruction),
    Label(String),
    Directive(Directive),
}
```

These types are re-exported for use by `ir-to-asm` so both stages share a single definition.
