# TODO.md

---

## 1. Backend (IR -> RISC-V Assembly) – HIGHEST PRIORITY

All files in `src/2_intermediate_language/asm_compiler/` are empty.

- [x] Implement `compiler_rv64.rs` – main IR to assembly translation
- [x] Implement `register_allocator.rs` – map infinite virtual registers to physical registers
- [x] Implement `frame_context.rs` – stack frame layout (locals, spill slots, return address)
- [x] Implement `function_context.rs` – per‑function bookkeeping
- [x] Implement `pseudo_instructions.rs` – expand IR pseudo‑ops (e.g., stack_alloc) to real RISC-V insts

---

## 2. Frontend / Semantic Gaps

- [ ] Implement cast expressions (target_type(value) syntax) – AST node, parser, sema, IR lowering
- [ ] Detect `free` built‑in and lower to `heap_free` IR instruction
- [ ] Lower pointer arithmetic `ptr + int` to `offset` instruction (byte‑scaled), not `math add`
- [ ] Use unsigned compare opcodes (`ult`, `ule`, `ugt`, `uge`) for unsigned integer types (u8..u64)
- [ ] Emit no destination register for `void` function calls
- [ ] Lower array literals if needed (AST node exists but no lowering)

---

## 3. Aggregate Types & Returns

- [ ] Support struct return values – use `sret` (pointer to struct) or multiple return registers (current IR assumes only scalars)
- [ ] Decide whether to keep field names in `IrType::Aggregate` (spec says no, code says yes) – update either spec or implementation

---

## 4. Constant Evaluation (Compile‑Time)

- [ ] Support field access on struct literals in const evaluation
- [ ] Support struct literals themselves in const evaluation
- [ ] Add recursion depth limit (present but fine)

---

## 5. Defer Handling

- [ ] Ensure non‑call expressions in `defer` are evaluated only once (currently re‑lowered at exit)

---

## 6. UI / Visualisation

- [x] Implement `AssemblyLanguageView` to show generated assembly
- [x] Implement `IntermediateLanguageView` to show IR side‑by‑side

---

## 7. Cleanup / Consistency

- [ ] Unify signed/unsigned handling: IR has both `div`/`sdiv` and signed/unsigned cmp – ensure frontend uses the right variant
- [ ] Remove deprecated comparison opcodes from IR if they are never emitted
- [ ] Verify that generic monomorphisation for functions is implemented if generic functions are added later