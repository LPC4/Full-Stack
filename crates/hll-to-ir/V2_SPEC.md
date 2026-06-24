# HLL V2 -- specification audit and redesign

This document drives the language V2 effort. It is written in two phases:

1. **Phase 1 (this commit): the audit.** Catalogue every problem with the current
   `_LANG_SPECIFICATIONS.md` -- internal contradictions, examples the grammar forbids,
   features the spec documents but the front end does not implement, and syntax that is
   simply too verbose. No fixes yet.
2. **Phase 2 (later): the redesign.** For each issue, the agreed V2 syntax/semantics, the
   new grammar, and the lowering. Then `_LANG_SPECIFICATIONS.md` is rewritten from this.

Until Phase 2 lands, `_LANG_SPECIFICATIONS.md` remains the source of truth for the
*implemented* language. Nothing here changes the compiler yet.

All line references below are into `crates/hll-to-ir/_LANG_SPECIFICATIONS.md` unless noted.

---

## How to read the audit

Each issue has an ID, a severity, the evidence, why it is a problem, and a one-line
pointer at the intended V2 direction (detail comes in Phase 2). Categories:

- **R -- reality gap:** the spec documents something the front end does not implement, or
  contradicts verified behavior. A reader cannot tell spec from aspiration.
- **C -- internal inconsistency:** the spec contradicts itself (prose vs. grammar, or one
  example vs. another).
- **V -- verbosity / ergonomics:** legal and implemented, but heavier than it should be.
- **H -- design hazard:** a foot-gun baked into the design (also tracked in GOTCHAS.md).

| ID | Severity | One-line |
|----|----------|----------|
| R1 | high | Function-decl syntax has 3 incompatible forms; only one is real |
| R2 | high | Generics (`<T>`) pervade the stdlib but are not in the grammar or front end |
| R3 | high | `{ field: value }` struct literal is used everywhere but the grammar forbids it |
| R4 | med  | `@arr[i].field` is shown but rejected by the strict pipeline |
| R5 | med  | `external` is the real cross-module mechanism but is absent from the grammar |
| R6 | low  | `print` is called throughout but defined nowhere |
| R7 | low  | Type-inferred declarations (`x = expr`) appear but are never specified |
| R8 | med  | Stdlib/FFI/optimizer sections describe unbuilt features as if they exist |
| C1 | high | "Significant newlines" (S2) reads as Python; blocks are actually brace-delimited |
| C2 | med  | `-> ()` is deprecated by S6.1 yet used in most later examples |
| C3 | med  | Struct-literal forms (`.f = v` vs `f: T = v` vs `f: v`) mixed within one section |
| V1 | high | Multiple return / destructuring forces re-typing every field on the LHS |
| V2 | high | Error handling has two competing conventions, both boilerplate-heavy |
| V3 | med  | Naming a struct needs `type X = { ... }`; no `struct`/`class` declaration |
| H1 | crit | `ptr + i` is byte-scaled but `arr[i]` is element-scaled |
| H2 | med  | `as` binds tighter than unary `@`/`&` and than `+`, forcing parens everywhere |

---

## R -- reality gaps

### R1. Three incompatible function-declaration syntaxes (high)

The grammar (S9.2) and the real front end use one form:
`function_decl = identifier ":" "(" [param_list] ")" [ "->" return_type ] block`, e.g.
`increment: (x_ptr: i32*) -> () { }` (S6.2:245) and `shell.hll:49`. But the spec also uses:

- **C-style** `name(params): ret { }` -- `compute_factorial(n: i32): i32 { }` (S8.1:379),
  `open_file(path: Str*): { ... } { }` (S8.2:393). Not in the grammar, not implemented.
- **Generic** `name: <T>(params) -> ret { }` -- `new_vector: <T>(...) { }` (S11.2:579). Not
  in the grammar (see R2).

So S8 is written in a syntax that contradicts both the grammar and every other section.
**V2 direction:** pick exactly one declaration form and use it in every example.

### R2. Generics are documented but do not exist (high)

S11.2/S11.3 are written around `type Vector<T> = { ... }`, `new_vector: <T>(...)`,
`new(Vector<T>)`, `new(T, n)` with a type parameter, `push: <T>(...)`. The grammar S9.2 has
**no** type-parameter production anywhere, and the front end has no generics. The single
most load-bearing stdlib type (`Vector`) is specified in a language the compiler cannot
parse. **V2 direction:** generics are a headline V2 feature (monomorphized); add the
grammar and re-author the stdlib against it.

### R3. The `{ field: value }` struct literal is ubiquitous but ungrammatical (high)

The grammar allows only two field-init forms (S9.2:473-476): shorthand `.field = expr` and
typed `field: Type = expr`. Yet examples repeatedly use a third, untyped `field: value`
form the grammar forbids:

- `@points[0] = { x: 1.0, y: 2.0 }` (S5.1:142)
- `return { remainder: a % b, quotient: a / b }` (S6.2:255)
- `@str_ptr = { data: data, length: length }` (S11.1:565)

This is the most natural-looking literal in the doc and it is not in the language.
**V2 direction:** make `.field = expr` the single field-init form (so `:` always introduces a
type), used both for named literals `Point { .x = 1, .y = 2 }` and anonymous literals
`{ .x = 1, .y = 2 }`. See D4 for the resolved form.

### R4. `@arr[i].field` is shown but rejected (med)

S5.1:143 (`x_val: f32 = @points[0].x`) and the array-of-structs examples imply
`@arr[i].field` works. Verified project knowledge (the array-of-structs refactor) is that
the strict pipeline **rejects** it; the working idiom is `p: T* = arr[i]` then `@p.field`.
The spec documents a construct that does not compile. **V2 direction:** indexing produces
an assignable element place, so the canonical spelling is `arr[i].field`; use `&arr[i]`
when the element's address is required (D2).

### R5. `external` is real but missing from the grammar (med)

`external name: (params) -> ret` is the actual cross-module mechanism (`shell.hll:4-43`,
S12.3:641), and the whole split-TU userspace depends on it. The grammar's `declaration`
list (S9.2:430) names only `variable/function/type/const/import/export` -- no
`external_decl`. The module system section (S6.2) documents `import`/`export` but never
`external`. **V2 direction:** add `external` to the grammar and the module-system section.

### R6. `print` is undefined (low)

`print(coords.x)` and friends are called in S5.3/S6.2 (lines 198, 199, 204, 261) but no
`print` exists in the stdlib reference (S11) -- real code uses `console_write`/`putc`.
Examples lean on an imaginary builtin. **V2 direction:** define the I/O surface used by
examples, or switch examples to the real one.

### R7. Type-inferred declarations are unspecified (low)

`coords = get_coordinates()` (S5.3:197) and `s = divide(10, 3)` (S6.2:260) declare new
variables with no `: Type`. The grammar makes the type optional
(`variable_decl = identifier [":" type] [ "=" expression ]`, S9.2:434), but the prose never
describes inference, and S3.2 insists on `name: Type = value`. Worse, `x = expr` is
ambiguous between "declare new" and "assign existing". **V2 direction:** decide whether
inference exists; if so, specify it and disambiguate declaration from assignment.

### R8. Aspirational sections read as implemented (med)

S11 (generic Vector, arenas, pools), S12 (full C ABI/FFI), and S13 (escape analysis,
SIMD vectorization, debug symbols) are written in the present tense as language facts, but
are largely unbuilt. Nothing marks spec vs. roadmap. **V2 direction:** tag every section
implemented / planned, and move the unbuilt parts behind that tag.

---

## C -- internal inconsistencies

### C1. "Significant newlines" misdescribes the block model (high)

S2:31 says "Statement termination: significant newlines (one statement per line)", which
reads like Python indentation. In reality blocks are **brace-delimited** (`block = "{" {
statement } "}"`, S9.2:448; confirmed in `shell.hll`), and a newline merely terminates a
statement *inside* a brace block (with multi-line continuation, S2.1:43). The prose never
states blocks use `{ }`; a reader infers indentation. **V2 direction:** state the block
model explicitly (and decide V2's stance: keep braces or go brace-light -- this also
affects how the V2 OOP/`match` examples should be written).

### C2. `-> ()` deprecated, then used everywhere (med)

S6.1:224 says prefer `name: () { }` and calls `-> ()` deprecated. Later examples use `-> ()`
constantly: `main: () -> ()` (S6.2:249, 258), `push: <T>(...) -> ()` (S11.2:587),
`free_vector: ... -> ()` (S11.2:593), `close_file: (f: File*) -> ()` (S11.4:612). The spec
contradicts its own recommendation. **V2 direction:** one void form, applied uniformly.

### C3. Struct-literal forms mixed within a single section (med)

Even inside S5 the literal syntax flips between `{ .x = 1.0, .y = 2.0 }` (S5.2:162, 166)
and `{ x: 1.0, y: 2.0 }` (S5.1:142). Combined with R3 this is three spellings for one
concept. **V2 direction:** one canonical literal.

---

## V -- verbosity / ergonomics

### V1. Multiple return and destructuring are heavyweight (high)

There are no tuples (intentionally -- structs cover grouping), so multi-value return goes
through inline structs, and unpacking re-types every field on the LHS:
`{ remainder: i32, quotient: i32 } = divide(10, 3)` (S6.2:264). The reader writes each name
and its type again just to bind two values. **V2 direction:** lighter binding for a
returned struct (positional or name-only destructuring without repeating types), keeping
structs as the grouping mechanism.

### V2. Error handling: two conventions, both boilerplate (high)

S8.2 specifies errors as `{ value: T, error: E }` structs **and** `null` for
pointer-returning functions -- two competing idioms. Consuming either needs a destructure
plus a manual `if x == null` / field check at every call site (S8.2:404-408). This is the
classic case for sum types. **V2 direction:** `Option<T>` / `Result<T, E>` (built on V2
generics + sum types) with a `?` propagation operator that lowers to a visible early
return.

### V3. Declaring a named struct requires `type X = { ... }` (med)

A struct type is introduced as a type alias to an anonymous struct (`type Point = { x:
f32, y: f32 }`, S5.2:157). There is no `struct`/`class` keyword, which also leaves nowhere
natural to attach methods. **V2 direction:** first-class `struct`/`class` declarations that
can carry methods (the OOP track: interfaces + classes, static dispatch by default, opt-in
`dyn` for polymorphism).

---

## H -- design hazards

### H1. Byte-scaled `ptr + i` vs element-scaled `arr[i]` (critical)

S4.2:119 and S5.1:150: raw `ptr + offset` advances by *bytes*, but `arr[i]` scales by
`sizeof(T)`. Mixing them silently overlaps records (the documented job-table corruption,
GOTCHAS CRITICAL). The two most common ways to reach element `i` disagree on what `i`
means. **V2 direction:** slices `T[]` (a bounds-checked, element-scaled `{ptr, len}` fat
pointer) as the default sequence type, demoting raw byte arithmetic to an explicit,
rarely-needed escape hatch.

### H2. `as` precedence is surprising (med)

`as` is a level-1 postfix operator (S9.3:486), so it binds tighter than unary `@`/`&`
(level 2) and than `+` (level 4): `@x as T` is `@(x as T)` (S9.3:508) and `a + b as u32*`
casts only `b` (GOTCHAS MEDIUM). Correct code needs defensive parentheses everywhere a
cast meets a deref or arithmetic. **V2 direction:** reconsider `as` precedence (or
require parenthesized cast targets) so the common reading is the correct one.

---

## Out of scope for the audit (already correct)

For the record, these are consistent and need no change: the four golden rules (S1.1),
pointer core operations `@`/`&`/`new`/`free` (S4.1), compound assignment (S9.4), the defer
capture-at-declaration semantics (S7.2), and the HLL-0 subset (Appendix D, which matches
`cc`).

---

## Phase 2 -- fixes

Each issue from the audit gets a concrete V2 decision below. Several issues share a root
cause, so four **foundational decisions (D1-D4)** come first; the per-issue fixes reference
them. All examples are written in V2 syntax (brace blocks, `;` comments).

Severity of breakage to existing `.hll`: most fixes are additive or example-only. D2 is a
deliberate semantic change: indexing now yields an element place instead of `T*`, ordinary
`@arr[i]` reads become `arr[i]`, and code that needs an element pointer uses `&arr[i]`.
Existing `@p.field` must be migrated according to intent because it now means `@(p.field)`,
not `(@p).field`. H2 changes cast precedence and removes previously required parentheses.
A one-time `.hll` migration pass handles these rewrites mechanically where type information
makes the intended operation unambiguous.

### Foundational decisions

#### D1. Block and statement model (fixes C1)

Blocks are **brace-delimited**, period. Keep braces -- the entire compiler and every `.hll`
source already use them, and indentation-significance would be a gratuitous breaking change.
The spec states it plainly:

- A block is `"{" { statement } "}"`. Indentation is insignificant.
- A newline terminates a statement. `;` is a comment, never a separator, so there is no
  statement-terminator token.
- A statement continues onto the next line when the current line cannot yet form a complete
  statement -- it ends with a binary operator, a comma, or an open `(`/`[`/`{`. This is the
  only continuation rule (replaces the vague S2 "significant newlines" wording).

"Significant newlines (one statement per line)" in S2 is deleted and replaced by the above.

#### D2. Places, indexing, member access, and explicit dereference (fixes R4)

V2 distinguishes a **place** from the value stored at that place. A local variable, a
struct field, and an indexed array or slice element are places. A place is read when used
in a value expression and written when used on the left-hand side of assignment. Taking
`&place` produces a pointer to it.

Indexing no longer returns a pointer. For an array or slice containing `T`, `seq[i]` is an
assignable place of type `T`; `&seq[i]` is its `T*` address. This removes `@` from ordinary
array access. Array and slice indexing remain element-scaled, and slice indexing remains
bounds-checked.

The `.` operator works on both `T` and `T*`. On `T*` it auto-dereferences exactly one
level to select a field or method of the pointee. `@` is retained only for reading or
writing a pointer's **whole pointee value**:

```hll
p: Point* = new(Point)
p.x = 5.0            ; writes a field through p
v: f32 = p.x         ; reads pointee field
whole: Point = @p    ; @ still required to read the whole struct value
@p = other           ; replaces the whole struct value

first: Point* = &pts[0]
first.x = 1.0        ; field access auto-dereferences first
pts[0].x = 1.0       ; indexing produces a Point place, then selects x
element: Point = pts[0]
element_ptr: Point* = &pts[0]
```

`@p.field` parses as `@(p.field)`: it dereferences a pointer stored in `field`; it does
**not** mean `(@p).field`. The latter spelling is valid but redundant because `p.field`
already performs the one permitted member auto-dereference.

This rule keeps routine aggregate access concise while preserving `@` as a visible marker
for raw, whole-pointee access. No other operator implicitly dereferences a pointer.

#### D3. Three distinct binding forms (fixes R7)

Declaration and assignment are syntactically separated so `x = e` is never ambiguous:

| Form | Meaning |
|------|---------|
| `name: T = expr` | declare `name` with explicit type `T` |
| `name := expr` | declare `name`, infer its type from `expr` |
| `name = expr` | assign to an existing `name` (never declares) |

Reading an undeclared name on the LHS of `=` is a compile error (catches typos). Inference
(`:=`) never crosses a function boundary or needs unification beyond the RHS type.

#### D4. One struct-literal form (fixes R3, C3)

A struct literal is `.field = expr` pairs in braces, optionally prefixed by the type name:

```hll
p := Point { .x = 1.0, .y = 2.0 } ; named -- type is explicit
q: Point = { .x = 1.0, .y = 2.0 } ; anonymous -- type inferred from the annotation
@ptr = { .x = 3.0, .y = 4.0 }     ; anonymous -- type inferred from the lvalue
```

`field_init = "." identifier "=" expression`. Field initialization uses `.field = expr` so
that `:` always introduces a *type* (declarations, parameters, struct field types) and never a
value. The `field: expr` and the `field: Type = expr` typed-init forms are **removed**; field
order is free, all fields must be present for a named literal, and missing fields in an
anonymous literal default to zero.

---

### Per-issue fixes

#### R1. One function-declaration syntax

Canonical form, everywhere:

```hll
name: (p0: T0, p1: T1) -> Ret { ... }   ; value-returning
name: (p0: T0) { ... }                  ; void -- omit -> entirely (see C2)
name: <T>(x: T) -> T { ... }            ; generic (see R2)
```

`function_decl = identifier ":" [ type_params ] "(" [ param_list ] ")" [ "->" type ] block`.
The C-style `name(params): ret { }` is deleted from the spec. S8's examples are rewritten in
this form.

#### R2. Generics (monomorphized)

Type parameters on functions, structs/classes, and interfaces; instantiated by
**monomorphization** (one compiled copy per concrete type set, name-mangled
`push__Vector_i32`). No boxing, no runtime type info -- zero per-call cost; the cost is
binary size.

```hll
struct Pair<T> { first: T, second: T }

max: <T>(a: T, b: T) -> T {
    if a > b { return a }
    return b
}

class Vec<T> : Seq<T> {
    data: T*
    len:  u64
    cap:  u64

    push: (self, value: T) {           ; `self` sugar = self: Vec<T>*  (see V3)
        if self.len >= self.cap { self.grow() }
        self.data[self.len] = value    ; element-scaled (slice/array indexing)
        self.len += 1
    }
}
```

Grammar additions: `type_params = "<" ident [ ":" bound ] { "," ident [ ":" bound ] } ">"`;
`bound = ident { "+" ident }` (interface bounds, see V3). `type`/`new`/calls accept
`name "<" type { "," type } ">"`. Bounds are checked at the call site, then erased.

Implementation status (2026-06-21): explicit generic function calls are monomorphized before
semantic analysis and IR lowering. Generic records support nested and recursive concrete
specializations and compose with generic functions. Call-site inference handles structurally
deducible literal, local, pointer, array, slice, and named-type arguments; unconstrained arguments
remain explicit. Recursive specialization that does not converge (a generic instantiated at an
unbounded type sequence) is diagnosed by a bounded specialization loop rather than hanging.

Bounds are deferred to V2.1: the V2 core has no interface system, so generics are unconstrained and
a specialization that uses an unsupported operation fails at the specialized site with an ordinary
type error. Specialization names are module-independent (`name__type`), so equal instantiations in
separate units share a symbol; linker deduplication is verified under Milestone 8.

#### R5. `external` in the grammar

Add to the declaration list and document it in the module-system section as the third
cross-module mechanism alongside `import`/`export`:

```ebnf
declaration  = ... | external_decl ;
external_decl = "external" identifier ":" "(" [ param_list ] ")" [ "->" type ] ;
```

An `external` names a symbol defined in another translation unit, resolved at link time;
unlike `import` it pulls in no module, and unlike `export` it declares no body.

#### R6. Define the I/O surface used by examples

Two stdlib entry points, used by every example (no more imaginary `print`):

```hll
print:   (s: Str)        ; write s.data[0..s.length] to fd 1, no newline
println: (s: Str)        ; print + '\n'
```

Both lower to a `write` ecall over the `Str` bytes -- no allocation. With interpolation
(V2/string-interp) this covers all example output. `putc` stays as the HLL-0 primitive.

#### R8. Mark implemented vs planned

Editorial policy applied to the rewritten `_LANG_SPECIFICATIONS.md`: every section header
carries a tag -- `[impl]` for shipped behavior, `[v2]` for newly added in this redesign.
Sections describing unbuilt machinery (SIMD vectorization, full escape-analysis promotion,
debug-symbol generation in S13; arenas/pools in S11.3) move under a clearly labelled
"Roadmap / non-normative" appendix so they cannot be mistaken for current behavior.

#### C2. One void form

`-> ()` is **removed** (not merely deprecated). A void function omits the arrow:
`name: (params) { ... }`. All examples updated; `return` with no expression still ends a
void function early.

#### V1. Lightweight destructuring (no tuples)

Structs stay the only grouping type, but unpacking a returned struct no longer repeats
types. Name-only binding, with optional rename, built on `:=`:

```hll
divmod: (a: i32, b: i32) -> DivMod { return DivMod { quo: a / b, rem: a % b } }

{ quo, rem } := divmod(17, 5)          ; binds quo, rem by field name; types inferred
{ quo: q, rem: r } := divmod(17, 5)    ; rename while binding
{ quo } := divmod(17, 5)               ; partial: bind quo, ignore the rest
```

`destructure_bind = "{" ident [ ":" ident ] { "," ident [ ":" ident ] } "}" ":=" expression`.
The old typed-LHS form (`{ quo: i32, rem: i32 } = ...`) is removed.

#### V2. Sum types + `Option`/`Result` + `?`

Tagged unions with exhaustive `match`, and the two error carriers in the prelude:

```hll
enum Shape { Circle(f64), Rect(f64, f64), Empty }

area: (s: Shape) -> f64 {
    match s {
        Circle(r)  -> { return 3.14159 * r * r }
        Rect(w, h) -> { return w * h }
        Empty      -> { return 0.0 }
    }
}

enum Option<T>    { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

`match` is exhaustive -- a missing variant is a compile error (no silent fallthrough). The
`?` postfix operator propagates failure with a visible early return:

```hll
run: (in: Str) -> Result<i32, ParseError> {
    n := parse(in)?            ; Ok(v) -> v ; Err(e) -> return Err(e)
    return Ok(n * 2)
}
```

Grammar: `enum_decl = "enum" ident [ type_params ] "{" variant { "," variant } "}"`;
`variant = ident [ "(" type { "," type } ")" ]`; `match_expr = "match" expression "{" arm
{ arm } "}"`; `arm = pattern "->" block`; `postfix … | "?"`. Lowering: a sum value is
`{ tag: i64, payload }` where `payload` is sized to the largest variant; `match` is a tag
switch with payload binds; `?` desugars to `match x { Ok(v) -> v, Err(e) -> return Err(e) }`
(and the `Option` analogue). This retires both error conventions in S8.2.

Implementation status (2026-06-21): generic enum uses are monomorphized before semantic analysis.
Concrete `Option`/`Result` constructors, exhaustive statement-position `match`, and postfix `?`
compile and execute. `?` accepts only the canonical carriers, checks the enclosing return family
and `Result` error payload, and propagates failure through an early aggregate return. Value-producing
`match` remains pending. Aggregate arguments are copied by value through the compiler's indirect
RV64 calling convention; slices also round-trip through function returns.

#### V3. `struct` / `class` / `interface` (the OOP track)

Three type-introducing keywords; `type` is reserved for genuine aliases only:

- `struct` -- plain data (fields only, C-ABI/FFI shape). No methods, no conformance.
- `class` -- data + methods + interface conformance. The OOP type.
- `interface` -- a method contract; no fields.

```hll
interface Shape {
    area: (self) -> f64        ; `self` in an interface = self: Self*
}

class Circle : Shape {
    radius: f64
    area: (self) -> f64 { return 3.14159 * self.radius * self.radius }
}
```

`self` is sugar for `self: <EnclosingType>*` (keeps golden rule 4: the receiver is an
explicit pointer; `.` auto-derefs it per D2). Method calls resolve statically:
`c.area()` with `c: Circle` lowers to `Circle_area(&c)`; with `c: Circle*` to
`Circle_area(c)` -- a **direct call**, zero dispatch cost.

Polymorphism is opt-in via `dyn`, the only place a vtable appears, and it is visible in the
source:

```hll
shapes: dyn Shape[] = [ &circle as dyn Shape, &rect as dyn Shape ]
for s in shapes { println(fmt_f64(s.area())) }   ; indirect call through the vtable
```

`dyn Shape` is the fat pointer `{ data: void*, vtable: ShapeVTable* }` (same fat-pointer
idea as `Str`/slices). Generic bounds (`<T: Shape>`, R2) monomorphize to direct calls;
`dyn` is for genuinely heterogeneous collections. Grammar:
`class_decl = "class" ident [ type_params ] [ ":" bound ] "{" { field_decl | method_decl }
"}"`; `interface_decl = "interface" ident [ type_params ] "{" { method_sig } "}"`;
`type "dyn" ident` as a fat-pointer type.

#### H1. Slices `T[]` as the default sequence

A slice is the bounds-checked, element-scaled fat pointer `{ ptr: T*, len: u64 }`:

```hll
nums: i32[5] = [ 1, 2, 3, 4, 5 ]
view: i32[] = nums[1..4]      ; {ptr: &nums[1], len: 3}, no copy
for n in view { sum += n }    ; element-scaled, bounds-checked
mid: i32 = view[0]            ; indexing reads the element place
mid_ptr: i32* = &view[0]      ; take its address explicitly
```

- A string literal is a `u8[]` slice in V2 (same `{ ptr, len }` layout as the legacy `Str` record,
  so it links against `Str`-typed stdlib functions). `.len`, indexing, `for`, and `s[a..b]` apply.
- `T[]` is the slice type; `arr[a..b]` and `slice[a..b]` produce sub-slices (half-open).
- Array and slice indexing produce an assignable `T` place, not a `T*`. Use `&seq[i]` to
  obtain the element's address.
- Slice indexing is element-scaled and bounds-checked against `len` (panic/trap on OOB).
- A stack array coerces to a slice via `arr[..]` (or implicitly where a `T[]` is expected).
- A fixed array is zero-initialized with the empty literal: `buf: u8[16] = []`. V2 requires every
  binding to have an initializer, and `;` is a comment (so `[0; 16]` is impossible), so `[]` is the
  canonical zero-fill. Local arrays are zeroed by explicit element stores; `.bss` globals are already
  zero. A bare `buf: u8[16]` (no initializer) stays an error.
- Raw `ptr + i` **stays byte-scaled** -- it is now the explicit escape hatch, used only when
  you have dropped to a raw `T*`. With slices as the default, the H1 mismatch is no longer
  on the common path; the GOTCHAS entry is downgraded to "raw-pointer escape hatch only."

Grammar: the type suffix gains the unsized `"[" "]"` slice form; `range_expr = expression
".." expression` (and `..=` inclusive); `for_stmt = "for" ident "in" ( range_expr | expression )
block`. This also delivers the `for`/range feature and kills the old S7.1 "for conflicts with
comments" objection -- range-`for` needs no `;`.

#### H2. `as` precedence lowered

`as` moves from level 1 (tighter than everything) to a new level just **below** additive and
shift, **above** comparison. The common reading becomes the correct one with no parentheses:

```hll
v := @ptr as i32          ; now (@ptr) as i32       (was @(ptr as i32))
p := ptr + 3 as u8*       ; now (ptr + 3) as u8*    (was ptr + (3 as u8*))
b := a + b as u32         ; now (a + b) as u32      (was a + (b as u32))
```

Revised precedence (high to low): `() [] . ?` ; unary `@ & - !` ; `* / %` ; `+ -` ;
`<< >>` ; **`as`** ; `< <= > >=` ; `== !=` ; `&` ; `^` ; `|` ; `and` ; `or` ; `=`. The
GOTCHAS "as precedence" entry is deleted (the trap no longer exists). Migration: the `.hll`
pass strips the now-redundant `( ... )` it previously required around casts.

#### Deferred V2.1 features

- **String interpolation.** `"hi {name}, n={count}"` -- `{expr}` splices, `{{`/`}}` are
  literal braces. Lowers to a visible `format(...)` over the literal chunks + arg
  conversions, producing a `Str`; in `print("...")` context it lowers to a sequence of
  `write`s (no allocation). Conversions come from the `Display` interface (V3): a value is
  interpolable iff it implements `Display`.
- **Composite array literals.** `[ e0, e1, e2 ]` -- type `T[N]` inferred from elements or
  from the annotation; usable as an array initializer or coerced to `T[]`.
- **`for` over ranges and slices.** Defined under H1; lowers to the existing `while` IR.

---

## Migration summary

| Change | Touches existing `.hll`? | How |
|--------|--------------------------|-----|
| D1 braces / newline rules | no (already true) | doc only |
| D2 place/access model | yes | `@arr[i]` -> `arr[i]`; `arr[i]` as `T*` -> `&arr[i]`; `@p.field` is reviewed according to its intended meaning |
| D3 `:=` inferred decl | additive | existing `name: T = e` unchanged |
| D4 one literal form | yes | rewrite `{ .f = v }` / `{ f: T = v }` -> `{ f: v }` |
| C2 drop `-> ()` | yes | strip `-> ()` from void fns |
| H2 `as` precedence | yes (simplifies) | drop now-redundant cast parens |
| R2/V2/V3/H1 + features | additive | new syntax; legacy code requires explicit V1 mode |

The D4, C2, and H2 rewrites are mechanical and scriptable. V2 is the default for the public
compiler and pipeline APIs. A leading `; @version 1` directive or an explicit
`LanguageVersion::V1` configuration selects deprecated compatibility mode during the stabilization
window; selecting it emits a warning. Runtime, kernel, shell, demo, and example sources use V2.
The self-hosted HLL-0 toolchain remains an explicit V1 compatibility surface until its pointer-based
array representation is redesigned or migrated with execution-equivalent address-taking.
