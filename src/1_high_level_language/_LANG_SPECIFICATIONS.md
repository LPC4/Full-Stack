# Language Specification v1.4.1

**Version:** 1.4.1
**Design Philosophy:** Consistency-First Memory Model  
**Target Domain:** Systems Programming

---

## 1. Core Design Principles

HLL enforces a 100% consistent pointer model. Memory operations are context-independent, explicit, and deterministic. The language eliminates implicit conversions, context-dependent dereferencing, and hidden ownership semantics.

### 1.1 The Four Golden Rules
1. **Pointers are always pointers.** If a type contains `*`, it is a pointer type. No implicit conversions between `T` and `T*`.
2. **Explicit dereferencing with `@`.** `@ptr` reads the value. `@ptr = value` writes the value. Field access requires `@ptr.field`. Array indexing returns pointers.
3. **Explicit address-of with `&`.** `&value` obtains a pointer to a stack variable or array element. `&@ptr` is invalid.
4. **No mutable primitive parameters.** All parameters are pass-by-value. Mutation requires explicit pointer parameters (`T*`).

**Duality Principle:** `@(&x) ≡ x` and `&(@ptr) ≡ ptr` (when `ptr` is a valid stack pointer).

---

## 2. Syntax & Lexical Conventions

| Feature | Rule |
|---------|------|
| Comments | Semicolon `;` (line comment; consumes the rest of the line) |
| Statement Termination | Significant newlines (one statement per line) |
| Whitespace | Insufficient except as token separator |
| Type Annotations | `name: Type = value` |
| Type Casting | Prefix syntax: `target_type(value)` |

### 2.1 Syntax Examples
```HLL
x: i32 = 42
y: f64 = 3.1415
z: i32 = 42 ; Allowed trailing comment

; Multi-line expression continuation
w: i32 = 1 + 2
    + 3

; Explicit casting
ptr: i32* = i32*(1000)
int_val: i32 = i32(ptr)
```

---

## 3. Type System & Declarations

### 3.1 Primitive Types
| Type | Description | Size | Default |
|------|-------------|------|---------|
| `i8`, `i16`, `i32`, `i64` | Signed integers | 1, 2, 4, 8 bytes | `0` |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | 1, 2, 4, 8 bytes | `0` |
| `f32`, `f64` | IEEE 754 floats | 4, 8 bytes | `0.0` |
| `bool` | Boolean | 1 byte | `false` |

**Note:** `Str` is **not** a primitive type. It is defined in the Standard Library as a struct containing a byte pointer and length (`data: u8*`, `length: u64`). String literals (e.g., `"text"`) evaluate to a compile-time inline struct `{ data: u8*, length: u64 }` representing the read-only data pointer and its pre-calculated length.

### 3.2 Declaration & Initialization
```HLL
; Initialized stack variable
count: i32 = 10

; Uninitialized stack array (contains undefined data)
buffer: u8[1024]

; Heap allocation (zero-initialized)
data_ptr: i32* = new(i32)
array_ptr: i32[10]* = new([10]i32)

; Compile-time constant
const MAX_SIZE = 100
```
**Initialization Rules:**
- Stack variables must be initialized unless explicitly declared as uninitialized buffers.
- Heap allocations via `new(T)` are zero-initialized.
- Reading uninitialized stack variables is a compile-time error.

---

## 4. Pointer Semantics

### 4.1 Core Operations
| Operation | Syntax | Type Rule | Example |
|-----------|--------|-----------|---------|
| Read | `@ptr` | `T* → T` | `val: i32 = @x_ptr` |
| Write | `@ptr = val` | `T* ← T` | `@x_ptr = 42` |
| Address-of | `&val` | `T → T*` | `x_ptr: i32* = &x` |
| Allocate | `new(T)` | `void → T*` | `p: Point* = new(Point)` |
| Deallocate | `free(ptr)` | `T* → void` | `free(x_ptr)` |
| Arithmetic | `@(ptr + offset)` | `T* → T` | `byte: u8 = @(raw_ptr + 3)` |
| Cast | `TargetType(val)` | `S → T` | `u8*(ptr)` |

### 4.2 Pointer Arithmetic Constraints
- Offsets using `+` are **always strictly in bytes**.
- Only addition is permitted for dereferencing: `@(ptr + offset)`.
- The array index operator `[]` is the **only** operator that performs type-scaled offsets.
- Pointer subtraction is invalid.
- Pointer-to-pointer arithmetic is invalid.

---

## 5. Composite Types

### 5.1 Arrays
Array indexing **always returns a pointer** (`T*`), never a value.
```HLL
local_arr: i32[5]
@local_arr[0] = 10          ; Write
first: i32 = @local_arr[0]  ; Read

heap_arr: i32* = new(i32, 10)
@heap_arr[3] = 42
value: i32 = @heap_arr[3]

; Array of structs
points: Point* = new(Point, 5)
@points[0] = { x: 1.0, y: 2.0 }
x_val: f32 = @points[0].x
```
**Rules:**
- `arr[index]` yields `T*`. It is syntactic sugar for a type-scaled offset. (e.g., it scales by `sizeof(T)`).
- Explicit `@` required for value operations (`@arr[index]`).
- Raw pointer arithmetic `(ptr + offset)` is strictly byte-scaled.
- Stack arrays: compile-time bounds checking. Heap arrays: no runtime bounds checking.
- Stack arrays cannot decay to pointers. Use `&arr[0]` to obtain a pointer.

### 5.2 Structs
```HLL
type Point = {
    x: f32,
    y: f32
}

p1: Point = { .x = 1.0, .y = 2.0 }
p1.x = 3.0                  ; Stack: direct access

p2_ptr: Point* = new(Point)
@p2_ptr = { .x = 3.0, .y = 4.0 } ; Heap: full struct write
@p2_ptr.x = 5.0             ; Heap: field write (requires @)
```
**Field Access Rules:**
- Stack struct: `struct.field`
- Heap/Stack pointer to struct: `@ptr.field`

### 5.3 Inline Structs & Destructuring
HLL uses inline structs for grouping multiple values, including multiple returns. Inline structs can be assigned directly to variables or unpacked using explicit destructuring.

```HLL
; Inline struct return type
get_coordinates: () -> { x: f32, y: f32 } {
    return { .x = 3.5, .y = 7.2 }
}

main: () -> () {
    ; Option 1: Direct Assignment
    coords = get_coordinates()
    print(coords.x)
    print(coords.y)

    ; Option 2: Struct Destructuring (Typed Punning)
    ; Variables are created matching the exact field names and types of the struct
    { x: f32, y: f32 } = get_coordinates()
    print(x)
}
```

**Partial Destructuring (Discarding Data)**
If you only need specific fields from a struct, you can omit the unwanted fields from the destructuring braces.
```HLL
; Extracts 'value', implicitly discards 'success'
{ value: i32 } = try_operation() 
```

---

## 6. Function Semantics

### 6.1 Parameters & Returns
- All parameters are pass-by-value.
- Mutability requires `T*` parameters and `&` at call site.
- Returning stack addresses (`return &x`) is a compile-time error.
- Multiple returns use struct syntax.

```HLL
increment: (x_ptr: i32*) -> () {
    @x_ptr = @x_ptr + 1
}

main: () -> () {
    x: i32 = 5
    increment(&x)
}

divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { quotient: a / b, remainder: a % b }
}

main: () -> () {
    ; Direct assignment
    s = divide(10, 3)
    print(s.quotient)

    ; Struct destructuring
    { quotient: i32, remainder: i32 } = divide(10, 3)
}
```

---

## 7. Control Flow & Resource Management

### 7.1 Control Flow
- `while`, `if`, `else`, `break`, `continue` are supported.
- No `for` loop syntax (avoids semantic conflicts with comments).

### 7.2 `defer` Statement
- Executes when the enclosing scope exits (LIFO order).
- Arguments are evaluated at declaration, not at execution.
- Cannot contain `return` statements.

```HLL
process_data() {
    file: File* = open("data.bin")
    defer close(file)

    buffer: u8[1024]
    while !eof(file) {
        read(file, buffer, 1024)
        if error_occurred { return }
        process(buffer)
    }
}
```
**Resource Rule:** All heap allocations require matching `free()` or `defer free()`. No garbage collection.

---

## 8. Compile-Time Evaluation & Error Handling

### 8.1 Compile-Time Functions
- Must be pure (no I/O, no side effects).
- Cannot use `defer`, `new`, or `free`.
- Only operate on compile-time known values.

```HLL
const FACTORIAL_10 = compute_factorial(10)
compute_factorial(n: i32): i32 {
    if n <= 1 { return 1 }
    return n * compute_factorial(n - 1)
}
```

### 8.2 Error Handling
- No exceptions. Errors are returned as structs `{ value: T, error: E }`.
- `null` indicates failure for pointer-returning functions.
- Explicit handling required at each call site. Unwanted fields can be ignored via partial destructuring.

```HLL
open_file(path: Str*): { file: File*, error: Str* } {
    if invalid_path(path) { 
        return { file: null, error: make_str("Invalid path") } 
    }
    return { file: new(File), error: null }
}

main: () -> () {
    path: Str* = make_str("data.txt")
    
    ; We omit 'error' from the destructuring to implicitly discard it
    { file: File* } = open_file(path)
    
    if file == null {
        ; Handle failure
    }
}
```

---

## 9. Formal Grammar (EBNF)

### 9.1 Lexical Grammar
```ebnf
ident       = letter { letter | digit | "_" };
integer     = digit { digit };
hex_integer = "0x" hex_digit { hex_digit };
float       = digit { digit } "." digit { digit } [ exponent ];
string      = '"' { any_char - '"' } '"';
comment     = ";" { any_char - newline };
newline     = "\n" | "\r\n";
```

### 9.2 Syntactic Grammar
```ebnf
program        = { declaration };
declaration    = variable_decl | function_decl | type_decl | const_decl;
variable_decl  = identifier [ ":" type ] [ "=" expression ];
type_decl      = "type" identifier "=" type_def;
const_decl     = "const" identifier "=" expression;
type_def       = struct_def | array_def | primitive_type | pointer_type;
struct_def     = "{" { field_decl "," } "}";
field_decl     = identifier ":" type;
array_def      = "[" integer "]" type;
pointer_type   = type "*";
primitive_type = "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
               | "f32" | "f64" | "bool";
function_decl  = identifier ":" "(" [ param_list ] ")" "->" return_type block;
param_list     = parameter { "," parameter };
parameter      = identifier ":" type;
return_type    = type | struct_def;
block          = "{" { statement } "}";
statement      = expression ";" | if_stmt | while_stmt | return_stmt | defer_stmt | variable_decl ";";
if_stmt        = "if" expression block [ "else" ( if_stmt | block ) ];
while_stmt     = "while" expression block;
return_stmt    = "return" [ expression ];
defer_stmt     = "defer" expression;
expression     = assignment | binary_expr | unary_expr | primary_expr;
assignment     = lvalue "=" expression;
lvalue         = struct_destructure | dereference | field_access | array_index | identifier;
struct_destructure = "{" identifier ":" type { "," identifier ":" type } "}";
unary_expr     = unary_op expression;
unary_op       = "-" | "!" | "&" | "@";
primary_expr   = identifier | literal | "(" expression ")" | function_call | array_literal | struct_literal;
struct_literal = "{" field_init { "," field_init } "}";
field_init     = identifier ":" expression;
dereference    = "@" expression;
field_access   = expression "." identifier;
array_index    = expression "[" expression "]";
```

### 9.3 Operator Precedence
| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `() [] .` | Left |
| 2 | `@ & - !` (unary) | Right |
| 3 | `* / %` | Left |
| 4 | `+ -` | Left |
| 5 | `< <= > >=` | Left |
| 6 | `== !=` | Left |
| 7 | `and` | Left |
| 8 | `or` | Left |
| 9 | `=` | Right |

**Validation Rule:** Boolean expressions follow standard operator precedence: `and` binds tighter than `or`.

---

## 10. Memory Safety Framework

### 10.1 Formal Model
- **Pointer Type `T*`:** Set of memory addresses containing values of type `T`.
- **Dereference:** Partial functions `load: T* → T` and `store: T* × T → unit`, valid only for active pointers.
- **No Implicit Conversions:** Type system strictly separates `T` and `T*`. Casting requires explicit syntax.

### 10.2 Error Prevention Matrix
| Error | Traditional Cause | HLL Prevention |
|-------|------------------|------------------|
| Null Dereference | Implicit assumptions | Explicit `@`, compile-time null tracking |
| Use-After-Free | Hidden ownership | Mandatory `free()`, compiler lifetime tracking |
| Buffer Overflow | Implicit bounds | Explicit indexing, stack bounds checks |
| Memory Leak | Untracked allocations | `defer free()`, compiler warnings |
| Dangling Pointers | Implicit copying | No stack address returns, static ownership tracking |
| Data Races | Implicit sharing | All mutation via explicit pointers |

**Theorem:** If a program compiles successfully, it contains no dangling pointer dereferences (single-threaded guarantee). Concurrency requires external synchronization primitives.

---

## 11. Standard Library Reference

### 11.1 Strings
```HLL
type Str = {
    data: u8*,
    length: u64
}

; String literals like "Hello World" evaluate to inline structs: { data: u8*, length: u64 }
make_str: (raw_str: { data: u8*, length: u64 }) -> Str* {
    { data: u8*, length: u64 } = raw_str
    str_ptr: Str* = new(Str)
    @str_ptr = { data: data, length: length }
    return str_ptr
}
```

### 11.2 Vector
```HLL
type Vector<T> = {
    data: T*,
    length: u64,
    capacity: u64
}

new_vector: <T>(initial_capacity: u64) -> Vector<T>* {
    vec: Vector<T>* = new(Vector<T>)
    @vec.length = 0
    @vec.capacity = initial_capacity
    @vec.data = new(T, initial_capacity)
    return vec
}

push: <T>(vec: Vector<T>*, value: T) -> () {
    if @vec.length >= @vec.capacity { resize_vector(vec, @vec.capacity * 2) }
    @vec.data[@vec.length] = value
    @vec.length = @vec.length + 1
}

free_vector: <T>(vec: Vector<T>*) -> () {
    free(@vec.data)
    free(vec)
}
```

### 11.3 Memory Allocators
- **Arena:** `new_arena(size)`, `alloc_in_arena(arena, size)`, `free_arena(arena)`. Batch deallocation.
- **Pool:** `new_pool<T>(count)`, `acquire<T>(pool)`, `release<T>(pool, obj)`. Fixed-size object recycling.

### 11.4 I/O Abstraction
```HLL
type File = { handle: u64, buffer: u8*, buffer_size: u64, buffer_pos: u64, buffer_end: u64 }
open_file: (path: Str*) -> { file: File*, error: Str* }
read_byte: (f: File*) -> { byte: u8, eof: bool }
close_file: (f: File*) -> ()
```
**Rule:** All resource-allocating functions return cleanup routines. Use `defer` for guaranteed release.

---

## 12. Interoperability & FFI

### 12.1 C ABI Compatibility
| HLL | C |
|-------|---|
| `i32` / `u32` | `int` / `unsigned int` |
| `f32` / `f64` | `float` / `double` |
| `bool` | `_Bool` |
| Standard structs | `struct` |
| Function pointers | Function pointers |

### 12.2 Ownership Transfer Protocols
1. **Caller Retains Ownership:** C reads/writes but does not free. HLL calls `free()` later.
2. **Transfer to C:** HLL passes pointer, C assumes ownership. HLL must not call `free()`.
3. **Transfer from C:** C allocates, returns pointer. HLL assumes ownership and must call `free()`.
4. **Shared Reference Counting:** Both sides follow `atomic_increment`/`atomic_decrement` protocol. Last owner frees memory.

### 12.3 FFI Wrapper Pattern
```HLL
external external_compute_sum: (values: f32*, count: i32) -> f32

compute_sum_wrapper: (values: f32*, count: i32) -> f32 {
    return external_compute_sum(values, count)
}
```
**Rule:** Cross-boundary ownership must be explicitly documented. Compiler guarantees do not apply across FFI boundaries.

---

## 13. Compiler Implementation Notes

### 13.1 Optimization Opportunities
- **Alias Analysis:** Explicit pointer operations enable precise tracking.
- **Dead Store Elimination:** Unambiguous write locations.
- **Vectorization:** Array indexing consistently yields pointers, enabling SIMD lane generation.
- **Escape Analysis:** `&` operator explicitly marks escaping variables.

### 13.2 Escape Analysis Rules
1. `&x` marks `x` as potentially escaping.
2. Returning pointers to stack variables is a compile error.
3. Storing stack pointers in heap/global memory marks escape.
4. Functions storing pointer parameters in global state cause escape of pointed-to values.
   **Result:** Non-escaping heap allocations can be promoted to stack allocation.

### 13.3 Debug Symbol Generation
- Source location attached to every `@` and `&` operation.
- Exact type preservation for debugger visualization.
- Lifetime ranges tracked for stack/heap regions.
- Optimization builds maintain semantic equivalence with source-level mapping.

---

## Appendix: Implementation Checklist

1. **Pointer Typing:** `T*` is strictly a pointer. Never auto-dereference.
2. **Dereference Syntax:** All value access requires `@`. `@ptr.field`, `@arr[i]`, `@ptr`.
3. **Address Syntax:** `&` applies only to stack variables/array elements.
4. **Indexing:** `arr[i]` evaluates to `T*`. Read/write requires `@arr[i]`.
5. **Mutability:** Parameters are immutable copies. Use `T*` and `&` for mutation.
6. **Resource Lifecycle:** `new()` requires `free()` or `defer free()`. No GC.
7. **Error Flow:** Functions return `{ value, error }` structs. Handle explicitly.
8. **Precedence:** Parenthesize ambiguous expressions. Compiler rejects implicit precedence.
9. **FFI Boundaries:** Document ownership transfer. Compiler safety does not cross language boundaries.
10. **Compile-Time Functions:** Pure, deterministic, no memory allocation.