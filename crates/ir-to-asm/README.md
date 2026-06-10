# ir-to-asm

**Stage 3 of the Full-Stack compiler pipeline.**

Back end that lowers a typed SSA `IrProgram` into RV64IMAFD assembly. It performs
register allocation, lays out each function's stack frame, selects machine
instructions, and emits the `.text`, `.data`, and `.rodata` sections.

## Flow

```
IrProgram
  -> Stack-slot planning  every IR value gets a frame slot (slot coloring shares them)
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

## Register allocation and stack slots

The code generator keeps every IR virtual register in a stack slot; it does not
hand out physical registers for values. A naive one-slot-per-register layout
makes frames grow without bound, wasting stack and pushing load/store offsets
past the RISC-V immediate range on large functions.

`slot_coloring` shrinks frames by letting registers whose live ranges never
overlap share a slot. It runs live-variable analysis over the real control-flow
graph (so loops are handled correctly), builds an interference graph among the
scalar value registers, and colors it greedily; each color becomes one 8-byte
slot. Function parameters are treated as simultaneous defs at the entry block so
they never collapse together. Registers that escape (`Alloc` destinations and
stack addresses) or need more than 8 bytes (aggregates, arrays) keep a dedicated
slot.

There is no physical register allocator yet: every value lives in a slot and is
reloaded on each use. A linear-scan allocator that keeps hot values in registers
across their live range (reusing `slot_coloring`'s liveness) is the planned next
step (PLAN 1.1).

## Module layout

```
src/
  compiler/   register allocation, frame and function contexts, data section,
              instruction selection, and the assembly emitter
```
