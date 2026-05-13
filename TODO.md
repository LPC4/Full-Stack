# TODO

## Remove registry seeding; make heap alloc/free intrinsic (no injected externs)

Currently the app compiles stdlib once, extracts a FunctionRegistry + TypeRegistry from the
resulting IR, then injects them into the semantic analyzer and IR compiler via special seeding
methods. This is not clean.

**New strategy (clean + extensible):** `new(T[, count])` and `free(ptr)` are *compiler intrinsics*.
They lower directly to IR heap instructions (`heap_alloc`, `heap_free`) and are type-checked without
any source-level extern declarations. The backend emits calls to the runtime allocator symbols
(e.g. `malloc`/`free`) as an *implementation detail*. This keeps the language surface clean while
making the allocator pluggable at link time.

**Linking model:** the runtime must still provide the allocator symbols that codegen emits.
These come from the runtime/stdlib object code (or any alternative allocator you link). They are
*not* injected into user source as extern declarations, and no stdlib types are auto-injected.
Other library APIs (`printf`, string utils, etc.) must be explicitly declared via `external` by
user code, as normal.

---

### Step 1 -- make alloc/free intrinsic (no extern injection)

    src/1_high_level_language/compiler/utility/semantic_analyzer.rs
      - stop seeding externs for malloc/free
      - ensure `new(...)` and `free(...)` type-check intrinsically

    src/1_high_level_language/compiler/compiler/expressions.rs
      - keep lowering `new(...)` to IrInstruction::HeapAlloc

    src/1_high_level_language/compiler/compiler/control_flow.rs
      - ensure `free(ptr)` lowers to IrInstruction::HeapFree via the normal call path
        (or add a dedicated lowering if it currently relies on extern signatures)

    src/2_intermediate_language/asm_compiler/compiler_rv64.rs
      - keep lowering heap_alloc/free to `call malloc` / `call free`
      - (optional) centralize allocator symbol names in one place for future swapping

---

### Step 2 -- remove unused registry infrastructure

    src/1_high_level_language/stdlib.rs
      - remove: FunctionSignature, FunctionRegistry, TypeRegistry, extract_registries()
      - remove: prepend_stdlib() (no more auto-injected types/functions)
      - keep:   get_stdlib_source() only if examples/tests still reference it

    src/1_high_level_language/compilation_pipeline.rs
      - remove: compile_with_externs(), semantic_analysis_with_externs(), compile_to_ir_with_externs()

    src/1_high_level_language/compiler/compiler.rs
      - remove: compile_program_with_externs()
      - remove: extern_fn_returns, extern_ty_aliases fields

    src/1_high_level_language/compiler/utility/semantic_analyzer.rs
      - remove: seed_extern_fn_returns(), seed_extern_type_aliases()

    app.rs
      - remove: stdlib_fn_registry, stdlib_ty_registry fields
      - drop all registry loading/extraction code
      - stdlib_tokens cache stays (still needed for token-level linking)

    tests/vm_diag_test.rs
      - remove: extract_registries() calls
      - run_gui_path: use normal compile() (no extern injection)

---

### Order

    Step 1 (all parallel)  ->  Step 2 (all parallel)

After Step 1, tests should pass as long as the runtime allocator symbols are still linked.
After Step 2, no registry seeding remains.
