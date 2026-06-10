# ir-to-asm

**Stage 3 of the Full-Stack compiler pipeline.**

Back end that lowers a typed SSA `IrProgram` into RV64IMAFD assembly. It performs
register allocation, lays out each function's stack frame, selects machine
instructions, and emits the `.text`, `.data`, and `.rodata` sections.

## Flow

```
IrProgram
  -> Register allocation  hot scalar values -> callee-saved registers
  -> Stack-slot planning  remaining IR values get frame slots (slot coloring shares them)
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

`register_allocator` (PLAN 1.1) keeps hot scalar values in registers instead of
memory. It reuses `slot_coloring`'s interference analysis and greedily colors
the highest-use-count integer/pointer values onto the callee-saved registers
s2-s11 (s0 is the frame pointer, s1 the sret pointer); values that do not fit
fall back to slots. Callee-saved registers survive calls and the kernel trap
entry saves x18-x27, so assigned values need no call-crossing analysis -- the
prologue/epilogue saves exactly the registers used. Results written to a
register are sign-extended to their IR width so they match what a typed slot
store/reload would produce. Excluded from allocation: floats (FP file),
aggregates, address-taken values, the hidden sret parameter, and any function
containing inline asm (which may clobber arbitrary registers). A tight integer
loop runs roughly 2x faster with allocation on
(`regalloc_hot_loop_is_substantially_faster`).

Allocation is on by default in the application's `CompilationPipeline` and off
on a raw `CompilerRv64::new()`; both expose `set_register_allocation`.

Values left in memory go through stack slots: `slot_coloring` shrinks frames by
letting registers whose live ranges never overlap share a slot. It runs
live-variable analysis over the real control-flow graph (so loops are handled
correctly), builds an interference graph among the scalar value registers, and
colors it greedily; each color becomes one 8-byte slot. Function parameters are
treated as simultaneous defs at the entry block so they never collapse
together. Registers that escape (`Alloc` destinations and stack addresses) or
need more than 8 bytes (aggregates, arrays) keep a dedicated slot.

## Module layout

```
src/
  compiler/   register allocation, frame and function contexts, data section,
              instruction selection, and the assembly emitter
```
