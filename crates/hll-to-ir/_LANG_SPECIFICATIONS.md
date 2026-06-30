# HLL Language Specification

HLL is a small systems language built around a consistency-first memory model. Memory
operations are explicit and deterministic: there are no hidden conversions, no hidden
ownership, and no hidden dispatch. This document is the normative specification of the
language the `hll-to-ir` front end implements. Sections describing unbuilt machinery are
collected in the non-normative roadmap appendix and are clearly marked.

There is one HLL language and one compiler. Earlier drafts of this document described a
predecessor dialect; that dialect and its compiler mode no longer exist.

## 1. Core design principles

HLL enforces a consistent pointer and place model.

### 1.1 The golden rules

1. Pointers are always pointers. If a type contains `*`, it is a pointer type. There are no
   implicit conversions between `T` and `T*`.
2. Indexing and fields produce places, not pointers. `seq[i]` is an assignable element place
   of type `T`; `place.field` selects a field. `.` auto-dereferences exactly one pointer
   level, so a field of `p: T*` is `p.field`. Take an address explicitly with `&`.
3. `@` is reserved for a pointer's whole pointee. `@ptr` reads the entire pointee value and
   `@ptr = value` writes it. Ordinary array and field access never needs `@`.
4. No mutable primitive parameters. All parameters are pass-by-value; mutation of a caller's
   storage requires an explicit pointer parameter (`T*`) and `&` at the call site.

`&place` produces a pointer to a place; `&` rejects non-place temporaries (`&@ptr` is
invalid). Returning the address of a local (`return &x`) is a compile-time error.

## 2. Syntax and lexical conventions

| Feature | Rule |
|---------|------|
| Comments | Semicolon `;` starts a line comment and consumes the rest of the line. |
| Blocks | Brace-delimited `{ ... }`. Indentation is insignificant. |
| Statement termination | A newline terminates a complete statement. `;` is never a separator. |
| Continuation | A statement continues onto the next line when the current line cannot yet form a complete statement: it ends with a binary operator, a comma, or an open `(`/`[`/`{`. |
| Type annotations | `name: Type` (`:` always introduces a type, never a value). |
| Type casting | Postfix `expr as Type` only. There is no prefix `Type(value)` cast form. |

### 2.1 Syntax examples

```hll
x: i32 = 42                ; explicitly typed declaration
y := 3.1415               ; inferred declaration (f64 from the literal)
z: i32 = 42               ; trailing comment

; multi-line continuation: the first line ends on a binary operator
w: i32 = 1 + 2
    + 3

; explicit cast
ptr: i32* = 1000 as i32*
addr: i64 = ptr as i64
```

## 3. Type system and declarations

### 3.1 Primitive types

| Type | Description | Size | Default |
|------|-------------|------|---------|
| `i8`, `i16`, `i32`, `i64` | Signed integers | 1, 2, 4, 8 bytes | `0` |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | 1, 2, 4, 8 bytes | `0` |
| `f32`, `f64` | IEEE 754 floats | 4, 8 bytes | `0.0` |
| `bool` | Boolean | 1 byte | `false` |

### 3.2 Bindings

Declaration and assignment are syntactically separate, so `x = e` is never ambiguous:

| Form | Meaning |
|------|---------|
| `name: T = expr` | declare `name` with explicit type `T` |
| `name := expr` | declare `name`, inferring its type from `expr` |
| `name = expr` | assign to an existing `name` (never declares) |

Assigning a name that has not been declared is a compile-time error (this catches typos).
Inference (`:=`) is local: it never crosses a function boundary and uses only the resolved
type of the right-hand side, which must be a single concrete non-void type.

```hll
count: i32 = 10           ; explicit
total := count + 5        ; inferred i32
const MAX_SIZE = 100      ; compile-time constant
```

A `const` initializer is evaluated at compile time over integer, float, boolean, and
character values with the usual operators. Field access on a compile-time value (reaching
into a struct literal from a `const`) is intentionally not part of constant evaluation; use
a runtime binding for that.

### 3.3 Initialization and allocation

```hll
buffer: u8[16] = []       ; fixed array, zero-filled by the empty literal
values: i32[4] = [3, 5, 7, 11]
data_ptr: i32* = new(i32)        ; one element, zero-initialized
array_ptr: i32* = new(i32, 10)   ; N contiguous elements; N may be a runtime expression
```

- Every binding must have an initializer. The empty array literal `[]` zero-fills a fixed
  array; because `;` is a comment, no `[0; N]` repeat form exists, and a bare
  `buffer: u8[16]` (no initializer) is an error. `[]` carries no element type or length on
  its own, so it is only valid where the array type is known from the binding or
  destination; an `[]` in a context without an expected type is rejected.
- `new(T)` returns `T*` for a single element; `new(T, N)` returns `T*` for `N` elements. All
  heap allocations are zero-initialized. Every allocation needs a matching `free()` or
  `defer free()`; there is no garbage collection.

### 3.4 Strings and character literals

A string literal `"text"` is a `u8[]` slice (see 5.3): a `{ ptr: u8*, len: u64 }` fat
pointer over read-only bytes. Its `.ptr` and `.len` fields, indexing, iteration, and range
slicing all apply. This layout matches the legacy `Str` record, so a `u8[]` string links
against `Str`-typed stdlib functions across translation units.

```hll
name: u8[] = "Ada"
first: u8 = name[0]       ; bounds-checked element read
raw: u8* = name.ptr       ; raw pointer for a C-string API
```

A character literal `'c'` is an integer literal equal to the ascii byte of `c` (default type
`i32`), so `putc('A')` and `putc(65)` are identical. The escapes `\n \t \r \b \0 \\ \' \"`
are recognized; the body must be exactly one ascii character.

## 4. Pointer semantics

### 4.1 Core operations

| Operation | Syntax | Type rule | Example |
|-----------|--------|-----------|---------|
| Read whole pointee | `@ptr` | `T* -> T` | `val: i32 = @x_ptr` |
| Write whole pointee | `@ptr = val` | `T* <- T` | `@x_ptr = 42` |
| Address-of place | `&place` | `T -> T*` | `x_ptr: i32* = &x` |
| Allocate | `new(T)` / `new(T, N)` | `-> T*` | `p: Point* = new(Point)` |
| Deallocate | `free(ptr)` | `T* -> void` | `free(x_ptr)` |
| Cast | `expr as Type` | `S -> T` | `slot_ptr as u8*` |

### 4.2 Pointer arithmetic

- `T* + n` and `T* - n` are element-scaled: the offset advances by `n * sizeof(T)`. This
  agrees with `seq[i]`, so the two ways to reach element `i` mean the same thing.
- Byte arithmetic is expressed on `u8*`, where the element size is 1.
- Integer-minus-pointer and pointer-minus-pointer are invalid.
- `@(ptr + n)` reads the element `n` steps after `ptr`. Raw-pointer indexing `ptr[n]` is the
  element-place equivalent and is unchecked (slices add the bounds check; see 5.3).

## 5. Composite types

### 5.1 Arrays

A fixed array is `T[N]`. Indexing produces an assignable place of type `T`, element-scaled.
Use `&arr[i]` for the element's address; a fixed array does not silently decay to a pointer.

```hll
nums: i32[5] = [1, 2, 3, 4, 5]
nums[0] = 10                 ; write a place
first: i32 = nums[0]         ; read a place
addr: i32* = &nums[0]        ; explicit address
```

### 5.2 Structs

`struct` introduces a plain data type. `type` is reserved for aliases only.

```hll
struct Point {
    x: f32,
    y: f32,
}
```

A struct literal is `.field = expr` pairs in braces, optionally prefixed by the type name. A
named literal must set every field; a contextual literal whose type is known from the
destination may omit fields, which default to zero. Field order is free; `:` never appears in
a literal because it only introduces a type.

```hll
p := Point { .x = 1.0, .y = 2.0 }   ; named -- type explicit
q: Point = { .x = 1.0, .y = 2.0 }   ; contextual -- type from the annotation
@ptr = { .x = 3.0, .y = 4.0 }       ; contextual -- type from the lvalue
```

Field access uses `.` and auto-dereferences one pointer level:

```hll
p.x = 3.0                ; p: Point        -- direct
r.x = 3.0                ; r: Point*       -- auto-dereferences r
whole: Point = @r        ; @ reads the whole pointee value
@r = p                   ; @ writes the whole pointee value
```

`@p.field` parses as `@(p.field)` (dereference a pointer stored in `field`); it is not
`(@p).field`, which is redundant because `p.field` already performs the one permitted member
auto-dereference.

### 5.3 Slices and ranges

A slice `T[]` is a bounds-checked, element-scaled fat pointer `{ ptr: T*, len: u64 }`.
Copying a slice copies only the fat pointer.

```hll
nums: i32[5] = [1, 2, 3, 4, 5]
view: i32[] = nums[1..4]     ; { &nums[1], 3 }, no copy
mid: i32 = view[0]           ; bounds-checked element read
mid_ptr: i32* = &view[0]     ; explicit element address
length: u64 = view.len
```

- `arr[a..b]` and `slice[a..b]` produce a half-open sub-slice; `..=` is inclusive; open
  endpoints default to `0` (start) and the source length (end).
- Slice indexing is element-scaled and bounds-checked against `.len`. An out-of-bounds access
  traps with a stable diagnostic code (see 7.4). Raw-pointer indexing is unchecked.
- A fixed array coerces to a slice via `arr[..]`, and implicitly where a `T[]` is expected.

### 5.4 Destructuring

A returned (or stored) struct can be unpacked by a typed field pattern. Fields are matched by
name, so the listed order need not match the declaration, and listing a subset discards the
rest.

```hll
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}

main: () -> i32 {
    s := divide(17, 5)               ; bind the whole struct
    { remainder: i32, quotient: i32 } = divide(17, 5)   ; by name, any order
    { quotient: i32 } = divide(17, 5)                   ; partial -- discard remainder
    return quotient - 3
}
```

## 6. Functions, generics, and modules

### 6.1 Functions

```hll
add: (a: i32, b: i32) -> i32 { return a + b }   ; value-returning
log_line: (s: u8[]) { println(s) }              ; void -- omit -> entirely
```

- A value-returning function names its return type after `->`.
- A void function omits the arrow. The `-> ()` form does not exist. A bare `return` ends a
  void function early.
- All parameters are pass-by-value. Aggregates may use a hidden pointer in the ABI without
  changing source semantics.
- Multiple results are returned as an anonymous struct (5.4).

Function pointer values use `fn(T, U) -> R` for a value-returning callable address and
`fn(T, U)` for a void callable address. A bare function name in value position coerces to a
matching function pointer type, and calling a binding of function pointer type uses the same
`callee(args)` syntax as a direct call. Function pointers are raw code addresses; they do not
carry closure environments.

A function pointer is plain pointer-sized data, so a struct field, array element, or any
other place may hold one. The callee in a call may therefore be an arbitrary expression, not
only a bare name: `record.op(args)`, `table[i](args)`, and `(@handler)(event)` all evaluate
the callee to a function pointer value and then call indirectly. A bare-name call still binds
as a direct call (or enum constructor, cast, or generic instantiation); only a non-name callee
becomes an indirect call. There is no implicit receiver: any state a function field needs is
passed explicitly, usually as another field holding a pointer.

### 6.2 Methods (`impl` blocks)

Methods are static sugar over ordinary functions; there is no dynamic dispatch, vtable, or
hidden receiver. An `impl Type { ... }` block groups functions that take a pointer receiver
named `self`:

```hll
struct Rect { width: i32, height: i32 }

impl Rect {
    area: (self: Rect*) -> i32 { return self.width * self.height }
    scaled: (self: Rect*, k: i32) -> i32 { return self.area() * k }
}

main: () -> i32 {
    r: Rect = Rect { .width = 6, .height = 7 }
    return r.scaled(2) - r.area() - 42   ; 84 - 42 - 42 == 0
}
```

- Each method desugars to a free function named `Type_method`. `impl Rect { area: ... }`
  defines `Rect_area`, so methods share the function namespace and must not collide with an
  existing free function of the same mangled name.
- The receiver must be the first parameter, named `self`, of pointer type `Type*`.
- A call `recv.method(args)` is sugar for `Type_method(self, args)`. The receiver supplies
  `self`: a value receiver is addressed automatically (`r.area()` passes `&r`), and a pointer
  receiver is passed through. A method may call another through `self` (`self.area()`).
- A struct data field shadows a method of the same name: if `recv.name` names a function
  pointer field, `recv.name(args)` is the indirect field call (6.1), not a method call.
- Dispatch is fully static. For runtime polymorphism, build an explicit vtable struct of
  function pointers (6.1).

### 6.3 Generics

Type parameters on functions and structs are monomorphized: the compiler emits one concrete
copy per distinct type argument set, name-mangled deterministically, with no boxing and no
runtime type information.

```hll
struct Box<T> { value: T }

max: <T>(a: T, b: T) -> T {
    if a > b { return a }
    return b
}

main: () -> i32 {
    b: Box<i32>* = new(Box<i32>)
    defer free(b)
    b.value = max<i32>(19, 23)
    return b.value - 23
}
```

Type arguments may be explicit (`max<i32>(...)`) or inferred from structurally deducible
literal, local, pointer, array, slice, and named-type arguments; an unconstrained argument
must be given explicitly. The V2 core has no interface system, so generics are unconstrained:
a specialization that uses an operation the concrete type does not support fails at the
specialized site with an ordinary type error. A recursive specialization that does not
converge is diagnosed rather than allowed to hang.

Generics do not cross a module boundary yet. A module interface carries an exported function
only as an `external` signature, not its body, so an importer cannot monomorphize an imported
generic at the use site. Until interfaces carry generic bodies, a generic must live in the
module that instantiates it.

### 6.4 Modules, `import`, and `export`

Each source file is one module. Modules compile separately to their own object and compose
only by object linking; build paths never concatenate HLL source text to form a translation
unit. A module names what it offers with `export` and what it consumes with `import(...)`. The
result is a single source of truth: the module graph and the shared interface both live in
the HLL, not in host wiring tables.

```hll
math := import("./math.hll")           ; bind module `math` (one new top-level name)

export const ZERO = 0                  ; offer a constant to importers
export struct Point { x: i32, y: i32 } ; offer a struct to importers
export add: (a: i32, b: i32) -> i32 { ... } ; offer a function to importers
external puts: (s: u8*) -> i32         ; low-level: a symbol from another unit, resolved at link

main: () -> i32 {
    return math.add(math.ZERO, 2)      ; exports are fields of the module value
}
```

#### `import(...)` is a compile-time module value

`import(path)` is a compiler builtin expression that yields a compile-time module record. It is
valid only in a top-level binding or constant (`alias := import(path)` or
`const alias = import(path)`); it is rejected inside a function body. The record has no runtime
storage, no address, and no IR value: it exists only so its exported declarations can be named
as fields (`math.add`, `math.Point`, `math.ZERO`). An import binding introduces exactly one new
top-level name; the target's exports do not enter the importer's bare namespace.

#### Visibility

`export` marks a declaration a field of the module value; an unexported declaration is module
private and is not a field. Exportable forms are `type`, `const`, binding (global), `struct`,
`enum`, and function declarations. Naming a missing or private member (`math.private_helper`)
is a compile error that names the target module and the absent export.

#### Qualified access

Exports are reached through the alias: `math.add(a, b)` calls exported `add`, `math.ZERO` reads
exported const `ZERO`, `math.Point` names exported struct `Point` in type position. An exported
function or global lowers to the same link reference an `external` would; an exported `type` or
`const` folds in the importer with no link symbol. Qualified calls are direct calls, not
indirect function-pointer calls.

For modules that preserve legacy flat link symbols, the host pipeline also accepts a short member
spelling when it can map it unambiguously to an exported symbol: `pmm.alloc()` resolves to
`pmm_alloc`, `scheduler.init()` resolves to `scheduler_init`, and `klog.ok()` resolves to
`klog_ok`. This is a source-level namespace convenience only; it does not rename object symbols
or change `.globl` visibility.

Inside an `asm { }` block the qualified `alias.member` spelling does not apply: inline assembly
names HLL symbols by their literal flat link name (`call trap_handler`, `la t0, pmm_free_head`).
This is the one allowed escape from qualified access. A module that calls an imported symbol only
from inline asm still imports it (e.g. `trap_handler_dep := import("trap_handler")`) so the build
closure compiles the provider and prepends its link interface; the alias binding may go otherwise
unused. Such modules must be built with per-module mangling off so the literal names resolve.

#### Paths

| Form | Meaning |
| --- | --- |
| `import("./math.hll")` | Local file relative to the importing module. |
| `import("../util/math.hll")` | Local file via a normalized relative path. |
| `import("http")` | Standard library or package module named `http`. |

Bare package names must not resolve to local project files. Imports are pure: resolving one
parses and type-checks the target's exported interface and adds a link edge, but does not run
the target's top-level code. Effectful setup lives behind an explicit function (`db.init()`).
Cyclic imports are rejected with a diagnostic that names the cycle.

The earlier whole-module form `import "name"` is no longer accepted. Imports must bind a
module value (`alias := import(path)` or `const alias = import(path)`) and exported members
must be reached through that alias.

#### `external` (low level)

`external name: (params) -> ret` and `external name: type` declare a symbol defined in another
unit with no body, resolved at link. `external` pulls in no module and contributes no link
edge; it only names a symbol the linker is expected to satisfy from some object already in the
link set. It remains the primitive that `import` is built on: an imported function lowers to
the same reference an `external` would. Hand-written `external` stays valid for cases the
registry does not cover, but ordinary cross-module sharing should use `import`/`export`.

#### Implementation status

The mechanisms above are the target. Current status:

- `external` is fully implemented and remains the low-level link primitive. Ordinary bundled
  kernel APIs have migrated to `import`/`export`; hand-written `external` remains for boundary
  symbols such as runtime entry points and trap assembly edges.
- `export` is retained on the AST (`Declaration::exported`) and carried into IR for functions
  and globals. The RV64 backend emits `.globl` only for exported declarations; private
  declarations remain local object symbols. The closure build still mangles every module's own
  fn/global symbols to `module__name` (private and exported alike) so unexported names do not
  collide across modules.
- `import` interface resolution is implemented for the host pipeline. `CompilationPipeline`
  carries a module resolver (name -> source) and, for each direct `import`, prepends the
  target's extracted interface (`hll_to_ir::imports`): exported `type`/`const`/`struct`/`enum`
  verbatim and exported `fn`/global as `external`. The in-VM `cc` toolchain and the raw
  `HllCompiler` path do not resolve imports; the import decl is inert there.
- Qualified `alias := import(path)` is implemented for value, call, const, and type exports:
  the pipeline resolves each module binding, prepends its interface, and passes the alias's
  exported names to the compiler, which rewrites `alias.member`, `alias.member(args)`, and
  `alias.Type` to the flat export reference and reports unknown or private members against the
  named module. Short member aliases are generated from exported names by stripping the import
  alias prefix (`pmm_alloc` -> `alloc`) or a kernel `k` prefix (`kshutdown` -> `shutdown`) when
  the mapping is unique. In a mangling closure build, each module's link-symbol exports resolve
  to `module__name` (const/type exports still fold flat); imports of stdlib (precompiled-flat)
  modules resolve flat. When mangling is off the flat namespace applies and duplicate imported
  export names or collisions with local top-level declarations are rejected. Bare names resolve
  through the target stdlib and host registry; paths beginning with `./`, `../`, `.\\`, `..\\`,
  or an absolute path resolve from the importing source file's directory, not the registry.
- The host pipeline compiles the transitive closure of qualified `import(path)` dependencies
  into per-module objects, dependencies first. Per-module mangling is on by default (so two
  modules may export the same name); `set_module_mangling(false)` disables it for the kernel,
  whose inline asm names HLL symbols literally and which links with unique flat names. Cycles
  are rejected under mangling and tolerated when flat (the kernel's `syscall`/`scheduler` pair).
  Header-only modules (e.g. `layout`) are skipped by the closure and reach importers via the
  prepended interface.
- The bundled resolver covers kernel modules (`layout`, `trap_entry`, `pmm`, `vmm`, `process`,
  `scheduler`, `fs`, `syscall`, `trap_handler`, `utilities`, `checks`) and stdlib modules
  (`console`, `sys`, `cstr`, `runtime`, `klog`, `mem`, `string`, `memory_allocator`) through the
  generated `os_runtime::module_source` registry. `hll_to_ir::stdlib` reads the ordered
  `stdlib/stdlib.build` lists for hosted, freestanding, and kernel module order; the folders are
  organizational only. Kernel TUs import the providers they use with qualified
  `name := import("name")`; `layout.hll` exports the PCB/trap-frame structs and ABI consts. The
  production kernel build (`ensure_kernel_binary`, the GUI, the CLI, and the integration tests)
  drives the link set from the `my_kernel` closure with mangling off, linking it against the
  precompiled stdlib objects plus the boot `entry`. The kernel-mode source prelude
  (auto-prepended `layout.hll`) remains as the fallback for the raw `HllCompiler` path, where the
  import is inert. The split `as`/`cc`/`ld`/`shell` tools are also closure-built through
  qualified relative imports; `as` and `cc` keep their layout headers as shared record prelude.
  User programs use qualified stdlib imports rather than hand-written `external` declarations;
  `external` remains reserved for ABI/runtime edges such as `_start` calling user `main`.

Migration order: (1) interface import for `type`/`const`/signatures, retiring the source
prelude where possible; `as`/`cc` still need layout prelude for shared records); (2)
`import`-driven transitive link closure with per-module mangling, now driving the kernel and
userspace tool builds (`aux_sources` retired for bundled manifests); (3) `export` visibility
enforcement at the object level (`.globl` only for exports), now implemented for host HLL builds.
`external` remains for ABI/syscall declarations and explicit cross-object symbols.

## 7. Control flow, enums, and resource management

### 7.1 Control flow

`if` / `else`, `while`, `break`, `continue`, and `for` are supported.

```hll
sum := 0
for n in 0..10 { sum += n }     ; range; ..= is inclusive
for v in view { sum += v }      ; iterate an array or slice
```

`for var in iterable { ... }` iterates a range, a fixed array, or a slice, and desugars to a
`while` loop. The end of a range is evaluated once; `continue` still advances the loop.

### 7.2 Enums, `match`, and `?`

An enum is a tagged union with unit and payload variants. Its runtime layout is
`{ tag: i64, payload }`, where the payload area is sized to the largest variant.

```hll
enum Shape { Circle(f64), Rect(f64, f64), Empty }

area: (s: Shape) -> f64 {
    match s {
        Circle(r)  -> { return 3.14159 * r * r }
        Rect(w, h) -> { return w * h }
        Empty      -> { return 0.0 }
    }
}
```

A unit variant is constructed by naming it; a payload variant by calling it (`Circle(2.0)`).
`match` is exhaustive: every variant must be covered, or a catch-all (`_` or a lowercase
binding) provided. A `match` may produce a value when every arm is a `-> expr` value arm whose
types agree; it is usable as a binding initializer, assignment right-hand side, or return
value.

A `match` on an integer scrutinee instead dispatches on scalar value equality. Arms are
integer or char literals (`'A'` reads as its ascii byte; a leading `-` forms a negative
literal) or a bare name that resolves to an integer constant, plus a binding or `_` catch-all
that sees the scrutinee value. Such a match cannot enumerate its domain, so a catch-all is
required. A const pattern must be the constant's capitalized form (it parses as a bare unit
name); a lowercase name is always read as a binding catch-all.

```hll
classify: (c: i32) -> i32 {
    return match c {
        '\n' -> 10
        '\t' -> 9
        _    -> c
    }
}
```

A leading `-N` literal pattern is only unambiguous in block-body arms or as the first arm,
since bare `-> expr` value arms parse newline-insensitively (a value arm followed by `-N`
reads as subtraction).

The prelude provides `Option<T>` (`Some(T)` / `None`) and `Result<T, E>` (`Ok(T)` /
`Err(E)`) unless a user declaration shadows the name. Postfix `?` propagates failure with a
visible early return:

```hll
run: (in: u8[]) -> Result<i32, ParseError> {
    n := parse(in)?            ; Ok(v) yields v; Err(e) returns Err(e) from run
    return Ok(n * 2)
}
```

`?` accepts only `Option<T>` and `Result<T, E>`, requires the enclosing return type to carry
the propagated failure, and for `Result` requires compatible error payloads.

### 7.3 The `defer` statement

A deferred call runs when the enclosing function exits, in LIFO order. Arguments are captured
at declaration time, not at execution, so reassigning a captured variable afterward does not
affect the deferred call. A `defer` statement may not contain a `return`.

```hll
ptr: i32* = new(i32)
defer free(ptr)          ; captures this ptr value now
ptr = new(i32)           ; the deferred free still targets the first allocation
```

### 7.4 Bounds and trap behavior

A failed slice/array bounds check, and any other checked runtime failure, transfers to a
non-returning trap with a stable diagnostic code. In the VM and kernel this is observable as a
clean halt carrying that code; raw-pointer access is documented as unchecked.

## 8. Inline assembly

Two forms emit raw RISC-V or read hardware registers. They exist only for low-level system
code (`_start`, syscall wrappers); application code should not need them.

`asm_reg(name)` reads a named ABI register as an `i64` and is valid in any expression:

```hll
stack_ok: () -> bool { return asm_reg(sp) > 0x10000 }
```

`asm { }` is a verbatim block of RISC-V instructions, one per line, whitespace-delimited:

```hll
_start: () {
    asm {
        call main
        li   a7, 93
        ecall
    }
}
```

- Allowed registers (both forms): `sp`, `fp`, `ra`, `gp`, `tp`, `a0`-`a7`, `s1`-`s11`. Spell
  the frame pointer `fp` (the `s0` alias is not accepted). Temporaries `t0`-`t6` are not
  allowed: the register allocator may hold live values in them at any asm site.
- An `asm { }` block contains raw assembly text only (no HLL expressions, no data directives).
  Branches and labels inside a block must not target outside it, and blocks cannot nest.

## 9. Formal grammar (EBNF)

### 9.1 Lexical grammar

```ebnf
ident       = letter { letter | digit | "_" };
integer     = digit { digit };
hex_integer = "0x" hex_digit { hex_digit };
float       = digit { digit } "." digit { digit } [ exponent ];
char        = "'" ( any_char - "'" | escape ) "'";
string      = '"' { any_char - '"' } '"';
comment     = ";" { any_char - newline };
newline     = "\n" | "\r\n";
```

### 9.2 Syntactic grammar

```ebnf
program        = { declaration };
declaration    = binding_decl | function_decl | struct_decl | enum_decl | type_decl
               | const_decl | import_decl | export_decl | external_decl;
import_decl    = module_import | "import" string;
module_import  = identifier ":=" "import" "(" string ")"
               | "const" identifier "=" "import" "(" string ")";
export_decl    = "export" declaration;
external_decl  = "external" identifier ":" [ type_params ] "(" [ param_list ] ")"
                 [ "->" type ];
binding_decl   = identifier ":" type "=" expression
               | identifier ":=" expression;
struct_decl    = "struct" identifier [ type_params ] "{" [ field_decl { "," field_decl }
                 [ "," ] ] "}";
enum_decl      = "enum" identifier [ type_params ] "{" variant { "," variant } "}";
variant        = identifier [ "(" type { "," type } ")" ];
type_decl      = "type" identifier "=" type;
const_decl     = "const" identifier "=" expression;
field_decl     = identifier ":" type;
type_params    = "<" identifier { "," identifier } ">";
type           = primitive_type | identifier [ type_args ] | struct_def | function_pointer_type
               | array_type | slice_type | pointer_type;
type_args      = "<" type { "," type } ">";
struct_def     = "{" [ field_decl { "," field_decl } [ "," ] ] "}";
function_pointer_type = "fn" "(" [ type { "," type } ] ")" [ "->" type ];
array_type     = type "[" integer "]";
slice_type     = type "[" "]";
pointer_type   = type "*";
primitive_type = "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
               | "f32" | "f64" | "bool";
function_decl  = identifier ":" [ type_params ] "(" [ param_list ] ")" [ "->" type ] block;
param_list     = parameter { "," parameter };
parameter      = identifier ":" type;
block          = "{" { statement } "}";
statement      = expression | binding_decl | assignment | destructure_assign
               | if_stmt | while_stmt | for_stmt | return_stmt | defer_stmt | asm_block;
assignment     = place ( "=" | compound_op ) expression;
destructure_assign = "{" field_decl { "," field_decl } [ "," ] "}" "=" expression;
compound_op    = "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=";
if_stmt        = "if" expression block [ "else" ( if_stmt | block ) ];
while_stmt     = "while" expression block;
for_stmt       = "for" identifier "in" ( range_expr | expression ) block;
range_expr     = [ expression ] ( ".." | "..=" ) [ expression ];
return_stmt    = "return" [ expression ];
defer_stmt     = "defer" expression;
asm_block      = "asm" "{" { asm_line } "}";
expression     = match_expr | binary_expr | cast_expr | unary_expr | postfix_expr;
match_expr     = "match" expression "{" arm { arm } "}";
arm            = pattern "->" ( block | expression );
pattern        = "_" | identifier | identifier "(" pattern { "," pattern } ")"
               | [ "-" ] ( integer | char );
cast_expr      = expression "as" type;
unary_expr     = ( "@" | "&" | "-" | "!" ) expression;
postfix_expr   = primary_expr { "." identifier | "[" index_or_range "]"
                 | "(" [ arg_list ] ")" | "?" };
index_or_range = expression | range_expr;
primary_expr   = identifier [ type_args ] | literal | "(" expression ")"
               | array_literal | struct_literal | new_expr | asm_reg_expr;
new_expr       = "new" "(" type [ "," expression ] ")";
asm_reg_expr   = "asm_reg" "(" abi_reg ")";
array_literal  = "[" [ expression { "," expression } ] "]";
struct_literal = [ identifier ] "{" [ field_init { "," field_init } [ "," ] ] "}";
field_init     = "." identifier "=" expression;
```

### 9.3 Operator precedence

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `() [] . ? as` (postfix) | Left |
| 2 | `@ & - !` (unary) | Right |
| 3 | `* / %` | Left |
| 4 | `+ -` | Left |
| 5 | `<< >>` | Left |
| 6 | `< <= > >=` | Left |
| 7 | `== !=` | Left |
| 8 | `&` (bitwise) | Left |
| 9 | `^` (bitwise) | Left |
| 10 | `\|` (bitwise) | Left |
| 11 | `and` | Left |
| 12 | `or` | Left |
| 13 | `=` and compound assignment | Right |

`as` is a high-precedence postfix operator and applies to the single operand on its left.
`a + b as u32` is `a + (b as u32)`, `ptr + 3 as u8*` is `ptr + (3 as u8*)`, and
`@ptr as i32` is `@(ptr as i32)` (cast then dereference), not `(@ptr) as i32`.
Parenthesize the operand to cast a larger expression or a dereference: write
`(a + b) as u32` and `(@ptr) as i32`.

Evaluation order: binary operands, function arguments, and struct-literal fields are evaluated
strictly left to right, each base/index/call expression exactly once. `defer` cleanup runs in
LIFO order at scope exit.

### 9.4 Compound assignment

`lhs OP= rhs` is shorthand for `lhs = lhs OP rhs`, available for `+= -= *= /= %=` and the
bitwise/shift forms `&= |= ^= <<= >>=`. The right-hand side is the whole expression, so
`x -= a + b` means `x = x - (a + b)`.

## 10. Memory safety framework

- Pointer type `T*` is the set of addresses holding values of type `T`. There are no implicit
  conversions; the type system separates `T` and `T*`, and casting requires explicit `as`.
- `&` marks a place as escaping; returning a pointer to a local is a compile error.
- Slice and array indexing are bounds-checked (5.3, 7.4); raw-pointer indexing is the explicit
  unchecked escape hatch.
- Every heap allocation needs a matching `free()`/`defer free()`. Memory safety is a
  single-threaded guarantee; concurrency requires external synchronization.

## 11. Standard library reference

### 11.1 Standard library modules

```hll
console := import("console")
sys     := import("sys")       ; hosted targets only
cstr    := import("cstr")      ; hosted targets
string  := import("string")
mem     := import("mem")
alloc   := import("memory_allocator")
```

The normal source API is qualified through imports:

```hll
console.writeln("hello".ptr)
fd: i64 = sys.open("/tmp/a".ptr, 0)
same: i64 = cstr.eq("a".ptr, "a".ptr)
ok: bool = string.equals("x", "x")
mem.copy(dst, src, n)
ptr: u8* = alloc.malloc(64)
```

`console.print`/`console.println` take a `u8[]` slice (string literals are slices) and write
the exact byte range with no NUL scan. `console.putchar`, `console.write`,
`console.writeln`, `console.print_int`, and `console.print_hex` keep unique prefixed link
symbols internally but are reached through the short qualified member names above. Hosted
targets provide `sys.*` syscall wrappers; freestanding and kernel targets omit that module.

### 11.2 Strings

A string value is a `u8[]` slice (3.4): `.ptr`, `.len`, bounds-checked indexing, `for`, and
range slicing all apply. The standard string utilities operate on `u8[]` through the
`string` module. The layout-compatible
`Str` record (`{ data: u8*, length: u64 }`) is retained only as a legacy ABI declaration for
linking against existing `Str`-typed entry points.

### 11.3 Option and Result

`Option<T>` and `Result<T, E>` (7.2) are the standard carriers for absence and failure. Prefer
them with `match` and `?` over ad-hoc `{ value, error }` records or `null` sentinels.

## 12. Roadmap (non-normative)

The following are reserved or planned for a later language revision (V2.1). The compiler does
not accept incomplete forms of them today; they are documented here only so the normative
sections above are not mistaken for the full long-term design.

- **Classes, interfaces, and `dyn`.** First-class `class`/`interface` declarations with
  methods, a `self` receiver, static dispatch by default, and opt-in `dyn` fat-pointer
  polymorphism. `struct` remains the plain-data type; `type` remains alias-only.
- **Generic bounds.** Interface-constrained type parameters (`<T: Shape>`), checked at the
  instantiation site, once interfaces exist. The V2 core has only unconstrained generics.
- **Name-only destructuring.** `{ quo, rem } := expr` binding by field name without repeating
  types. The implemented form is the typed pattern in 5.4.
- **String interpolation.** `"hi {name}"` lowering to a visible format over literal chunks.
- **Allocators and richer runtime.** Arena/pool allocators, escape-analysis stack promotion,
  SIMD lane generation, and debug-symbol generation.

## Appendix A: HLL-0 (the self-hosting subset)

HLL-0 is the deliberately tiny subset the in-VM compiler `cc` accepts. It is not a separate
language: every HLL-0 program is also a valid HLL program except for the one I/O intrinsic
below. HLL-0 drops everything that needs a type checker or heap: one numeric type, no
pointers, structs, arrays, floats, casts, slices, `defer`, or inline `asm`.

### A.1 Types

Only `i32`. Every local, parameter, and return value is `i32`; arithmetic wraps modulo 2^32
(two's complement). There is no `bool`: comparisons yield `i32` `0`/`1`, and `if`/`while`
conditions test "non-zero".

### A.2 Program shape

A program is a list of function definitions. Execution starts at `main: () -> i32`; the `i32`
it returns becomes the process exit code. Functions take zero or more `i32` parameters and
return `i32`.

```hll
name: (p0: i32, p1: i32) -> i32 {
    ; statements
}
```

### A.3 Statements

| Statement | Form |
|-----------|------|
| Local declaration | `name: i32 = expr` |
| Assignment | `name = expr` |
| Conditional | `if expr { ... }` (no `else` in HLL-0) |
| Loop | `while expr { ... }` |
| Return | `return expr` |
| Expression statement | a bare call, e.g. `putc(10)` |

`break`/`continue`/`defer` are out of HLL-0 scope.

### A.4 Expressions

Integer and `'c'` char literals, parameter/local identifiers, function calls `f(a, b)`, the
binary operators `+ - * / %`, and the comparisons `< <= > >= == !=` (which produce `0`/`1`).
A char literal is its ascii byte, so `putc('A')` equals `putc(65)`. Multiplicative binds above
additive above comparison; parenthesize anything ambiguous.

### A.5 I/O intrinsic

`putc(ch: i32)` writes the low byte of `ch` to file descriptor 1. It is the only intrinsic;
`cc` lowers a `putc` call to a plain `call putc`, left undefined and resolved at link time
against the asm stdlib (`programs/user/sample/stdlib/stdlib.s`). Process exit is `main`'s return value,
lowered to an exit ecall. All other output is built from `putc`.

### A.6 Codegen target

`cc` emits naive stack-machine RISC-V in the subset the in-VM assembler `/bin/as` covers:
every local occupies a stack slot, operands are reloaded before each use, arguments pass in
`a0..a7`, and each function keeps `ra` in its frame across calls. The pure-HLL-0 sample
`programs/user/sample/hello/hello.hll` omits any `putc` definition; the in-VM
`cc` -> `as` -> `ld` -> run
toolchain assembles and links it against the asm stdlib.
