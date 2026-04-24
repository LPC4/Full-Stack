# TODO.md

Prioritized backlog for the compiler, specs, and project hygiene.

## HLL spec — structural issues

- [x] ~~[P0] Document the canonical struct type syntax as `type X = { ... }` everywhere in the HLL spec.~~
- [x] ~~[P0] Document inline struct literals with shorthand fields, e.g. `{ .field = expr }`, alongside the explicit typed form if both remain supported.~~
- [x] ~~[P0] Clarify that commas are required between fields in struct type definitions.~~
- [x] ~~[P0] Clarify destructuring semantics for named fields, partial destructuring, and reordered fields.~~
- [x] ~~[P1] Clarify the exact `&` rules in the spec: allow `&identifier` and stack-safe lvalues like `&arr[index]`, reject `&@ptr`.~~
- [x] ~~[P1] Remove or rename remaining “tuple” terminology in HLL spec text so it consistently says “struct destructuring” / “inline struct”.~~
- [x] ~~[P2] Review precedence examples and grammar notes so they do not rely on ambiguous parsing the spec intends to reject.~~

## HLL implementation / compilation issues

- [x] ~~Fix the duplicate parser test symbol in `src/1_high_level_language/parser.rs` so the test suite builds again.~~
- [x] ~~Fix struct destructuring lowering in `src/1_high_level_language/compiler/expressions.rs` so fields are matched by name, not by position.~~
- [x] ~~Implement precise address-of handling in the compiler: support valid lvalues such as `&arr[index]` and reject `&@ptr` with a targeted diagnostic.~~
- [x] ~~Keep generic-placeholder arithmetic permissive for now, but isolate it so real named types still fail where appropriate.~~
- [x] ~~Audit `src/1_high_level_language/compiler/utility/semantic_analyzer.rs` for any other places where `@`, field access, or indexing produce the wrong rvalue type.~~
- [x] ~~Rename or descope legacy tuple-related code paths in `ast.rs`, `parser.rs`, and `compiler/assignments.rs` so the implementation terminology matches the spec.~~
- [x] ~~Add negative tests for the v1.4.1 invariants: `&@ptr`, returning stack addresses, ambiguous precedence rejection, and invalid pointer arithmetic.~~
- [x] ~~Expand fixture coverage for destructuring order/partial binding and pointer-heavy flows.~~

## IR spec — structural issues

- [P1] Audit the IR spec for consistency with the current aggregate-format output used by the compiler.
- [P1] Ensure the IR spec examples match the current named-aggregate formatting used by `Display` implementations.
- [P2] Check whether any IR wording still implies tuple-style terminology instead of aggregate/struct terminology.
- [P2] Add a short source-to-IR mapping note for aggregate types and pointer-rich examples so the spec matches current compiler behavior.
- [P1] Decide whether IR aggregate types are canonical with field names or anonymous only: `IrType::Display` currently drops names, but `TypeContext::get_type_name` and aggregate parsing preserve them.
- [P1] Reconcile the spec’s `type Point = {f32, f32}`-style examples with the compiler’s named-field aggregate representation (`{ x: f32, y: f32 }`) so HLL and IR use one consistent story.
- [P2] Clarify whether anonymous inline structs are actually forbidden in IR, because the frontend lowers HLL inline structs into aggregates even when they are not introduced by top-level IR `type` aliases.
- [P2] Review the IR grammar/examples for any other spots where the textual syntax (`@label`, `@ func`, aggregate printing, `call`/`branch` forms) diverges from the current pretty-printer output.

## IR implementation / compilation issues

- [P1] Verify that IR lowering for aggregates, pointers, and field access still matches the current HLL semantics after the HLL fixes.
- [P1] Re-check any IR emission paths that depend on destructuring order, pointer depth, or field offsets.
- [P2] Add regression coverage for IR output on the updated showcase program once the HLL side is fully stable.
- [P2] Keep IR snapshot output deterministic across platforms.

## Other project issues

- [x] ~~Update the fixture/test process so missing golden `.ir` files do not auto-write in CI mode.~~
- [P0] Add Rust tests that actually execute the integration `.hll` programs under `programs/test/integration/`.
- [x] ~~Separate bootstrap snapshot generation from normal test runs so regressions cannot be silently blessed.~~
- [x] ~~Clean up any outdated fixture syntax still drifting from the current HLL grammar.~~
- [P2] Reduce warning noise from dead legacy code paths once the new semantics are settled.

## Suggested order of work

1. P0 build blockers and semantic correctness issues.
2. HLL spec/implementation alignment.
3. IR spec/implementation alignment.
4. Test coverage and process hardening.
5. Legacy cleanup and terminology normalization.
