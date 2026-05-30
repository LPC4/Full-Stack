# asm-to-binary

**Stage 4 of the Full-Stack compiler pipeline.**

Assembler and linker. Converts a `Vec<RvInstruction>` token stream (or raw RISC-V
assembly text) into a relocatable `AssembledOutput`, links one or more objects into a
single image, and exports an ELF-64 binary for the VM or QEMU.

## Flow

```
Vec<RvInstruction>
  -> Parse     RvInstruction -> internal AsmToken stream
  -> Layout    assign section offsets, resolve label addresses
  -> Encode    emit machine bytes, record relocations
  -> AssembledOutput   sections (.text/.data/.rodata/.bss) + symbol table + relocations

AssembledOutput x N
  -> ObjectLinker::link   resolve cross-object symbols, apply relocations
  -> to_elf               single PT_LOAD ELF-64 image
```

## Public API

```rust
use asm_to_binary::{Assembler, AssembledOutput, ObjectLinker, RvInstruction};

// Assemble one object.
let obj: AssembledOutput = Assembler::assemble(&tokens)?;

// Link several objects (each tagged with a module name).
let linked = ObjectLinker::link(&[("user", &obj), ("stdlib", &stdlib_obj)])?;

// Export an ELF loaded at the given base address.
let elf: Vec<u8> = linked.to_elf(0x8000_0000);
```

`AssembledOutput` holds the section data, a `SymbolTable` of global/local labels, and a
list of `RelocationRecord`s. `RvInstruction` is defined here and re-exported so
`ir-to-asm` and this crate share a single instruction definition.

## Instruction model

```rust
pub enum RvInstruction {
    Real(RealInstruction),       // encodable RV64IMAFD instructions
    Pseudo(PseudoInstruction),   // expanded during assembly (li, call, la, ...)
    Label(String),               // symbol definition
    Directive(Directive),        // .text, .data, .word, .align, ...
}
```

## Module layout

```
src/
  assembler/   parser, layout, encoder, sections, symbol table, relocations
  riscv/       per-extension instruction encoders (rv64i/m/a/fd/zicsr)
```

Instruction encode/decode, pseudo and real instruction definitions, and the object
linker sit alongside these as sibling modules.
