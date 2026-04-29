# Compiler Project Backlog

## Aggregate Types & Constant Evaluation
- [ ] **Spec Alignment:** Reconcile `IrType::Aggregate` design—decide whether to strip or retain field names and synchronize the IR specification with the codebase.
- [ ] **Const Eval (Struct Access):** Enable compile-time field access on struct literals.
- [ ] **Const Eval (Struct Literals):** Fully support struct literals within constant evaluation contexts.

---

## Control Flow & Code Quality
- [ ] **Defer Semantics:** Fix `defer` handling to guarantee non-call expressions are evaluated strictly once at the site of declaration, rather than re-evaluated at block exit.
- [ ] **IR Cleanup:** Purge deprecated comparison opcodes from the IR specification and implementation.
- [ ] **Monomorphization:** Verify and solidify generic monomorphization logic for functions (ensuring namespace collision safety).