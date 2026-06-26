# Intermediate Representation Specification

This document specifies the typed, SSA-form intermediate representation that sits
between the HLL front end and the RISC-V back end. The front end (`hll-to-ir`)
lowers source to this IR; the back end (`ir-to-asm`) consumes it and emits assembly.
It is a strongly typed, "fat" high-level IR designed to be cheap to parse, easy to
read, and friendly to middle-end optimization.

## 1. Core design principles

The IR bridges HLL's strict, explicit memory model with a machine-agnostic format.
Four ideas shape it.

1. Static single assignment. Every virtual register is assigned exactly once, which
   keeps data-flow analysis and dead-code elimination simple.
2. Infinite virtual registers. Registers use a `$` prefix and are unbounded; register
   allocation is deferred entirely to the target-specific back end.
3. A fat instruction set. Instructions are expressive and polymorphic over type, which
   keeps the IR concise and semantically rich for high-level passes.
4. Explicit reads and writes. Virtual registers hold values; memory access always goes
   through `read` and `write`. The `@` sigil is reserved for memory access.

### 1.1 Aggregate type representation

HLL structs carry named fields, for example `type Point = { x: f32, y: f32 }`. The IR
preserves those field names in its aggregate types: the lowered form is
`{x: f32, y: f32}`, and named aliases such as `type Point = {x: f32, y: f32}` are emitted
at module scope. Field access still resolves to byte offsets at lowering time (the
`offset` instruction), but the names are kept in the type for readability. Anonymous
aggregates with empty field names are also legal where a one-off struct type is needed.

### 1.2 Signedness in integer operations

The IR does not distinguish signed from unsigned integer types; an integer type is just
its width (`i8`, `i16`, `i32`, `i64`). Signedness lives in the operation instead.

- Division and modulo: `sdiv` / `mod` are signed; `udiv` / `umod` are unsigned. A plain
  `div` opcode exists for completeness, but the front end always selects `sdiv` or `udiv`
  from the operand's signedness.
- Comparison: each comparison carries an `s` or `u` prefix where it matters
  (`slt` vs `ult`, and so on).
- Casting: `sext` (sign-extend) and `zext` (zero-extend) widen with the matching
  interpretation of the source.

## 2. Syntax and lexical conventions

The IR uses a simple, unambiguous text format meant for fast parsing and human reading.

| Entity | Syntax | Example |
|--------|--------|---------|
| Virtual register | `$` prefix | `$1`, `$base_ptr`, `$t0` |
| Basic block | bare label and `:` | `entry:`, `loop_header:` |
| Function | bare name after `define` / `call` | `define i32 compute_sum(...)`, `call compute_sum(...)` |
| Global string | `const` declaration | `const hello = c"hi"` |
| Temporary register | numeric | `$0`, `$1`, `$2` (compiler generated) |
| Named register | alphanumeric | `$count`, `$value` (front-end mapped) |
| Comment | semicolon `;` | `; calculate offset` |

## 3. Type system

The IR mirrors the HLL front-end types closely to keep the semantic gap small.

| Type | Description |
|------|-------------|
| `void` | No value (used for function returns). |
| `i1`, `i8`, `i16`, `i32`, `i64` | Integers (`i1` is boolean). Signedness is in the opcode, not the type. |
| `f32`, `f64` | IEEE 754 floating point. |
| `T*` | Pointer to type `T`. |
| `fn(T, U) -> R` / `fn(T, U)` | Function pointer code address with parameter and return types. |
| `T[N]` | Fixed-size array, for example `i32[10]`. |
| `{name: T, ...}` | Aggregate (struct) type with named fields; names may be empty for anonymous aggregates. |
| `Name` | A named type definition or alias. |

### 3.1 Aggregate types

An aggregate can appear two ways:

- As a module-level named alias for a reusable struct definition.
- Inline in a type expression (an allocation, parameter, or return type).

Both forms are equivalent at runtime; the choice is stylistic. Field names are kept in
the type, but member access is always computed as a byte offset.

```text
; Named alias (reusable)
type Point = {x: f32, y: f32}
type DivideResult = {i32, i32}

; Inline aggregates in signatures and allocations
define {i32, i32} divide(i32 $a, i32 $b) { ... }
$ptr = stack_alloc {i32, i32}
```

## 4. Instruction set

### 4.1 Memory and state

Memory instructions use `@` to mirror HLL's dereferencing and to separate type-scaled
indexing from byte-scaled offsets.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| `stack_alloc` | `$dest = stack_alloc <type> [count]` | Allocate stack space. Returns `type*`. |
| `heap_alloc` | `$dest = heap_alloc <type> [x<count>]` | Allocate zero-initialized heap memory. Returns `type*`. Mirrors HLL `new(...)`; `count` may be a runtime register. |
| `heap_free` | `heap_free $ptr` | Free heap memory. |
| `read` | `$dest = read <type> @ $ptr [+ offset]` | Read from memory. `offset` is an immediate byte offset. |
| `write` | `write <type> <value> @ $ptr [+ offset]` | Write `<value>` to `$ptr` plus `offset` bytes. |
| `index` | `$dest = index <type> $base_ptr, $idx` | Array indexing; scales the offset by `sizeof(type)`. Returns a pointer. |
| `offset` | `$dest = offset <type> $base_ptr, $byte_offset` | Raw pointer arithmetic; scales strictly by 1 byte. Returns a pointer. |
| `global_ref` | `$dest = global_ref <name>` | Load the address of a named global variable. |
| `read_reg` | `$dest = read_reg <reg>` | Read a named ABI register into `$dest`. Result type is always `i64`. |
| `inline_asm` | `inline_asm { "line"; ... }` | Emit raw RISC-V lines verbatim. Only ABI-stable registers (sp, fp, ra, a0-a7, s1-s11) may appear; any branches or labels must stay inside the block. |

Field access typically lowers to an `offset` that computes the member pointer, followed
by a `read` or `write`. The optional `+ offset` form on `read` / `write` is equivalent
and folds the byte offset into the access.

### 4.2 Compute

Compute instructions are strongly typed but polymorphic in operation.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| `math` | `$dest = math <op> <type> <lhs>, <rhs>` | `<op>`: `add, sub, mul, div, sdiv, udiv, mod, umod, shl, shr, and, or, xor`. Most ops are bitwise identical for signed and unsigned; only division (`sdiv`/`udiv`) and modulo (`mod`/`umod`) differ. |
| `unary` | `$dest = unary <op> <type> <value>` | `<op>`: `neg`, `not`. |
| `cmp` | `$dest = cmp <cond> <type> <lhs>, <rhs>` | Returns `i1`. `eq` and `ne` are signedness-free; ordering uses `slt, sle, sgt, sge` (signed) or `ult, ule, ugt, uge` (unsigned). There is no signedness-ambiguous `lt`/`le`/`gt`/`ge`. |
| `cast` | `$dest = cast <mode> <value> -> <type>` | `<mode>`: `trunc, zext, sext, bitcast, f2i, i2f`. |

### 4.3 Control flow

Control flow runs between labeled basic blocks. Each block ends in exactly one
terminator.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| `phi` | `$dest = phi <type> [ value, label ], ...` | SSA merge: selects a value by the predecessor block control came from. |
| `jump` | `jump label` | Unconditional jump to a basic block. |
| `branch` | `branch $cond ? true_lbl : false_lbl` | Conditional branch on an `i1` register. |
| `call` | `[$res =] call func(<value>, ...)` | Invoke a function. |
| `indirect_call` | `[$res =] indirect_call fn(...) $callee(<value>, ...)` | Invoke a function pointer with `jalr`-style semantics. |
| `function_addr` | `$dest = function_addr func` | Load a function symbol's code address. |
| `ret` | `ret [$val]` | Return to the caller. |

`jump`, `branch`, and `ret` are terminators; `phi`, `call`, and `indirect_call` are
ordinary instructions.

Aggregate call arguments have value semantics. RV64 lowering passes one pointer to the caller's
aggregate bytes in the next integer ABI argument slot; the callee copies the complete value into
its parameter slot before executing the entry block. The pointer is an ABI detail and does not
change the IR parameter type. Register overflow stores the pointer in the ordinary outgoing stack
argument area.

## 5. Structs and multiple returns

The language has no tuples, so multiple returns are modeled as named structs. Structs are
allocated on the stack and passed or returned by pointer, or embedded directly in a
signature.

```text
; Inline aggregate in the return type
define {i32, i32} divide(i32 $a, i32 $b) {
entry:
    $0 = math sdiv i32 $a, $b
    $1 = math mod i32 $a, $b

    ; Allocate the result struct on the stack
    $result_ptr = stack_alloc {i32, i32}

    ; Write members using their byte offsets
    write i32 $0 @ $result_ptr + 0
    write i32 $1 @ $result_ptr + 4

    ret $result_ptr
}

; A named alias is equivalent
type DivideResult = {i32, i32}

define DivideResult divide_alt(i32 $a, i32 $b) {
entry:
    $0 = math sdiv i32 $a, $b
    $1 = math mod i32 $a, $b

    $result_ptr = stack_alloc DivideResult

    write i32 $0 @ $result_ptr + 0
    write i32 $1 @ $result_ptr + 4

    ret $result_ptr
}
```

Member access relies on byte offsets computed at lowering time.

## 6. Generics and monomorphization

The IR has no native generics; all generic resolution happens in the front end. To keep
monomorphized names collision-free, the IR allows angle brackets `<` and `>` directly in
identifiers. Because those characters are illegal in HLL identifiers, a monomorphized
name such as `Vector<i32>` can never collide with a user-defined type.

```text
type Vector<i32> = {i32*, i64, i64}

define void Vector<i32>.push(Vector<i32>* $vec, i32 $val) {
    ; implementation
}
```

## 7. Full translation example

This shows array indexing, field access via `offset`, and `read` / `write` semantics.

HLL source:

```hll
type Point = { x: f32, y: f32 }

offset_point(points: Point[10]*, idx: i32) {
    @points[idx].x = @points[idx].x + 10.0
}
```

IR output:

```text
type Point = {x: f32, y: f32}

define void offset_point(Point[10]* $points, i32 $idx) {
entry:
    ; index scales by sizeof(Point) to reach points[idx]
    $1 = index Point $points, $idx

    ; offset reaches the x field (byte 0) and read loads it
    $2 = offset f32 $1, 0
    $3 = read f32 @ $2

    ; compute the new value
    $4 = math add f32 $3, 10.0

    ; write it back to the same field location
    write f32 $4 @ $2

    ret
}
```

## 8. Formal grammar (EBNF)

### 8.1 Lexical elements

```ebnf
register    = "$" ( letter { letter | digit | "_" } | digit { digit } );
identifier  = letter { letter | digit | "_" | "." | "<" | ">" };
label       = identifier;
field       = [ identifier ":" ] type;
aggregate   = "{" field { "," field } "}";
type        = "void" | "i1" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64"
            | identifier | aggregate
            | type "*" | type "[" integer "]";
integer     = [ "-" ] digit { digit };
float       = [ "-" ] digit { digit } "." digit { digit };
value       = register | identifier | integer | float | "true" | "false" | "null";
```

### 8.2 Instructions

```ebnf
type_decl     = "type" identifier "=" aggregate;
function_def  = "define" type identifier "(" [ param_list ] ")" "{" { basic_block } "}";
param_list    = type register { "," type register };
basic_block   = label ":" { instruction } terminator;

global_string = "const" identifier "=" "c\"" { any_char - '"' } "\"";

program       = { type_decl } { global_string } { function_def };

instruction   = alloc_inst | heap_alloc_inst | free_inst | read_inst | write_inst
              | index_inst | offset_inst | global_ref_inst | math_inst | unary_inst
              | cmp_inst | cast_inst | call_inst | phi_inst | inline_asm_inst
              | read_reg_inst;

alloc_inst      = register "=" "stack_alloc" type [ integer ];
heap_alloc_inst = register "=" "heap_alloc" type [ "x" value ];
free_inst       = "heap_free" register;

inline_asm_inst = "inline_asm" "{" { '"' { any_char - '"' } '"' ";" } "}";
read_reg_inst   = register "=" "read_reg" abi_reg;
abi_reg         = "sp" | "fp" | "ra"
                | "a0" | "a1" | "a2" | "a3" | "a4" | "a5" | "a6" | "a7"
                | "s1" | "s2" | "s3" | "s4" | "s5" | "s6" | "s7" | "s8" | "s9" | "s10" | "s11";

read_inst       = register "=" "read" type "@" register [ "+" integer ];
write_inst      = "write" type value "@" register [ "+" integer ];

index_inst      = register "=" "index" type register "," value;
offset_inst     = register "=" "offset" type register "," value;
global_ref_inst = register "=" "global_ref" identifier;

math_inst       = register "=" "math" math_op type value "," value;
math_op         = "add" | "sub" | "mul" | "div" | "sdiv" | "udiv" | "mod" | "umod"
                | "shl" | "shr" | "and" | "or" | "xor";

unary_inst      = register "=" "unary" unary_op type value;
unary_op        = "neg" | "not";

cmp_inst        = register "=" "cmp" cmp_op type value "," value;
cmp_op          = "eq" | "ne" | "slt" | "ult" | "sle" | "ule" | "sgt" | "ugt" | "sge" | "uge";

cast_inst       = register "=" "cast" cast_mode value "->" type;
cast_mode       = "trunc" | "zext" | "sext" | "bitcast" | "f2i" | "i2f";

call_inst       = [ register "=" ] "call" identifier "(" [ arg_list ] ")";
indirect_call_inst = [ register "=" ] "indirect_call" type value "(" [ arg_list ] ")";
function_addr_inst = register "=" "function_addr" identifier;
arg_list        = value { "," value };

phi_inst        = register "=" "phi" type phi_arm { "," phi_arm };
phi_arm         = "[" value "," label "]";

terminator      = jump_inst | branch_inst | ret_inst;
jump_inst       = "jump" label;
branch_inst     = "branch" value "?" label ":" label;
ret_inst        = "ret" [ value ];
```
