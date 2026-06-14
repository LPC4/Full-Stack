# HLL Language Specification

HLL is a small systems language built around a consistency-first memory model. Memory
operations are explicit, context-independent, and deterministic: there are no implicit
conversions, no context-dependent dereferencing, and no hidden ownership. This document
defines the syntax, type system, and semantics that the `hll-to-ir` front end implements.

## 1. Core design principles

HLL enforces a fully consistent pointer model.

### 1.1 The four golden rules

1. Pointers are always pointers. If a type contains `*`, it is a pointer type. There are
   no implicit conversions between `T` and `T*`.
2. Dereference explicitly with `@`. `@ptr` reads the value, `@ptr = value` writes it, and
   field access uses `@ptr.field`. Array indexing returns pointers.
3. Take an address explicitly with `&`. `&identifier` is a pointer to a stack variable, and
   stack-safe lvalues such as `&arr[index]` are also valid. `&@ptr` is invalid.
4. No mutable primitive parameters. All parameters are pass-by-value; mutation requires an
   explicit pointer parameter (`T*`).

The duality principle: `@(&x)` equals `x` when `x` is a stack-safe lvalue. The reverse is
not a blanket identity, and `&@ptr` is rejected.

## 2. Syntax and lexical conventions

| Feature | Rule |
|---------|------|
| Comments | Semicolon `;` (line comment; consumes the rest of the line) |
| Statement termination | Significant newlines (one statement per line) |
| Whitespace | Insignificant except as a token separator |
| Type annotations | `name: Type = value` |
| Type casting | Postfix `as`: `expr as TargetType`. The prefix form `TargetType(value)` is also accepted. |

### 2.1 Syntax examples

```hll
x: i32 = 42
y: f64 = 3.1415
z: i32 = 42        ; trailing comment

; multi-line expression continuation
w: i32 = 1 + 2
    + 3

; explicit casting
ptr: i32* = i32*(1000)
int_val: i32 = i32(ptr)
```

## 3. Type system and declarations

### 3.1 Primitive types

| Type | Description | Size | Default |
|------|-------------|------|---------|
| `i8`, `i16`, `i32`, `i64` | Signed integers | 1, 2, 4, 8 bytes | `0` |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | 1, 2, 4, 8 bytes | `0` |
| `f32`, `f64` | IEEE 754 floats | 4, 8 bytes | `0.0` |
| `bool` | Boolean | 1 byte | `false` |

`Str` is not a primitive. It is a standard-library struct holding a byte pointer and a
length (`data: u8*`, `length: u64`). A string literal such as `"text"` evaluates to a
compile-time anonymous inline struct with the exact shape `{ data: u8*, length: u64 }`.
Anonymous inline structs are allowed anywhere a struct type is accepted.

### 3.2 Declaration and initialization

```hll
; Initialized stack variable
count: i32 = 10

; Uninitialized stack array (contains undefined data)
buffer: u8[1024]

; Heap allocation (zero-initialized)
data_ptr: i32* = new(i32)
array_ptr: i32* = new(i32, 10)

; Compile-time constant
const MAX_SIZE = 100
```

Heap allocation rules:

- `new(T)` allocates a single element of type `T` and returns `T*`.
- `new(T, N)` allocates `N` contiguous elements of type `T` and returns `T*`. `N` may be a
  runtime expression.
- `new([N]T)` has been removed; use `new(T, N)` instead.
- All heap allocations are zero-initialized.

String literals: `"text"` evaluates to a compile-time inline struct `{ data: u8*, length: u64 }`
equivalent to `Str`, so `s: Str = "hello"` is valid with no wrapper call.

Character literals: `'c'` is an integer literal equal to the ascii byte of `c` (default
type `i32`, like any integer literal), so `putc('A')` and `putc(65)` are identical. Escapes
`\n \t \r \b \0 \\ \' \"` are recognized; the body must be exactly one ascii character.

Initialization rules: stack variables must be initialized unless declared as an
uninitialized buffer, and reading an uninitialized stack variable is a compile-time error.

## 4. Pointer semantics

### 4.1 Core operations

| Operation | Syntax | Type rule | Example |
|-----------|--------|-----------|---------|
| Read | `@ptr` | `T* -> T` | `val: i32 = @x_ptr` |
| Write | `@ptr = val` | `T* <- T` | `@x_ptr = 42` |
| Address-of | `&val` | `T -> T*` | `x_ptr: i32* = &x` |
| Allocate | `new(T)` | `void -> T*` | `p: Point* = new(Point)` |
| Deallocate | `free(ptr)` | `T* -> void` | `free(x_ptr)` |
| Arithmetic | `@(ptr + offset)` | `T* -> T` | `byte: u8 = @(raw_ptr + 3)` |
| Cast | `val as TargetType` | `S -> T` | `ptr as u8*` |

### 4.2 Pointer arithmetic constraints

- Offsets using `+` are always strictly in bytes.
- Only addition is permitted for dereferencing: `@(ptr + offset)`.
- The array index operator `[]` is the only operator that performs type-scaled offsets.
- Pointer subtraction is invalid.
- Pointer-to-pointer arithmetic is invalid.

## 5. Composite types

### 5.1 Arrays

Array indexing always returns a pointer (`T*`), never a value.

```hll
local_arr: i32[5]
@local_arr[0] = 10          ; write
first: i32 = @local_arr[0]  ; read

heap_arr: i32* = new(i32, 10)
@heap_arr[3] = 42
value: i32 = @heap_arr[3]

; Array of structs
points: Point* = new(Point, 5)
@points[0] = { x: 1.0, y: 2.0 }
x_val: f32 = @points[0].x
```

Rules:

- `arr[index]` yields `T*`. It is sugar for a type-scaled offset (it scales by `sizeof(T)`).
- An explicit `@` is required for value operations (`@arr[index]`).
- Raw pointer arithmetic `(ptr + offset)` is strictly byte-scaled.
- Stack arrays get compile-time bounds checking; heap arrays get no runtime bounds checking.
- Stack arrays cannot decay to pointers; use `&arr[0]` to obtain a pointer.

### 5.2 Structs

```hll
type Point = {
    x: f32,
    y: f32
}

p1: Point = { .x = 1.0, .y = 2.0 }
p1.x = 3.0                  ; stack: direct access

p2_ptr: Point* = new(Point)
@p2_ptr = { .x = 3.0, .y = 4.0 } ; heap: full struct write
@p2_ptr.x = 5.0             ; heap: field write (requires @)
```

Struct type rule: fields are comma-separated; commas are required between fields and a
trailing comma is allowed for multi-line definitions.

Struct literal rule: anonymous inline structs may be used anywhere a struct type is
accepted. Literals support shorthand field initialization with `{ .field = expr }` and,
where an explicit annotation helps, the typed form `{ field: Type = expr }`.

Field access rules:

- Stack struct: `struct.field`.
- Heap or stack pointer to struct: `@ptr.field`.

### 5.3 Inline structs and destructuring

HLL uses anonymous inline structs to group multiple values, including multiple returns.
They are allowed anywhere a struct type is accepted: declarations, parameters, return
types, type aliases, and intermediate expressions. An inline struct can be assigned
directly to a variable or unpacked with explicit destructuring.

```hll
; Inline struct return type
get_coordinates: () -> { x: f32, y: f32 } {
    return { .y = 7.2, .x = 3.5 }
}

main: () -> () {
    ; Option 1: direct assignment
    coords = get_coordinates()
    print(coords.x)
    print(coords.y)

    ; Option 2: struct destructuring (typed pattern)
    ; Fields are matched by name, not position, so the order may differ from the source.
    { y: f32, x: f32 } = get_coordinates()
    print(x)
}
```

Partial destructuring discards data: list only the fields you need and omit the rest. The
listed field order does not need to match the source struct.

```hll
; Extracts 'value', implicitly discards 'success'
{ value: i32 } = try_operation()
```

## 6. Function semantics

### 6.1 Parameters and returns

- All parameters are pass-by-value.
- Mutability requires a `T*` parameter and `&` at the call site.
- Returning a stack address (`return &x`) is a compile-time error.
- Multiple returns use anonymous inline struct syntax.
- Void return: omit the `->` clause entirely. The `-> ()` form is accepted for
  compatibility but deprecated; prefer `name: () { }` over `name: () -> () { }`.

### 6.2 Module system

Modules are file-scoped: each source file is one module.

```hll
import "path/to/module"   ; declare a module dependency
export fn_name: () { }    ; mark a declaration as visible to importers
```

- `import` records the module path for the linker; the pipeline resolves it.
- `export` marks a declaration as publicly visible. Declarations without `export` are
  private by convention (the linker still sees them; the keyword documents intent and
  enables future enforcement).
- A module implicitly imports the `core` builtins (primitive types, `new`, `free`,
  `defer`, `asm`).
- Cyclic imports are rejected at compile time.

```hll
increment: (x_ptr: i32*) -> () {
    @x_ptr = @x_ptr + 1
}

main: () -> () {
    x: i32 = 5
    increment(&x)
}

divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { remainder: a % b, quotient: a / b }
}

main: () -> () {
    ; Direct assignment
    s = divide(10, 3)
    print(s.quotient)

    ; Struct destructuring by name, with fields listed in any order
    { remainder: i32, quotient: i32 } = divide(10, 3)
}
```

## 7. Control flow and resource management

### 7.1 Control flow

- `while`, `if`, `else`, `break`, and `continue` are supported.
- There is no `for` loop syntax (it would conflict semantically with comments).

### 7.2 The defer statement

- A deferred call runs when the enclosing function exits, in LIFO order.
- Arguments are captured at declaration time, not at execution. Mutating a captured
  variable afterward does not affect the deferred call.
- A defer statement cannot contain a `return`.

```hll
; defer captures the value of `ptr` at this line, not when the function exits.
defer free(ptr)
ptr = new(i32)   ; reassigning ptr does not affect the deferred free
```

```hll
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

Resource rule: every heap allocation needs a matching `free()` or `defer free()`. There is
no garbage collection.

### 7.3 Inline assembly

Two forms of inline assembly emit raw RISC-V instructions or read hardware registers
directly from HLL. They exist only for low-level system code (`_start`, syscall wrappers,
and similar); application code should not need them.

`asm_reg(name)` is a register-read expression. It reads the current value of a named ABI
register as an `i64` and is valid anywhere an expression is accepted, including conditions
and arithmetic.

```hll
stack_ok: () -> bool {
    return asm_reg(sp) > 0x10000
}

get_sp: () -> i64 {
    return asm_reg(sp)
}
```

`asm { }` is a verbatim assembly block. It emits raw RISC-V instruction lines interleaved
with the surrounding compiled code. Each line is one instruction, whitespace-delimited and
terminated by a newline or semicolon.

```hll
putchar: (c: i32) -> i32 {
    asm {
        addi  sp, sp, -16
        sd    ra, 8(sp)
        sb    a0, 7(sp)
        li    a0, 1
        addi  a1, sp, 7
        li    a2, 1
        li    a7, 64
        ecall
        ld    ra, 8(sp)
        addi  sp, sp, 16
    }
    return 0
}

_start: () {
    asm {
        call main
        li   a7, 93
        ecall
    }
}
```

Allowed registers (both forms): `sp`, `fp`, `ra`, `gp`, `tp`, `a0`-`a7`, `s1`-`s11`.

- `fp` must be spelled `fp`; the `s0` alias is not accepted by name.
- `gp` (global pointer) and `tp` (thread pointer) are available for OS-level use.
- Temp registers `t0`-`t6` are not allowed. The register allocator may hold live values in
  them at any asm site, so clobbering them would silently corrupt surrounding code.

Restrictions on `asm { }` blocks:

- No HLL variables or expressions inside; raw assembly text only.
- No data directives (`.asciz`, `.word`, and so on); use HLL string or array literals for data.
- Branches and labels within a block are permitted but must not target labels outside it.
- Blocks cannot be nested.

## 8. Compile-time evaluation and error handling

### 8.1 Compile-time functions

- Must be pure (no I/O, no side effects).
- Cannot use `defer`, `new`, or `free`.
- Operate only on compile-time known values.

```hll
const FACTORIAL_10 = compute_factorial(10)
compute_factorial(n: i32): i32 {
    if n <= 1 { return 1 }
    return n * compute_factorial(n - 1)
}
```

### 8.2 Error handling

- There are no exceptions. Errors are returned as structs `{ value: T, error: E }`.
- `null` indicates failure for pointer-returning functions.
- Handling is explicit at each call site. Unwanted fields can be dropped via partial
  destructuring, which matches by name rather than position.

```hll
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

## 9. Formal grammar (EBNF)

### 9.1 Lexical grammar

```ebnf
ident       = letter { letter | digit | "_" };
integer     = digit { digit };
hex_integer = "0x" hex_digit { hex_digit };
float       = digit { digit } "." digit { digit } [ exponent ];
string      = '"' { any_char - '"' } '"';
comment     = ";" { any_char - newline };
newline     = "\n" | "\r\n";
```

### 9.2 Syntactic grammar

```ebnf
program        = { declaration };
declaration    = variable_decl | function_decl | type_decl | const_decl
               | import_decl | export_decl;
import_decl    = "import" string;
export_decl    = "export" declaration;
variable_decl  = identifier [ ":" type ] [ "=" expression ];
type_decl      = "type" identifier "=" type;
const_decl     = "const" identifier "=" expression;
type           = primitive_type | identifier | struct_def | array_def | pointer_type;
struct_def     = "{" [ field_decl { "," field_decl } [ "," ] ] "}";
field_decl     = identifier ":" type;
array_def      = "[" integer "]" type;
pointer_type   = type "*";
primitive_type = "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
               | "f32" | "f64" | "bool";
function_decl  = identifier ":" "(" [ param_list ] ")" [ "->" return_type ] block;
param_list     = parameter { "," parameter };
parameter      = identifier ":" type;
return_type    = type;
block          = "{" { statement } "}";
statement      = expression | if_stmt | while_stmt | return_stmt | defer_stmt
               | variable_decl | asm_block;
if_stmt        = "if" expression block [ "else" ( if_stmt | block ) ];
while_stmt     = "while" expression block;
return_stmt    = "return" [ expression ];
defer_stmt     = "defer" expression;
asm_block      = "asm" "{" { asm_line } "}";
asm_line       = { any_char - newline } newline;
expression     = assignment | binary_expr | cast_expr | unary_expr | postfix_expr;
cast_expr      = expression "as" type;
assignment     = lvalue ( "=" | compound_op ) expression;
compound_op    = "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=";
lvalue         = struct_destructure | dereference | field_access | array_index | identifier;
struct_destructure = "{" [ identifier ":" type { "," identifier ":" type } [ "," ] ] "}";
unary_expr     = unary_op expression;
unary_op       = "-" | "!" | "&" | "@";
postfix_expr   = primary_expr { "." identifier | "[" expression "]" | "as" type };
primary_expr   = identifier | literal | "(" expression ")" | function_call | array_literal
               | struct_literal | new_expr | asm_reg_expr;
new_expr       = "new" "(" type [ "," expression ] ")";
asm_reg_expr   = "asm_reg" "(" abi_reg ")";
abi_reg        = "sp" | "fp" | "ra" | "gp" | "tp"
               | "a0" | "a1" | "a2" | "a3" | "a4" | "a5" | "a6" | "a7"
               | "s1" | "s2" | "s3" | "s4" | "s5" | "s6" | "s7" | "s8" | "s9" | "s10" | "s11";
struct_literal = "{" [ field_init { "," field_init } [ "," ] ] "}";
field_init     = shorthand_field_init | typed_field_init;
shorthand_field_init = "." identifier "=" expression;
typed_field_init = identifier ":" type "=" expression;
dereference    = "@" expression;
field_access   = expression "." identifier;
array_index    = expression "[" expression "]";
```

### 9.3 Operator precedence

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `() [] . as` | Left |
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
| 13 | `=` | Right |

Precedence is absolute: parentheses are required to override the default binding, and the
parser rejects ambiguous expressions rather than guessing intent.

The `as` and `@` interaction is a common trap. Because `as` is at level 1 (postfix) and `@`
is at level 2 (prefix), `@x as T` parses as `@(x as T)` (cast first, then dereference). To
dereference and then cast, use outer parentheses: `(@x) as T`.

```hll
; WRONG: parses as @(ptr as i32) - dereferencing an i32!
val: i32 = @ptr as i32

; CORRECT: dereference the u8*, then cast the resulting u8 to i32
val: i32 = (@ptr) as i32
```

Evaluation order: binary expressions, function arguments, and struct literals are evaluated
strictly left to right. `defer` cleanup runs in LIFO order at the enclosing scope exit.

### 9.4 Compound assignment

`lhs OP= rhs` is shorthand for `lhs = lhs OP rhs`, available for `+= -= *= /= %=` and the
bitwise/shift forms `&= |= ^= <<= >>=`. The right-hand side is the whole expression, so
`x -= a + b` means `x = x - (a + b)`. The left-hand side is evaluated twice (once to read,
once to write); HLL lvalues are simple (identifiers, fields, dereferenced array elements)
with no side effects, so this is observationally equivalent to a single evaluation.

## 10. Memory safety framework

### 10.1 Formal model

- Pointer type `T*` is the set of memory addresses holding values of type `T`.
- Dereference is the partial functions `load: T* -> T` and `store: T* x T -> unit`, valid
  only for active pointers.
- There are no implicit conversions; the type system strictly separates `T` and `T*`, and
  casting requires explicit syntax.

### 10.2 Error prevention matrix

| Error | Traditional cause | HLL prevention |
|-------|-------------------|----------------|
| Null dereference | Implicit assumptions | Explicit `@`, compile-time null tracking |
| Use-after-free | Hidden ownership | Mandatory `free()`, compiler lifetime tracking |
| Buffer overflow | Implicit bounds | Explicit indexing, stack bounds checks |
| Memory leak | Untracked allocations | `defer free()`, compiler warnings |
| Dangling pointers | Implicit copying | No stack-address returns, static ownership tracking |
| Data races | Implicit sharing | All mutation via explicit pointers |

If a program compiles successfully, it contains no dangling pointer dereferences (a
single-threaded guarantee). Concurrency requires external synchronization primitives.

## 11. Standard library reference

### 11.1 Strings

```hll
type Str = {
    data: u8*,
    length: u64
}

; String literals like "Hello World" evaluate to compile-time anonymous inline
; structs with the exact shape: { data: u8*, length: u64 }
make_str: (raw_str: { data: u8*, length: u64 }) -> Str* {
    { data: u8*, length: u64 } = raw_str
    str_ptr: Str* = new(Str)
    @str_ptr = { data: data, length: length }
    return str_ptr
}
```

### 11.2 Vector

```hll
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

### 11.3 Memory allocators

- Arena: `new_arena(size)`, `alloc_in_arena(arena, size)`, `free_arena(arena)` for batch
  deallocation.
- Pool: `new_pool<T>(count)`, `acquire<T>(pool)`, `release<T>(pool, obj)` for fixed-size
  object recycling.

### 11.4 I/O abstraction

```hll
type File = { handle: u64, buffer: u8*, buffer_size: u64, buffer_pos: u64, buffer_end: u64 }
open_file: (path: Str*) -> { file: File*, error: Str* }
read_byte: (f: File*) -> { byte: u8, eof: bool }
close_file: (f: File*) -> ()
```

Resource-allocating functions return cleanup routines; use `defer` for guaranteed release.

## 12. Interoperability and FFI

### 12.1 C ABI compatibility

| HLL | C |
|-----|---|
| `i32` / `u32` | `int` / `unsigned int` |
| `f32` / `f64` | `float` / `double` |
| `bool` | `_Bool` |
| Standard structs | `struct` |
| Function pointers | Function pointers |

### 12.2 Ownership transfer protocols

1. Caller retains ownership: C reads and writes but does not free; HLL calls `free()` later.
2. Transfer to C: HLL passes a pointer and C assumes ownership; HLL must not call `free()`.
3. Transfer from C: C allocates and returns a pointer; HLL assumes ownership and must call
   `free()`.
4. Shared reference counting: both sides follow an `atomic_increment` / `atomic_decrement`
   protocol, and the last owner frees the memory.

### 12.3 FFI wrapper pattern

```hll
external external_compute_sum: (values: f32*, count: i32) -> f32

compute_sum_wrapper: (values: f32*, count: i32) -> f32 {
    return external_compute_sum(values, count)
}
```

Cross-boundary ownership must be documented explicitly; compiler guarantees do not apply
across FFI boundaries.

## 13. Compiler implementation notes

### 13.1 Optimization opportunities

- Alias analysis: explicit pointer operations enable precise tracking.
- Dead-store elimination: write locations are unambiguous.
- Vectorization: array indexing consistently yields pointers, enabling SIMD lane generation.
- Escape analysis: the `&` operator explicitly marks escaping variables.

### 13.2 Escape analysis rules

1. `&x` marks `x` as potentially escaping.
2. Returning a pointer to a stack variable is a compile error.
3. Storing a stack pointer in heap or global memory marks an escape.
4. Functions that store pointer parameters in global state cause the pointed-to values to
   escape. Non-escaping heap allocations can then be promoted to stack allocations.

### 13.3 Debug symbol generation

- A source location is attached to every `@` and `&` operation.
- Types are preserved exactly for debugger visualization.
- Lifetime ranges are tracked for stack and heap regions.
- Optimized builds maintain semantic equivalence with a source-level mapping.

## Appendix D: HLL-0 (the self-hosting subset)

HLL-0 is the deliberately tiny subset the in-VM compiler `cc` (PLAN 1.2) accepts. It is
*not* a separate language: every HLL-0 program is also a valid HLL program except for the
one I/O intrinsic below. The point is to make a naive, self-hostable compiler tractable, so
HLL-0 drops everything that needs a type checker or heap: there is one numeric type, no
pointers, structs, arrays, floats, casts, `defer`, or inline `asm`.

### D.1 Types

Only `i32`. Every local, parameter, and return value is `i32`; arithmetic wraps modulo 2^32
(two's complement). There is no `bool`: comparisons yield `i32` `0`/`1`, and `if`/`while`
conditions test "non-zero".

### D.2 Program shape

A program is a list of function definitions. Execution starts at `main: () -> i32`; the
`i32` it returns becomes the process exit code. Functions take zero or more `i32`
parameters and return `i32`.

```hll
name: (p0: i32, p1: i32) -> i32 {
    ; statements
}
```

### D.3 Statements

| Statement | Form |
|-----------|------|
| Local declaration | `name: i32 = expr` |
| Assignment | `name = expr` |
| Conditional | `if expr { ... }` (no `else` in HLL-0) |
| Loop | `while expr { ... }` |
| Return | `return expr` |
| Expression statement | a bare call, e.g. `putc(10)` |

`break`/`continue`/`defer` are out of HLL-0 scope.

### D.4 Expressions

Integer and `'c'` char literals, parameter/local identifiers, function calls `f(a, b)`, the
binary operators `+ - * / %`, and the comparisons `< <= > >= == !=`. Comparisons produce
`0`/`1`. A char literal is its ascii byte (escapes `\n \t \r \0` recognized), so `putc('A')`
equals `putc(65)`. Operator precedence follows §9.3 (multiplicative above additive above
comparison); parenthesize anything ambiguous.

### D.5 I/O intrinsic

`putc(ch: i32)` writes the low byte of `ch` to file descriptor 1. It is the only intrinsic;
`cc` lowers it to a `write(1, &ch, 1)` ecall (a7=64) rather than a real call. All other
output is built from `putc`. Process exit is `main`'s return value, lowered to an exit ecall
(a7=93). This is the whole "ecall-based I/O" surface of HLL-0.

### D.6 Codegen target

`cc` emits naive stack-machine RISC-V in the subset the in-VM assembler `/bin/as` covers
(PLAN §1.1): every local occupies a stack slot, operands are reloaded before each use,
arguments pass in `a0..a7`, and each function keeps `ra` in its frame across calls. The
frozen reference pair is `user/fixtures/hello.hll` (source) and `user/fixtures/hello.s`
(the exact assembly `cc` must produce). `hello.hll` spells `putc` out as an inline-asm
function so the source also compiles and runs on the host toolchain;
`kernel_cc_target_roundtrips` runs the host-compiled source and the `/bin/as`-assembled
hand-written target side by side and checks they behave identically.

The in-VM compiler now exists: `user/bin/cc.hll` (installed at `/bin/cc.elf`, OS spec 10.4)
parses this subset and emits the stack-machine assembly described here. Because cc treats
`putc` as the built-in I/O intrinsic and emits the helper itself, its input omits the
inline-asm `putc` definition (`user/examples/cc_demo.hll` is the pure-HLL-0 sample);
`kernel_cc_compiles_and_runs` exercises the full in-VM `cc` -> `as` -> run toolchain.
