# Intermediate Representation Specification v1.2

**Version:** 1.2
**Design Philosophy:** Strongly-Typed, Static Single Assignment (SSA), "Fat" High-Level IR  
**Target Domain:** Compiler Backends & Middle-End Optimization

---

## 1. Core Design Principles

This IR is the translation layer between the HLL frontend and the machine-code backend. It bridges HLL's strict, explicit memory model with a machine-agnostic, easily optimizable format. 

### 1.1 The Architecture Pillars
1. **Static Single Assignment (SSA):** Every virtual register is assigned exactly once. This guarantees unhindered data-flow analysis and trivial Dead Code Elimination (DCE).
2. **Infinite Virtual Registers:** This IR assumes infinite registers using the `$` prefix. Register allocation is deferred entirely to the target-specific backend.
3. **"Fat" Instruction Set:** This IR utilizes highly expressive, polymorphic instructions (e.g., baked-in pointer offsets, generic `math` operations). This keeps the IR concise, readable, and semantically rich for high-level optimizations.
4. **Explicit Load/Store:** Virtual registers hold values; memory operations (stack/heap) require explicit `load` and `store` instructions. This translates HLL's `@` operator 1:1.

---

## 2. Syntax & Lexical Conventions

This IR uses a simple, unambiguous text format designed for both ultra-fast parsing and human readability.

| Entity | Syntax | Example |
|--------|--------|---------|
| Virtual Register | `$` prefix | `$1`, `$base_ptr`, `$t0` |
| Basic Block | `@` prefix | `@entry:`, `@loop_header:` |
| Function | `@` prefix | `@compute_sum` |
| Temporary Reg | Numeric | `$0`, `$1`, `$2` (Compiler generated) |
| Named Reg | Alphanumeric | `$count`, `$value` (Frontend mapped) |
| Comments | Semicolon `;` | `; Calculate offset` |

### 2.1 Namespace Separation Guarantee
To prevent naming collisions during lowering, the frontend maps user-defined variables to alphanumeric registers (e.g., `$count`) and reserves purely numeric registers for intermediate SSA calculations (e.g., `$0`, `$1`).

---

## 3. Type System

This IR maps HLL's frontend types to basic IR types.

| This IR Type | Description |
|----------|-------------|
| `i1`, `i8`, `i16`, `i32`, `i64` | Integers (i1 is boolean). Signedness is handled by opcodes, not types. |
| `f32`, `f64` | IEEE 754 Floating point. |
| `T*` | Pointer to type `T`. |
| `{T1, T2, ...}` | Aggregate/Struct types (used for HLL structs and tuples). |
| `[N x T]` | Fixed-size array. |
| `Name` | Named type alias emitted by the frontend (e.g., `Point`). |

This IR supports optional named type declarations for readability and stable layout reuse:

```text
type Point = {f32, f32}
```

Backends may canonicalize named aliases to structural types during lowering.

---

## 4. Instruction Set Architecture (ISA)

### 4.1 Memory & State Management
Memory instructions in This IR are "Fat" — they optionally bake in byte offsets to prevent emitting chains of trivial pointer-arithmetic instructions.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`stack_alloc`** | `$dest = stack_alloc <type> [count]` | Allocates space on the stack. If `count` is provided, allocates an array. Returns `type*`. |
| **`heap_alloc`** | `$dest = heap_alloc <type> [count]` | Allocates heap memory. Returns `type*`. Mirrors HLL `new(...)`. |
| **`heap_free`** | `heap_free $ptr` | Frees heap memory previously returned by `heap_alloc` (or runtime allocator wrappers). |
| **`load`** | `$dest = load <type> $ptr [+ offset]` | Dereferences memory. `offset` is an immediate byte offset. |
| **`store`** | `store <type> <value> -> $ptr [+ offset]`| Writes `<value>` to memory at `$ptr` + `offset` bytes. |
| **`offset`**| `$dest = offset <type> $ptr, <value>` | Pure pointer arithmetic. Returns a new pointer without reading memory. |

### 4.2 Polymorphic Compute
Compute instructions are strongly typed but polymorphic in operation, reducing opcode bloat.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`math`** | `$dest = math <op> <type> <lhs>, <rhs>` | `<op>`: `add, sub, mul, div, sdiv, mod, shl, shr, and, or, xor`. |
| **`unary`** | `$dest = unary <op> <type> <value>` | `<op>`: `neg` (arithmetic), `not` (logical/bitwise). |
| **`cmp`** | `$dest = cmp <cond> <type> <lhs>, <rhs>`| Returns `i1`. `<cond>`: `eq, ne, lt, le, gt, ge` (with `u` or `s` prefixes for integers, e.g., `slt`). |
| **`cast`** | `$dest = cast <mode> <value> -> <type>` | `<mode>`: `trunc`, `zext`, `sext`, `bitcast`, `f2i`, `i2f`. |

### 4.3 Control Flow & Basic Blocks
Control flow operates strictly between labeled Basic Blocks.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`jump`** | `jump @label` | Unconditional jump to a basic block. |
| **`branch`**| `branch $cond ? @true_lbl : @false_lbl`| Conditional branch based on an `i1` register. |
| **`call`** | `[$res =] call @func(<value>, ...)` | Invokes a function. |
| **`ret`** | `ret [$val]` | Returns control to the caller, optionally with a value. |

---

## 5. Aggregates: Structs & Tuples

In This IR, both HLL Structs and Tuples are represented as **Structs in Memory** (`{T1, T2, ...}`). 

Because This IR is a high-level IR, it does not force tuple destructuring into multiple independent virtual registers for function returns. Instead, tuples are allocated on the stack and passed by pointer, or returned as aggregate values.

### 5.1 Tuple Lowering Example
**HLL Source:**
```HLL
divide(a: i32, b: i32): (i32, i32) {
    return {a / b, a % b}
}
```

### 5.2 Nulls and Error-Flow Conventions

This IR supports explicit `null` pointer literals in value positions.

- Pointer-returning failure paths lower to `ret null` (or aggregate forms containing `null`).
- HLL `(value, error)` tuples lower to This IR aggregates where error channels are explicit fields.
- Backends must not assume non-null pointers unless proven by analysis.

**This IR Representation:**
```text
; Tuples are mapped to anonymous structs {i32, i32}
define {i32, i32} @divide(i32 $a, i32 $b) {
@entry:
    $0 = math sdiv i32 $a, $b
    $1 = math mod i32 $a, $b
    
    ; Allocate tuple struct on the stack
    $tuple = stack_alloc {i32, i32}
    
    ; Store values into struct memory using baked-in offsets
    store i32 $0 -> $tuple + 0
    store i32 $1 -> $tuple + 4
    
    ; Load the aggregate and return
    $2 = load {i32, i32} $tuple
    ret $2
}
```

---

## 6. Generics & Monomorphization

This IR **does not support generic types**. All generic resolution occurs in the HLL frontend prior to IR generation.

When a generic struct or function is used in HLL code, the frontend performs **monomorphization**:
1. It duplicates the generic template for the specific concrete type used (e.g., `i32`, `Point`).
2. It mangles the name to ensure uniqueness (e.g., `@Vector_push_i32`).
3. It emits concrete This IR types and instructions.

This guarantees zero runtime overhead for generic dispatch and allows the backend to perform highly specific optimizations (like vectorization) based on the exact memory layouts of the concrete types.

---

## 7. Full Translation Example

This demonstrates how HLL's explicit `@` (dereference) and `&` (address-of) map beautifully to This IR's "Fat" memory instructions.

### HLL Source
```HLL
type Point = { x: f32, y: f32 }

offset_point(points: Point*, index: i32) {
    @points[index].x = @points[index].x + 10.0
}
```

### This IR Output
```text
type Point = {f32, f32}

define void @offset_point(Point* $points, i32 $index) {
@entry:
    ; Calculate array offset (index * 8 bytes per Point)
    $0 = math mul i32 $index, 8
    
    ; Get pointer to the specific struct in the array
    $1 = offset Point* $points, $0
    
    ; Load 'x' field directly using a baked-in offset (x is at byte 0)
    $2 = load f32 $1 + 0
    
    ; Compute new value
    $3 = math add f32 $2, 10.0
    
    ; Store back to the 'x' field memory location
    store f32 $3 -> $1 + 0
    
    ret
}
```

---

## 8. Formal Grammar (EBNF) for This IR

### 8.1 Lexical Elements
```ebnf
register    = "$" ( letter { letter | digit | "_" } | digit { digit } );
label       = "@" letter { letter | digit | "_" };
identifier  = letter { letter | digit | "_" };
type        = "i1" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | identifier
            | type "*" | "{" { type "," } "}" | "[" integer "x" type "]";
integer     = [ "-" ] digit { digit };
float       = [ "-" ] digit { digit } "." digit { digit };
value       = register | integer | float | "true" | "false" | "null";
```

### 8.2 Instructions
```ebnf
type_decl    = "type" identifier "=" type;
function_def = "define" type label "(" [ param_list ] ")" "{" { basic_block } "}";
param_list   = type register { "," type register };
basic_block  = label ":" { instruction } terminator;

program      = { type_decl } { function_def };

instruction  = alloc_inst | heap_alloc_inst | free_inst | load_inst | store_inst | offset_inst
             | math_inst | unary_inst | cmp_inst | cast_inst | call_inst;

alloc_inst   = register "=" "stack_alloc" type [ integer ];
heap_alloc_inst = register "=" "heap_alloc" type [ integer ];
free_inst  = "free" register;
load_inst    = register "=" "load" type register [ "+" integer ];
store_inst   = "store" type value "->" register [ "+" integer ];
offset_inst  = register "=" "offset" type register "," value;

math_inst    = register "=" "math" math_op type value "," value;
math_op      = "add" | "sub" | "mul" | "div" | "sdiv" | "mod" | "shl" | "shr" | "and" | "or" | "xor";

unary_inst   = register "=" "unary" unary_op type value;
unary_op     = "neg" | "not";

cmp_inst     = register "=" "cmp" cmp_op type value "," value;
cmp_op       = "eq" | "ne" | "slt" | "ult" | "sle" | "ule" | "sgt" | "ugt" | "sge" | "uge";

cast_inst    = register "=" "cast" cast_mode value "->" type;
cast_mode    = "trunc" | "zext" | "sext" | "bitcast" | "f2i" | "i2f";

call_inst    = [ register "=" ] "call" label "(" [ arg_list ] ")";
arg_list     = value { "," value };

terminator   = jump_inst | branch_inst | ret_inst;
jump_inst    = "jump" label;
branch_inst  = "branch" value "?" label ":" label;
ret_inst     = "ret" [ value ];
```

---

## Appendix: Compiler Lowering Notes

1. **Defer Statements:** `defer` from HLL does not exist in This IR. The frontend must inject explicit `call` instructions to cleanup routines at every `ret` point.
2. **Compile-Time Functions:** Resolved purely in the HLL frontend. This IR only sees the computed constant literals.
3. **Heap Lifecycle:** HLL `new`/`free` lower to `heap_alloc`/`free` (or runtime allocator wrapper calls with equivalent semantics).
4. **Register Allocation:** Target-specific. Backends (e.g., x86_64, ARM, Wasm) will map `$` virtual registers to physical registers and insert stack spills where `$` count exceeds physical limits.
