# Intermediate Representation Specification v1.4.1

**Version:** 1.4.1  
**Design Philosophy:** Strongly-Typed, Static Single Assignment (SSA), "Fat" High-Level IR  
**Target Domain:** Compiler Backends & Middle-End Optimization

---

## 1. Core Design Principles

This IR is the translation layer between the HLL frontend and the machine-code backend. It bridges HLL's strict, explicit memory model with a machine-agnostic, easily optimizable format.

### 1.1 The Architecture Pillars
1. **Static Single Assignment (SSA):** Every virtual register is assigned exactly once. This guarantees unhindered data-flow analysis and trivial Dead Code Elimination (DCE).
2. **Infinite Virtual Registers:** This IR assumes infinite registers using the `$` prefix. Register allocation is deferred entirely to the target-specific backend.
3. **"Fat" Instruction Set:** This IR utilizes highly expressive, polymorphic instructions. This keeps the IR concise, readable, and semantically rich for high-level optimizations.
4. **Explicit Read/Write:** Virtual registers hold values; memory operations (stack/heap) require explicit `read` and `write` instructions. The `@` sigil is reserved exclusively for memory access.

### 1.2 Aggregate Type Representation
HLL programs define structs with named fields (e.g., `type Point = { x: f32, y: f32 }`). When lowered to IR, these structs are represented as **anonymous inline aggregates** (`{f32, f32}`) because the IR operates on byte offsets rather than field names. Named type aliases may be introduced at the IR level for clarity or to enable certain optimizations, but the canonical IR form is anonymous and field-name-agnostic.

### 1.3 Signedness in Integer Operations
The IR does not distinguish signed from unsigned integer types; all integers are represented by their width (`i8`, `i16`, `i32`, `i64`). Signedness is encoded in the **operation itself**:
- **Division:** `div` performs unsigned division; `sdiv` performs signed division.
- **Comparison:** Each comparison is prefixed with `s` (signed) or `u` (unsigned): `slt` vs `ult`, etc.
- **Type Casting:** `sext` (sign-extend) and `zext` (zero-extend) both cast to a wider type but interpret the source signedness differently.

---

## 2. Syntax & Lexical Conventions

This IR uses a simple, unambiguous text format designed for both ultra-fast parsing and human readability.

| Entity | Syntax | Example |
|--------|--------|---------|
| Virtual Register | `$` prefix | `$1`, `$base_ptr`, `$t0` |
| Basic Block | Bare label + `:` | `entry:`, `loop_header:` |
| Function | Bare name after `define` / `call` | `define i32 compute_sum(...)`, `call compute_sum(...)` |
| Global String | `const` declaration | `const hello = c"hi"` |
| Temporary Reg | Numeric | `$0`, `$1`, `$2` (Compiler generated) |
| Named Reg | Alphanumeric | `$count`, `$value` (Frontend mapped) |
| Comments | Semicolon `;` | `; Calculate offset` |

---

## 3. Type System

This IR heavily mirrors HLL's frontend types to minimize the semantic gap.

| This IR Type | Description |
|----------|-------------|
| `i1`, `i8`, `i16`, `i32`, `i64` | Integers (i1 is boolean). Signedness is encoded in opcodes, not in the type itself. |
| `f32`, `f64` | IEEE 754 Floating point. |
| `T*` | Pointer to type `T`. |
| `T[N]` | Fixed-size array (e.g., `i32[10]`). |
| `{T1, T2, ...}` | Aggregate (struct) type with anonymous inline fields or named alias. |
| `Name` | Explicitly named type definitions or aliases. |

### 3.1 Aggregate Types
Aggregate types (structs) in this IR can be represented in two ways:
- **Named type aliases** at the module level for reusable struct definitions
- **Anonymous inline aggregates** in type expressions (allocations, parameters, returns)

Both forms are equivalent at runtime; the distinction is purely stylistic. Named aliases improve readability for frequently-used types, while inline aggregates are more concise for one-off struct types.

```text
; Named alias (reusable)
type Point = {f32, f32}
type DivideResult = {i32, i32}

; Anonymous inline aggregates (common in function signatures and allocations)
define {i32, i32} divide(i32 $a, i32 $b) { ... }
$ptr = stack_alloc {i32, i32}
```

All field information is stored in the aggregate definition or at the memory-access site via byte offsets; field **names are not preserved** in the IR's type representation to keep the IR lightweight.

---

## 4. Instruction Set Architecture (ISA)

### 4.1 Memory & State Management
Memory instructions in This IR use the `@` syntax to visually mirror HLL's dereferencing rules and explicitly differentiate between type-scaled indexing and byte-scaled offsets.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`stack_alloc`** | `$dest = stack_alloc <type> [count]` | Allocates space on the stack. Returns `type*`. |
| **`heap_alloc`** | `$dest = heap_alloc <type> [count]` | Allocates heap memory. Returns `type*`. Mirrors HLL `new(...)`. |
| **`heap_free`** | `heap_free $ptr` | Frees heap memory. |
| **`read`** | `$dest = read <type> @ $ptr [+ offset]` | Dereferences memory. `offset` is an immediate byte offset. |
| **`write`** | `write <type> <value> @ $ptr [+ offset]`| Writes `<value>` to memory at `$ptr` + `offset` bytes. |
| **`index`**| `$dest = index <type> $base_ptr, $idx` | Array indexing. Scales the offset automatically by `sizeof(type)`. Returns a pointer. |
| **`offset`**| `$dest = offset <type> $base_ptr, $byte_offset`| Raw pointer arithmetic. Scales strictly by 1 byte. Returns a pointer. |

### 4.2 Polymorphic Compute
Compute instructions are strongly typed but polymorphic in operation.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`math`** | `$dest = math <op> <type> <lhs>, <rhs>` | `<op>`: `add, sub, mul, div, sdiv, mod, shl, shr, and, or, xor`. Most ops are bitwise identical for signed and unsigned; only `div` (unsigned) vs `sdiv` (signed) differ. |
| **`unary`** | `$dest = unary <op> <type> <value>` | `<op>`: `neg`, `not`. |
| **`cmp`** | `$dest = cmp <cond> <type> <lhs>, <rhs>`| Returns `i1`. Signedness is explicit in the condition: `slt, sle, sgt, sge` (signed) or `ult, ule, ugt, uge` (unsigned); `eq, ne, lt, le, gt, ge` are deprecated in favor of signed/unsigned variants. |
| **`cast`** | `$dest = cast <mode> <value> -> <type>` | `<mode>`: `trunc, zext, sext, bitcast, f2i, i2f`. Sign/zero-extend modes explicitly specify signedness intent. |

### 4.3 Control Flow & Basic Blocks
Control flow operates strictly between labeled basic blocks.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`jump`** | `jump label` | Unconditional jump to a basic block. |
| **`branch`**| `branch $cond ? true_lbl : false_lbl`| Conditional branch based on an `i1` register. |
| **`call`** | `[$res =] call func(<value>, ...)` | Invokes a function. |
| **`ret`** | `ret [$val]` | Returns control to the caller. |

---

## 5. Structs & Multiple Returns

Since tuples do not exist in the language, multiple returns are handled via explicitly named structs. Structs are allocated on the stack and manipulated via pointers or embedded in function signatures.

**This IR Representation:**

In the canonical form, aggregate types are represented as anonymous inline structs:

```text
; Option 1: inline aggregate in return type
define {i32, i32} divide(i32 $a, i32 $b) {
entry:
    $0 = math sdiv i32 $a, $b
    $1 = math mod i32 $a, $b
    
    ; Allocate an unnamed struct on the stack
    $result_ptr = stack_alloc {i32, i32}
    
    ; Write values into struct memory using baked-in byte offsets
    write i32 $0 @ $result_ptr + 0
    write i32 $1 @ $result_ptr + 4
    
    ret $result_ptr
}

; Option 2: named type alias (equivalent)
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

Field **names are not preserved** in the IR's aggregate type representation; field access relies on **byte offsets** computed at lowering time.

---

## 6. Generics & Monomorphization (Collision-Free)

This IR does not support generic types natively. All generic resolution occurs in the HLL frontend. To prevent naming collisions during monomorphization (e.g., a generic `Vector<T>` colliding with a user-defined struct literally named `Vector_T`), the IR utilizes angle brackets `< >` directly in its identifier grammar for monomorphized types and functions.

Because `< >` characters are inherently illegal in standard HLL identifier names, this guarantees a mathematically collision-free namespace.

**Example Monomorphization:**
```text
; Clean, collision-free IR mangling
type Vector<i32> = {i32*, i64, i64}

define void Vector<i32>.push(Vector<i32>* $vec, i32 $val) {
    ; Implementation
}
```

---

## 7. Full Translation Example

This demonstrates the unified type syntax, the new `index` instruction, and the `@` read/write semantics.

### HLL Source
```HLL
type Point = { x: f32, y: f32 }

offset_point(points: Point[10]*, idx: i32) {
    @points[idx].x = @points[idx].x + 10.0
}
```

### This IR Output
```text
type Point = {f32, f32}

define void offset_point(Point[10]* $points, i32 $idx) {
entry:
    ; Use 'index' for array traversal (scales by sizeof(Point) automatically)
    $1 = index Point $points, $idx
    
    ; Read 'x' field directly using a baked-in offset (x is at byte 0)
    $2 = read f32 @ $1 + 0
    
    ; Compute new value
    $3 = math add f32 $2, 10.0
    
    ; Write back to the 'x' field memory location
    write f32 $3 @ $1 + 0
    
    ret
}
```

---

## 8. Formal Grammar (EBNF) for This IR

### 8.1 Lexical Elements
```ebnf
register    = "$" ( letter { letter | digit | "_" } | digit { digit } );
identifier  = letter { letter | digit | "_" | "." | "<" | ">" };
label       = identifier;
aggregate   = "{" type { "," type } "}";
type        = "i1" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" 
            | identifier | aggregate
            | type "*" | type "[" integer "]";
integer     = [ "-" ] digit { digit };
float       = [ "-" ] digit { digit } "." digit { digit };
value       = register | identifier | integer | float | "true" | "false" | "null";
```

### 8.2 Instructions
```ebnf
type_decl    = "type" identifier "=" "{" type { "," type } "}";
function_def = "define" type identifier "(" [ param_list ] ")" "{" { basic_block } "}";
param_list   = type register { "," type register };
basic_block  = label ":" { instruction } terminator;

global_string = "const" identifier "=" "c\"" { any_char - '"' } "\"";

program      = { type_decl } { global_string } { function_def };

instruction  = alloc_inst | heap_alloc_inst | free_inst | read_inst | write_inst 
             | index_inst | offset_inst | math_inst | unary_inst | cmp_inst | cast_inst | call_inst;

alloc_inst      = register "=" "stack_alloc" type [ integer ];
heap_alloc_inst = register "=" "heap_alloc" type [ integer ];
free_inst       = "heap_free" register;

read_inst    = register "=" "read" type "@" register [ "+" integer ];
write_inst   = "write" type value "@" register [ "+" integer ];

index_inst   = register "=" "index" type register "," value;
offset_inst  = register "=" "offset" type register "," value;

math_inst    = register "=" "math" math_op type value "," value;
math_op      = "add" | "sub" | "mul" | "div" | "sdiv" | "mod" | "shl" | "shr" | "and" | "or" | "xor";

unary_inst   = register "=" "unary" unary_op type value;
unary_op     = "neg" | "not";

cmp_inst     = register "=" "cmp" cmp_op type value "," value;
cmp_op       = "eq" | "ne" | "slt" | "ult" | "sle" | "ule" | "sgt" | "ugt" | "sge" | "uge";

cast_inst    = register "=" "cast" cast_mode value "->" type;
cast_mode    = "trunc" | "zext" | "sext" | "bitcast" | "f2i" | "i2f";

call_inst    = [ register "=" ] "call" identifier "(" [ arg_list ] ")";
arg_list     = value { "," value };

terminator   = jump_inst | branch_inst | ret_inst;
jump_inst    = "jump" label;
branch_inst  = "branch" value "?" label ":" label;
ret_inst     = "ret" [ value ];
```