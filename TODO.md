# Compiler Project Backlog

---

## 1. Critical Backend Fixes & Memory Safety (Highest Priority)
- [x] **Struct Return Memory Bug:** Implemented `sret` (Hidden Pointer) pattern in `compiler_rv64.rs`. Functions returning aggregates now receive a hidden pointer parameter (passed in `a0`, saved in callee-saved register `s1`). The caller allocates memory and passes the pointer, and the callee copies aggregate data to that location instead of returning it in registers. This ensures memory safety and follows proper RISC-V calling conventions.
- [x] **Strict Scalar Type Checking:** Added targeted type validation in operations that require scalar values (`Math`, `Unary`, `Cmp`, `Cast`). These operations now explicitly panic with clear error messages when attempting to operate on `Aggregate` or `Array` types. The check is applied at the point of use rather than during value loading, allowing legitimate uses like pointer loads while preventing invalid operations like `array + array` or casting aggregates.
- [x] **Optimize Memory Copies:** Refactored `copy_bytes_from_addr_to_slot`, `copy_bytes_from_slot_to_addr`, and added new `copy_bytes_from_addr_to_addr` helper. All three functions now use chunked copying strategy: 8-byte chunks via `ld`/`sd`, 4-byte chunks via `lw`/`sw`, then remaining bytes individually. This provides 4-8x performance improvement for typical struct/array sizes (8-64 bytes).
- [x] **Struct Return Optimization:** Implemented hybrid struct return strategy. Small structs (≤16 bytes) are now returned directly in registers `a0/a1` according to RISC-V ABI, while larger structs continue to use the sret pattern. This eliminates unnecessary memory copies for small structs and improves performance.

---

## 2. Backend Implementation (IR -> RISC-V Assembly)
- [x] Implement main IR to assembly translation (`compiler_rv64.rs`).
- [x] Map virtual registers to physical/stack slots (`register_allocator.rs`).
- [x] Define stack frame layout and ABI conventions (`frame_context.rs`).
- [x] Track per-function bookkeeping and local state (`function_context.rs`).
- [x] Expand IR pseudo-operations to real RISC-V instructions (`pseudo_instructions.rs`).
- [x] **ABI Compliance:** Implemented RISC-V LP64 ABI struct packing for small structs (≤ 16 bytes) to be returned directly in `a0/a1` registers. Large structs (>16 bytes) use the sret pattern.
- [x] **Assembly Golden Tests:** Added golden test suite for assembly output (.s files) with automated generation via `UPDATE_ASM_GOLDENS=1`. Includes IDE run configurations for easy regeneration.

---

## 3. Frontend Lowering & Semantic Mapping
- [ ] **Type Casting:** Implement cast expressions (`target_type(value)`) end-to-end (AST node, Semantic Analysis, IR lowering).
- [ ] **Signed/Unsigned Operations:** Ensure the frontend correctly lowers to unsigned comparison opcodes (`ult`, `ule`, `ugt`, `uge`) and unsigned division (`div`) for `u8..u64` types.
- [ ] **Memory Management:** Detect the `free` built-in and correctly lower it to the `heap_free` IR instruction.
- [ ] **Void Calls:** Ensure `void` function calls omit destination register assignments in the IR.
- [ ] **Array Literals:** Implement IR lowering for array literal AST nodes.

---

## 4. Aggregate Types & Constant Evaluation
- [ ] **Spec Alignment:** Reconcile `IrType::Aggregate` design—decide whether to strip or retain field names and synchronize the IR specification with the codebase.
- [ ] **Const Eval (Struct Access):** Enable compile-time field access on struct literals.
- [ ] **Const Eval (Struct Literals):** Fully support struct literals within constant evaluation contexts.

---

## 5. Control Flow & Code Quality
- [ ] **Defer Semantics:** Fix `defer` handling to guarantee non-call expressions are evaluated strictly once at the site of declaration, rather than re-evaluated at block exit.
- [ ] **IR Cleanup:** Purge deprecated comparison opcodes from the IR specification and implementation.
- [ ] **Monomorphization:** Verify and solidify generic monomorphization logic for functions (ensuring namespace collision safety).