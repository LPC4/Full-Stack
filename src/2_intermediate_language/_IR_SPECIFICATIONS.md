# Intermediate Representation Specification v1.4

**Version:** 1.4  
**Design Philosophy:** Strongly-Typed, Static Single Assignment (SSA), "Fat" High-Level IR  
**Target Domain:** Compiler Backends & Middle-End Optimization

---

## 1. Core Design Principles

This IR is the translation layer between the HLL frontend and the machine-code backend. It bridges HLL's strict, explicit memory model with a machine-agnostic, easily optimizable format.

### 1.1 The Architecture Pillars
1. **Static Single Assignment (SSA):** Every virtual register is assigned exactly once. This guarantees unhindered data-flow analysis and trivial Dead Code Elimination (DCE).
2. **Infinite Virtual Registers:** This IR assumes infinite registers using the `$` prefix. Register allocation is deferred entirely to the target-specific backend.
3. **"Fat" Instruction Set:** This IR utilizes highly expressive, polymorphic instructions. This keeps the IR concise, readable, and semantically rich for high-level optimizations.
4. **Explicit Read/Write:** Virtual registers hold values; memory operations (stack/heap) require explicit `read` and `write` instructions, mirroring HLL's explicit `@` duality principle.

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

---

## 3. Type System

This IR heavily mirrors HLL's frontend types to minimize the semantic gap.

| This IR Type | Description |
|----------|-------------|
| `i1`, `i8`, `i16`, `i32`, `i64` | Integers (i1 is boolean). Signedness handled by opcodes. |
| `f32`, `f64` | IEEE 754 Floating point. |
| `T*` | Pointer to type `T`. |
| `T[N]` | Fixed-size array (e.g., `i32[10]`). |
| `Name` | Explicitly named type definitions. |

### 3.1 Named Struct Definitions
Anonymous inline structs are not permitted. All structs must be explicitly named at the top level of the IR module, matching HLL definitions.

```text
type Point = {f32, f32}
type DivideResult = {i32, i32}
```

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
| **`math`** | `$dest = math <op> <type> <lhs>, <rhs>` | `<op>`: `add, sub, mul, div, sdiv, mod, shl, shr, and, or, xor`. |
| **`unary`** | `$dest = unary <op> <type> <value>` | `<op>`: `neg`, `not`. |
| **`cmp`** | `$dest = cmp <cond> <type> <lhs>, <rhs>`| Returns `i1`. `<cond>`: `eq, ne, lt, le, gt, ge` (with `u` or `s` prefixes). |
| **`cast`** | `$dest = cast <mode> <value> -> <type>` | `<mode>`: `trunc`, `zext`, `sext`, `bitcast`, `f2i`, `i2f`. |

### 4.3 Control Flow & Basic Blocks
Control flow operates strictly between labeled Basic Blocks.

| Instruction | Syntax | Description |
|-------------|--------|-------------|
| **`jump`** | `jump @label` | Unconditional jump to a basic block. |
| **`branch`**| `branch $cond ? @true_lbl : @false_lbl`| Conditional branch based on an `i1` register. |
| **`call`** | `[$res =] call @func(<value>, ...)` | Invokes a function. |
| **`ret`** | `ret [$val]` | Returns control to the caller. |

---

## 5. Structs & Multiple Returns

Since tuples do not exist in the language, multiple returns are handled via explicitly named structs. Structs are allocated on the stack and manipulated via pointers.

**This IR Representation:**
```text
type DivideResult = {i32, i32}

define DivideResult @divide(i32 $a, i32 $b) {
@entry:
    $0 = math sdiv i32 $a, $b
    $1 = math mod i32 $a, $b
    
    $result_ptr = stack_alloc DivideResult
    
    ; Write values into struct memory using baked-in byte offsets
    write i32 $0 @ $result_ptr + 0
    write i32 $1 @ $result_ptr + 4
    
    $2 = read DivideResult @ $result_ptr
    ret $2
}
```

---

## 6. Generics & Monomorphization (Collision-Free)

This IR does not support generic types natively. All generic resolution occurs in the HLL frontend. To prevent naming collisions during monomorphization (e.g., a generic `Vector<T>` colliding with a user-defined struct literally named `Vector_T`), the IR utilizes angle brackets `< >` directly in its identifier grammar for monomorphized types and functions.

Because `< >` characters are inherently illegal in standard HLL identifier names, this guarantees a mathematically collision-free namespace.

**Example Monomorphization:**
```text
; Clean, collision-free IR mangling
type Vector<i32> = {i32*, u64, u64}

define void @Vector<i32>.push(Vector<i32>* $vec, i32 $val) {
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

define void @offset_point(Point[10]* $points, i32 $idx) {
@entry:
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
label       = "@" letter { letter | digit | "_" | "." | "<" | ">" };
identifier  = letter { letter | digit | "_" | "." | "<" | ">" };
type        = "i1" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | identifier
            | type "*" | type "[" integer "]";
integer     = [ "-" ] digit { digit };
float       = [ "-" ] digit { digit } "." digit { digit };
value       = register | integer | float | "true" | "false" | "null";
```

### 8.2 Instructions
```ebnf
type_decl    = "type" identifier "=" "{" type { "," type } "}";
function_def = "define" type label "(" [ param_list ] ")" "{" { basic_block } "}";
param_list   = type register { "," type register };
basic_block  = label ":" { instruction } terminator;

program      = { type_decl } { function_def };

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

call_inst    = [ register "=" ] "call" label "(" [ arg_list ] ")";
arg_list     = value { "," value };

terminator   = jump_inst | branch_inst | ret_inst;
jump_inst    = "jump" label;
branch_inst  = "branch" value "?" label ":" label;
ret_inst     = "ret" [ value ];
```