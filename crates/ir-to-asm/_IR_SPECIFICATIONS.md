# Intermediate Representation Specification

This document specifies IR 2.0, the typed intermediate representation between the HLL front end
and the RISC-V back end. The front end (`hll-to-ir`) builds an `IrProgram`; the back end
(`ir-to-asm`) consumes that structure directly. Its textual form is the canonical human-readable
dump used by diagnostics, tests, goldens, the IDE, and the Guide.

## 1. Design principles

1. Static single assignment for virtual registers. Every temporary and parameter register is
   assigned once. Mutable HLL locals remain in explicit stack slots until mem2reg is implemented.
2. Infinite virtual registers. Registers use a `$` prefix and are allocated by the target back
   end only after IR optimization.
3. A typed, high-level instruction set. Operations retain enough type and layout information for
   target-independent analysis and readable dumps.
4. Explicit memory access. Values live in virtual registers; `load` and `store` are the only
   ordinary memory reads and writes.
5. One visible opcode per instruction. Internal Rust groupings such as `Math`, `Unary`, and
   `Cast` do not appear as wrapper words in the text.
6. Deterministic output. Stable names, ordering, spacing, and escaping make textual goldens useful
   assertions rather than approximate snapshots.

### 1.1 Aggregate representation

HLL structs preserve their field names in IR aggregate types:

```text
type Point = { x: f32, y: f32 }
```

Anonymous fields are legal and print as types without a name:

```text
type DivideResult = { i32, i32 }
```

Member access still resolves to a byte offset before the backend. Field names exist for type
identity and readability; `offset`, `load`, and `store` carry the executable operation.

### 1.2 Signedness

Integer types record width but not signedness. Signedness belongs to the operation:

- Division and remainder use `sdiv` / `mod` or `udiv` / `umod`.
- Ordered comparisons use `slt`, `sle`, `sgt`, `sge` or their `u` forms.
- Widening uses `sext` or `zext`.
- Operations whose bits are identical for signed and unsigned values use one opcode.

The plain `div` opcode remains part of the model, but HLL lowering selects `sdiv` or `udiv`.

## 2. Lexical conventions

| Entity | Syntax | Example |
| --- | --- | --- |
| Virtual register | `$` prefix | `$0`, `$value`, `$base_ptr` |
| Basic block | Bare identifier and `:` | `entry:`, `while_cond_0:` |
| Function or global | Bare link name | `call sum($a, $b)`, `global_ref counter` |
| Compiler temporary | Numeric register | `$0`, `$1`, `$2` |
| Named value | Identifier register | `$count`, `$result` |
| Comment | Semicolon to end of line | `; aggregate return uses sret` |

IR output uses four spaces for instructions and terminators inside a basic block. Identifiers may
contain dots and angle-bracketed monomorphization components because those characters cannot
collide with HLL source identifiers.

String contents use C-style escapes for backslash, quote, newline, tab, and carriage return.
Inline assembly strings escape backslash and quote. Source-line markers are internal metadata and
do not appear in the canonical textual dump.

Generated comments retain useful source information that instructions cannot express directly.
In particular, `; local var: name` and `; inferred local var: name` mark HLL bindings before their
stack slots. Field-layout and ABI comments are also retained; comments that merely say
`assignment` or restate a control-flow opcode are omitted.

## 3. Types

| Type | Meaning |
| --- | --- |
| `void` | No returned value. |
| `i1`, `i8`, `i16`, `i32`, `i64` | Width-only integer; `i1` is boolean. |
| `f32`, `f64` | IEEE 754 floating point. |
| `T*` | Pointer to `T`. |
| `T[N]` | Fixed-size array of `N` elements. |
| `T[]` | Slice fat pointer containing data pointer and length. |
| `{ name: T, U }` | Aggregate with named or anonymous fields. |
| `fn(T, U) -> R` | Function pointer returning `R`. |
| `fn(T, U)` | Function pointer returning `void`. |
| `Name` | Named type or alias. |

Postfix types compose from the inside out. `i8*[4]` is an array of four pointers, while `i8[4]*`
is a pointer to an array.

## 4. Top-level declarations

### 4.1 Type aliases

```text
type Point = { x: f32, y: f32 }
type Buffer = i8[256]
```

Aliases may name any IR type, although HLL normally emits them for structs, enums, and concrete
generic specializations.

### 4.2 Extern functions

An extern is a complete typed declaration:

```text
extern fmt_subslice(i8[], i64, i64) -> i8[]
extern log(i32) -> void
```

Parameter names do not appear because extern parameters are not values in the current module.
The backend uses the return type to select integer, floating-point, aggregate, or sret call ABI.

### 4.3 Global strings

```text
const greeting = c"hello\n"
```

The name denotes the global string data when used as an IR value.

### 4.4 Global variables

```text
global counter: i64 = zero
export global flags: i8[4] = bytes[0x00; 4]
global pair: i8[2] = bytes[0x01, 0x02]
```

`zero` means zero-initialized storage and normally lowers to `.bss`. `bytes[...]` is an explicit
initializer and normally lowers to `.data`. A repeated byte may use `byte; count`; otherwise every
byte is listed in target memory order. The declared type determines size and alignment.

`export` makes a defined function or global visible to other objects. Its absence means the symbol
is module-private.

### 4.5 Functions and blocks

```text
export define add($lhs: i32, $rhs: i32) -> i32 {
entry:
    $sum = add i32 $lhs, $rhs
    ret $sum
}
```

Every function prints an explicit return type, including `-> void`. Each block has one label and
at most one terminator. Valid lowered functions end every reachable block in exactly one
terminator.

## 5. Instructions

### 5.1 Allocation and memory

| Instruction | Syntax | Meaning |
| --- | --- | --- |
| Stack allocation | `$dest = stack_alloc T[, count]` | Allocate one or `count` elements in the current frame; result is `T*`. |
| Heap allocation | `$dest = heap_alloc T[, count]` | Allocate zeroed heap storage; count may be a runtime value. |
| Heap free | `heap_free $ptr` | Release a heap allocation. |
| Load | `$dest = load T, $ptr[ + bytes]` | Read `T` from the pointer plus an immediate byte offset. |
| Store | `store T value, $ptr[ + bytes]` | Write `T` to the pointer plus an immediate byte offset. |
| Index | `$dest = index T $base, index` | Add `index * sizeof(T)` and return `T*`. |
| Offset | `$dest = offset T $base, bytes` | Add an unscaled byte count and return `T*`. |
| Global address | `$dest = global_ref name` | Load a global variable's address. |
| ABI register | `$dest = read_reg reg` | Read an allowed ABI register as `i64`. |

An omitted allocation count means one element. Stack counts are compile-time unsigned integers;
heap counts may also be virtual registers.

`offset` and `index` accept dynamic values. The optional offset folded into `load` and `store` is
an immediate signed byte count.

### 5.2 Arithmetic and bit operations

```text
$sum = add i32 $lhs, $rhs
$quotient = sdiv i64 $value, 10
$mask = and i64 $bits, 255
$negative = neg i32 $value
$inverted = bitnot i32 $value
```

Binary operations use:

```text
add sub mul div sdiv udiv mod umod shl shr and or xor
```

Unary operations use:

```text
neg not bitnot
```

`not` is logical negation (`value == 0`); `bitnot` complements every bit.

### 5.3 Comparisons

```text
$same = cmp eq i32 $lhs, $rhs
$before = cmp slt i64 $lhs, $rhs
```

Comparisons return `i1`. Conditions are:

```text
eq ne slt ult sle ule sgt ugt sge uge
```

Equality is signedness-independent. There are no ambiguous `lt`, `le`, `gt`, or `ge` forms.

### 5.4 Casts

```text
$wide = sext $small to i64
$bits = bitcast $float to i32
```

Cast opcodes are `trunc`, `zext`, `sext`, `bitcast`, `f2i`, and `i2f`. The source type comes from
the typed value; the target type appears after `to`.

### 5.5 Calls and addresses

```text
$result = call add($lhs, $rhs)
call log($result)
$callback = function_addr add
$result = indirect_call $callback($lhs, $rhs) : fn(i32, i32) -> i32
```

Direct calls obtain their signature from a local definition or extern declaration. Indirect calls
carry the complete function-pointer type after the invocation. A call returning `void` omits the
destination.

Aggregate arguments have value semantics. RV64 lowering passes a pointer to the caller's bytes in
the next integer ABI argument location, and the callee copies the value into its parameter slot.
The pointer is an ABI detail and does not change the IR parameter type.

### 5.6 Phi nodes

```text
$value = phi i32 [ $from_then, if_then_0 ], [ $from_else, if_else_1 ]
```

A phi selects the incoming value associated with the predecessor block. The backend lowers phis
to edge copies with parallel-assignment semantics. Conditional edges may route through generated
trampoline labels so copies execute only on the selected edge.

The HLL front end does not yet emit organic phi nodes. Mutable locals remain in stack slots. See
`.ai/plan/PHI_PLAN.md` for the separate mem2reg roadmap.

### 5.7 Inline assembly

```text
inline_asm { "mv a0, s1"; "ecall"; }
```

Lines are emitted verbatim after string unescaping. Only ABI-stable registers (`sp`, `fp`, `ra`,
`a0` through `a7`, and `s1` through `s11`) may appear. Labels and branches must remain inside the
block.

## 6. Terminators

| Terminator | Syntax | Meaning |
| --- | --- | --- |
| Jump | `jump target` | Continue in one block. |
| Branch | `branch condition ? then : else` | Select a successor using an `i1` value. |
| Return | `ret` or `ret value` | Return to the caller. |
| Trap | `trap code` | Abort with a diagnostic code and no successor. |

`jump`, `branch`, `ret`, and `trap` end a block. Phi and call instructions do not.

## 7. Translation example

HLL:

```hll
type Point = { x: f32, y: f32 }

shift_x: (point: Point*, shift: f32) -> f32 {
    point.x = point.x + shift
    return point.x
}
```

IR 2.0:

```text
type Point = { x: f32, y: f32 }

define shift_x($point: Point*, $shift: f32) -> f32 {
entry:
    $point_ptr = stack_alloc Point*
    store Point* $point, $point_ptr
    $shift_ptr = stack_alloc f32
    store f32 $shift, $shift_ptr
    $0 = load Point*, $point_ptr
    $1 = offset f32 $0, 0
    $2 = load Point*, $point_ptr
    $3 = load f32, $2 + 0
    $4 = load f32, $shift_ptr
    $5 = add f32 $3, $4
    store f32 $5, $1
    $6 = load Point*, $point_ptr
    $7 = load f32, $6 + 0
    ret $7
}
```

The stack slots preserve mutable HLL binding semantics. Numeric registers are SSA temporaries.

## 8. Formal grammar (EBNF)

The grammar describes canonical output. `identifier` includes implementation-generated generic
components; a consumer must balance nested `<...>` components rather than split at their commas.

### 8.1 Lexical elements

```ebnf
letter          = "A".."Z" | "a".."z" | "_";
digit           = "0".."9";
hex_digit       = digit | "a".."f" | "A".."F";
register        = "$" ( letter { letter | digit } | digit { digit } );
identifier      = letter { letter | digit | "." | "_" | generic_component };
generic_component = "<" identifier_or_type { "," identifier_or_type } ">";
identifier_or_type = identifier | type;
label           = identifier;
unsigned        = digit { digit };
integer         = [ "-" ] unsigned;
exponent        = ( "e" | "E" ) [ "+" | "-" ] unsigned;
float           = [ "-" ] unsigned ( "." unsigned [ exponent ] | exponent )
                | "inf" | "-inf" | "nan";
hex_byte        = "0x" hex_digit hex_digit;
string          = '"' { escaped_char | any_char_except_quote_or_backslash } '"';
c_string        = "c" string;
value           = register | identifier | integer | float | "true" | "false" | "null";
```

### 8.2 Types

```ebnf
primitive_type  = "void" | "i1" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64";
field           = [ identifier ":" ] type;
aggregate       = "{" [ field { "," field } ] "}";
function_type   = "fn" "(" [ type { "," type } ] ")" [ "->" type ];
primary_type    = primitive_type | identifier | aggregate | function_type;
type            = primary_type { "*" | "[" unsigned "]" | "[]" };
```

An omitted function-type return means `void`. Function definitions and externs always print an
explicit return type.

### 8.3 Top-level program

```ebnf
type_decl       = "type" identifier "=" type;
extern_decl     = "extern" identifier "(" [ type { "," type } ] ")" "->" type;
string_decl     = "const" identifier "=" c_string;
byte_init       = "bytes" "[" [ hex_byte [ ";" unsigned ]
                                | hex_byte { "," hex_byte } ] "]";
global_init     = "zero" | byte_init;
global_decl     = [ "export" ] "global" identifier ":" type "=" global_init;
param           = register ":" type;
function_def    = [ "export" ] "define" identifier
                  "(" [ param { "," param } ] ")" "->" type
                  "{" { basic_block } "}";
basic_block     = label ":" { instruction } [ terminator ];
program         = { type_decl } { extern_decl } { string_decl | global_decl } { function_def };
```

### 8.4 Instructions

```ebnf
stack_alloc     = register "=" "stack_alloc" type [ "," unsigned ];
heap_alloc      = register "=" "heap_alloc" type [ "," value ];
heap_free       = "heap_free" register;
load            = register "=" "load" type "," register [ "+" integer ];
store           = "store" type value "," register [ "+" integer ];
index           = register "=" "index" type register "," value;
offset          = register "=" "offset" type register "," value;
global_ref      = register "=" "global_ref" identifier;
read_reg        = register "=" "read_reg" abi_reg;

binary_op       = "add" | "sub" | "mul" | "div" | "sdiv" | "udiv"
                | "mod" | "umod" | "shl" | "shr" | "and" | "or" | "xor";
binary          = register "=" binary_op type value "," value;
unary_op        = "neg" | "not" | "bitnot";
unary           = register "=" unary_op type value;
cmp_op          = "eq" | "ne" | "slt" | "ult" | "sle" | "ule"
                | "sgt" | "ugt" | "sge" | "uge";
compare         = register "=" "cmp" cmp_op type value "," value;
cast_op         = "trunc" | "zext" | "sext" | "bitcast" | "f2i" | "i2f";
cast            = register "=" cast_op value "to" type;

argument_list   = value { "," value };
call            = [ register "=" ] "call" identifier "(" [ argument_list ] ")";
indirect_call   = [ register "=" ] "indirect_call" value
                  "(" [ argument_list ] ")" ":" function_type;
function_addr   = register "=" "function_addr" identifier;
phi_arm         = "[" value "," label "]";
phi             = register "=" "phi" type phi_arm { "," phi_arm };
inline_asm      = "inline_asm" "{" { string ";" } "}";
comment         = ";" { any_char_except_newline };

instruction     = stack_alloc | heap_alloc | heap_free | load | store | index | offset
                | global_ref | read_reg | binary | unary | compare | cast | call
                | indirect_call | function_addr | phi | inline_asm | comment;
```

```ebnf
abi_reg         = "sp" | "fp" | "ra"
                | "a0" | "a1" | "a2" | "a3" | "a4" | "a5" | "a6" | "a7"
                | "s1" | "s2" | "s3" | "s4" | "s5" | "s6"
                | "s7" | "s8" | "s9" | "s10" | "s11";
```

### 8.5 Terminators

```ebnf
jump            = "jump" label;
branch          = "branch" value "?" label ":" label;
return          = "ret" [ value ];
trap            = "trap" unsigned;
terminator      = jump | branch | return | trap;
```
