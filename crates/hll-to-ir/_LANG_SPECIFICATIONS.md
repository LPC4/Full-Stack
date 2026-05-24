## Language Specification v1.4.3

**Design Philosophy:** Consistency-First Memory Model  
**Target Domain:** Systems Programming

---

## 1. Core Design Principles

HLL enforces a 100% consistent pointer model. Memory operations are context-independent, explicit, and deterministic. The language eliminates implicit conversions, context-dependent dereferencing, and hidden ownership semantics.

### 1.1 The Four Golden Rules
1. **Pointers are always pointers.** If a type contains `*`, it is a pointer type. No implicit conversions between `T` and `T*`.
2. **Explicit dereferencing with `@`.** `@ptr` reads the value. `@ptr = value` writes the value. Field access requires `@ptr.field`. Array indexing returns pointers.
3. **Explicit address-of with `&`.** `&identifier` obtains a pointer to a stack variable, and stack-safe lvalues such as `&arr[index]` are also valid. `&@ptr` is invalid.
4. **No mutable primitive parameters.** All parameters are pass-by-value. Mutation requires explicit pointer parameters (`T*`).

**Duality Principle:** `@(&x)  x` when `x` is a stack-safe lvalue. The reverse form is not a blanket identity; `&@ptr` is rejected.

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
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">42</span>
<span style="color:#abb2bf">y</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f64</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">3.1415</span>
<span style="color:#abb2bf">z</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">42</span> <span style="color:#7f848e;font-style:italic">; Allowed trailing comment</span>

<span style="color:#7f848e;font-style:italic">; Multi-line expression continuation</span>
<span style="color:#abb2bf">w</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">1</span> <span style="color:#56b6c2">+</span> <span style="color:#98c379">2</span>
    <span style="color:#56b6c2">+</span> <span style="color:#98c379">3</span>

<span style="color:#7f848e;font-style:italic">; Explicit casting</span>
<span style="color:#abb2bf">ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">*(</span><span style="color:#98c379">1000</span><span style="color:#56b6c2">)</span>
<span style="color:#abb2bf">int_val</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">ptr</span><span style="color:#56b6c2">)</span></code></pre>

---

## 3. Type System & Declarations

### 3.1 Primitive Types
| Type | Description | Size | Default |
|------|-------------|------|---------|
| `i8`, `i16`, `i32`, `i64` | Signed integers | 1, 2, 4, 8 bytes | `0` |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | 1, 2, 4, 8 bytes | `0` |
| `f32`, `f64` | IEEE 754 floats | 4, 8 bytes | `0.0` |
| `bool` | Boolean | 1 byte | `false` |

**Note:** `Str` is **not** a primitive type. It is defined in the Standard Library as a struct containing a byte pointer and length (`data: u8*`, `length: u64`). String literals (e.g., `"text"`) evaluate to a compile-time anonymous inline struct with the exact shape `{ data: u8*, length: u64 }`, representing the read-only data pointer and its pre-calculated length. Anonymous inline structs are allowed anywhere a struct type is accepted.

### 3.2 Declaration & Initialization
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#7f848e;font-style:italic">; Initialized stack variable</span>
<span style="color:#abb2bf">count</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">10</span>

<span style="color:#7f848e;font-style:italic">; Uninitialized stack array (contains undefined data)</span>
<span style="color:#abb2bf">buffer</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">[</span><span style="color:#98c379">1024</span><span style="color:#56b6c2">]</span>

<span style="color:#7f848e;font-style:italic">; Heap allocation (zero-initialized)</span>
<span style="color:#abb2bf">data_ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">i32</span><span style="color:#56b6c2">)</span>
<span style="color:#abb2bf">array_ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">[</span><span style="color:#98c379">10</span><span style="color:#56b6c2">]*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">([</span><span style="color:#98c379">10</span><span style="color:#56b6c2">]</span><span style="color:#e5c07b">i32</span><span style="color:#56b6c2">)</span>

<span style="color:#7f848e;font-style:italic">; Compile-time constant</span>
<span style="color:#c678dd">const</span> <span style="color:#abb2bf">MAX_SIZE</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">100</span></code></pre>
**Initialization Rules:**
- Stack variables must be initialized unless explicitly declared as uninitialized buffers.
- Heap allocations via `new(T)` are zero-initialized.
- Reading uninitialized stack variables is a compile-time error.

---

## 4. Pointer Semantics

### 4.1 Core Operations
| Operation | Syntax | Type Rule | Example |
|-----------|--------|-----------|---------|
| Read | `@ptr` | `T* -> T` | `val: i32 = @x_ptr` |
| Write | `@ptr = val` | `T* <- T` | `@x_ptr = 42` |
| Address-of | `&val` | `T -> T*` | `x_ptr: i32* = &x` |
| Allocate | `new(T)` | `void -> T*` | `p: Point* = new(Point)` |
| Deallocate | `free(ptr)` | `T* -> void` | `free(x_ptr)` |
| Arithmetic | `@(ptr + offset)` | `T* -> T` | `byte: u8 = @(raw_ptr + 3)` |
| Cast | `TargetType(val)` | `S -> T` | `u8*(ptr)` |

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
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#abb2bf">local_arr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">[</span><span style="color:#98c379">5</span><span style="color:#56b6c2">]</span>
<span style="color:#56b6c2">@</span><span style="color:#abb2bf">local_arr</span><span style="color:#56b6c2">[</span><span style="color:#98c379">0</span><span style="color:#56b6c2">]</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">10</span>          <span style="color:#7f848e;font-style:italic">; Write</span>
<span style="color:#abb2bf">first</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">local_arr</span><span style="color:#56b6c2">[</span><span style="color:#98c379">0</span><span style="color:#56b6c2">]</span>  <span style="color:#7f848e;font-style:italic">; Read</span>

<span style="color:#abb2bf">heap_arr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">i32</span><span style="color:#56b6c2">,</span> <span style="color:#98c379">10</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">@</span><span style="color:#abb2bf">heap_arr</span><span style="color:#56b6c2">[</span><span style="color:#98c379">3</span><span style="color:#56b6c2">]</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">42</span>
<span style="color:#abb2bf">value</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">heap_arr</span><span style="color:#56b6c2">[</span><span style="color:#98c379">3</span><span style="color:#56b6c2">]</span>

<span style="color:#7f848e;font-style:italic">; Array of structs</span>
<span style="color:#abb2bf">points</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Point</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">Point</span><span style="color:#56b6c2">,</span> <span style="color:#98c379">5</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">@</span><span style="color:#abb2bf">points</span><span style="color:#56b6c2">[</span><span style="color:#98c379">0</span><span style="color:#56b6c2">]</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#98c379">1.0</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">y</span><span style="color:#56b6c2">:</span> <span style="color:#98c379">2.0</span> <span style="color:#56b6c2">}</span>
<span style="color:#abb2bf">x_val</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">points</span><span style="color:#56b6c2">[</span><span style="color:#98c379">0</span><span style="color:#56b6c2">].</span><span style="color:#abb2bf">x</span></code></pre>
**Rules:**
- `arr[index]` yields `T*`. It is syntactic sugar for a type-scaled offset. (e.g., it scales by `sizeof(T)`).
- Explicit `@` required for value operations (`@arr[index]`).
- Raw pointer arithmetic `(ptr + offset)` is strictly byte-scaled.
- Stack arrays: compile-time bounds checking. Heap arrays: no runtime bounds checking.
- Stack arrays cannot decay to pointers. Use `&arr[0]` to obtain a pointer.

### 5.2 Structs
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">type</span> <span style="color:#e5c07b">Point</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span><span style="color:#56b6c2">,</span>
    <span style="color:#abb2bf">y</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span>
<span style="color:#56b6c2">}</span>

<span style="color:#abb2bf">p1</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Point</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">1.0</span><span style="color:#56b6c2">,</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">y</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">2.0</span> <span style="color:#56b6c2">}</span>
<span style="color:#abb2bf">p1</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">3.0</span>                  <span style="color:#7f848e;font-style:italic">; Stack: direct access</span>

<span style="color:#abb2bf">p2_ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Point</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">Point</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">@</span><span style="color:#abb2bf">p2_ptr</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">3.0</span><span style="color:#56b6c2">,</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">y</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">4.0</span> <span style="color:#56b6c2">}</span> <span style="color:#7f848e;font-style:italic">; Heap: full struct write</span>
<span style="color:#56b6c2">@</span><span style="color:#abb2bf">p2_ptr</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">5.0</span>             <span style="color:#7f848e;font-style:italic">; Heap: field write (requires @)</span></code></pre>
**Struct Type Rule:** Struct type fields are comma-separated. Commas are required between fields; trailing commas are allowed for multiline definitions.

**Struct Literal Rule:** Anonymous inline structs may be used anywhere a struct type is accepted. Struct literals support shorthand field initialization with `{ .field = expr }` and, where an explicit annotation is useful, the typed form `{ field: Type = expr }`.

**Field Access Rules:**
- Stack struct: `struct.field`
- Heap/Stack pointer to struct: `@ptr.field`

### 5.3 Inline Structs & Destructuring
HLL uses anonymous inline structs for grouping multiple values, including multiple returns. Anonymous inline structs are allowed anywhere a struct type is accepted: variable declarations, parameters, return types, type aliases, and intermediate expressions. Inline structs can be assigned directly to variables or unpacked using explicit destructuring.

<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#7f848e;font-style:italic">; Inline struct return type</span>
<span style="color:#61afef">get_coordinates</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">y</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">return</span> <span style="color:#56b6c2">{</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">y</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">7.2</span><span style="color:#56b6c2">,</span> <span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">3.5</span> <span style="color:#56b6c2">}</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">main</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#7f848e;font-style:italic">; Option 1: Direct Assignment</span>
    <span style="color:#abb2bf">coords</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">get_coordinates</span><span style="color:#56b6c2">()</span>
    <span style="color:#61afef">print</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">coords</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">x</span><span style="color:#56b6c2">)</span>
    <span style="color:#61afef">print</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">coords</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">y</span><span style="color:#56b6c2">)</span>

    <span style="color:#7f848e;font-style:italic">; Option 2: Struct Destructuring (typed pattern)</span>
    <span style="color:#7f848e;font-style:italic">; Field names are matched by name, not position, so the order may differ from the source struct</span>
    <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">y</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">get_coordinates</span><span style="color:#56b6c2">()</span>
    <span style="color:#61afef">print</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">x</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span></code></pre>

**Partial Destructuring (Discarding Data)**
If you only need specific fields from a struct, you can omit the unwanted fields from the destructuring braces. Omitted fields are discarded, and the listed field order does not need to match the source struct.
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#7f848e;font-style:italic">; Extracts 'value', implicitly discards 'success'</span>
<span style="color:#56b6c2">{</span> <span style="color:#abb2bf">value</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">try_operation</span><span style="color:#56b6c2">()</span></code></pre>

---

## 6. Function Semantics

### 6.1 Parameters & Returns
- All parameters are pass-by-value.
- Mutability requires `T*` parameters and `&` at call site.
- Returning stack addresses (`return &x`) is a compile-time error.
- Multiple returns use anonymous inline struct syntax.

<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#61afef">increment</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#abb2bf">x_ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">x_ptr</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">x_ptr</span> <span style="color:#56b6c2">+</span> <span style="color:#98c379">1</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">main</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">x</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">5</span>
    <span style="color:#61afef">increment</span><span style="color:#56b6c2">(&</span><span style="color:#abb2bf">x</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">divide</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#abb2bf">a</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">b</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">quotient</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">remainder</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">return</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">remainder</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">a</span> <span style="color:#56b6c2">%</span> <span style="color:#abb2bf">b</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">quotient</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">a</span> <span style="color:#56b6c2">/</span> <span style="color:#abb2bf">b</span> <span style="color:#56b6c2">}</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">main</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#7f848e;font-style:italic">; Direct assignment</span>
    <span style="color:#abb2bf">s</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">divide</span><span style="color:#56b6c2">(</span><span style="color:#98c379">10</span><span style="color:#56b6c2">,</span> <span style="color:#98c379">3</span><span style="color:#56b6c2">)</span>
    <span style="color:#61afef">print</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">s</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">quotient</span><span style="color:#56b6c2">)</span>

    <span style="color:#7f848e;font-style:italic">; Struct destructuring by name, with fields listed in any order</span>
    <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">remainder</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">quotient</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">divide</span><span style="color:#56b6c2">(</span><span style="color:#98c379">10</span><span style="color:#56b6c2">,</span> <span style="color:#98c379">3</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span></code></pre>

---

## 7. Control Flow & Resource Management

### 7.1 Control Flow
- `while`, `if`, `else`, `break`, `continue` are supported.
- No `for` loop syntax (avoids semantic conflicts with comments).

### 7.2 `defer` Statement
- Executes when the enclosing scope exits (LIFO order).
- Arguments are evaluated at declaration, not at execution.
- Cannot contain `return` statements.

<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#61afef">process_data</span><span style="color:#56b6c2">()</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">open</span><span style="color:#56b6c2">(</span><span style="color:#98c379">"data.bin"</span><span style="color:#56b6c2">)</span>
    <span style="color:#c678dd">defer</span> <span style="color:#61afef">close</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">file</span><span style="color:#56b6c2">)</span>

    <span style="color:#abb2bf">buffer</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">[</span><span style="color:#98c379">1024</span><span style="color:#56b6c2">]</span>
    <span style="color:#c678dd">while</span> <span style="color:#56b6c2">!</span><span style="color:#61afef">eof</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">file</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
        <span style="color:#61afef">read</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">file</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">buffer</span><span style="color:#56b6c2">,</span> <span style="color:#98c379">1024</span><span style="color:#56b6c2">)</span>
        <span style="color:#c678dd">if</span> <span style="color:#abb2bf">error_occurred</span> <span style="color:#56b6c2">{</span> <span style="color:#c678dd">return</span> <span style="color:#56b6c2">}</span>
        <span style="color:#61afef">process</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">buffer</span><span style="color:#56b6c2">)</span>
    <span style="color:#56b6c2">}</span>
<span style="color:#56b6c2">}</span></code></pre>
**Resource Rule:** All heap allocations require matching `free()` or `defer free()`. No garbage collection.

### 7.3 Inline Assembly

Two forms of inline assembly let you emit raw RISC-V instructions or read hardware registers directly from HLL. They exist exclusively for low-level system code (`_start`, syscall wrappers, etc.); application code should not need them.

**`asm_reg(name)` - register read expression**

Reads the current value of a named ABI register as an `i64`. Valid anywhere an expression is accepted, including conditions and arithmetic.

```hll
stack_ok: () -> bool {
    return asm_reg(sp) > 0x10000
}

get_sp: () -> i64 {
    return asm_reg(sp)
}
```

**`asm { }` - verbatim assembly block**

A statement that emits raw RISC-V instruction lines interleaved with surrounding compiled code. Each line is one instruction (whitespace-delimited, newline or semicolon terminated).

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

**Allowed registers (both forms):** `sp`, `fp`/`s0`, `ra`, `a0`-`a7`, `s1`-`s11`.

Temp registers `t0`-`t6` are **not allowed** - the register allocator may hold live values in them at any asm site; clobbering them would silently corrupt surrounding compiled code.

**Restrictions on `asm { }` blocks:**
- No HLL variables or expressions inside - raw assembly text only.
- No data directives (`.asciz`, `.word`, ...) - use HLL string/array literals for data.
- Branches and labels within a block are permitted; they must not target labels outside the block.
- Cannot be nested.

---

## 8. Compile-Time Evaluation & Error Handling

### 8.1 Compile-Time Functions
- Must be pure (no I/O, no side effects).
- Cannot use `defer`, `new`, or `free`.
- Only operate on compile-time known values.

<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">const</span> <span style="color:#abb2bf">FACTORIAL_10</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">compute_factorial</span><span style="color:#56b6c2">(</span><span style="color:#98c379">10</span><span style="color:#56b6c2">)</span>
<span style="color:#61afef">compute_factorial</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">n</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">):</span> <span style="color:#e5c07b">i32</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">if</span> <span style="color:#abb2bf">n</span> <span style="color:#56b6c2">&lt;=</span> <span style="color:#98c379">1</span> <span style="color:#56b6c2">{</span> <span style="color:#c678dd">return</span> <span style="color:#98c379">1</span> <span style="color:#56b6c2">}</span>
    <span style="color:#c678dd">return</span> <span style="color:#abb2bf">n</span> <span style="color:#56b6c2">*</span> <span style="color:#61afef">compute_factorial</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">n</span> <span style="color:#56b6c2">-</span> <span style="color:#98c379">1</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span></code></pre>

### 8.2 Error Handling
- No exceptions. Errors are returned as structs `{ value: T, error: E }`.
- `null` indicates failure for pointer-returning functions.
- Explicit handling required at each call site. Unwanted fields can be ignored via partial destructuring, and destructuring matches fields by name rather than position.

<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#61afef">open_file</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">path</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*):</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">error</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">if</span> <span style="color:#61afef">invalid_path</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">path</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span> 
        <span style="color:#c678dd">return</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#98c379">null</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">error</span><span style="color:#56b6c2">:</span> <span style="color:#61afef">make_str</span><span style="color:#56b6c2">(</span><span style="color:#98c379">"Invalid path"</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">}</span> 
    <span style="color:#56b6c2">}</span>
    <span style="color:#c678dd">return</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">File</span><span style="color:#56b6c2">),</span> <span style="color:#abb2bf">error</span><span style="color:#56b6c2">:</span> <span style="color:#98c379">null</span> <span style="color:#56b6c2">}</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">main</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">path</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">make_str</span><span style="color:#56b6c2">(</span><span style="color:#98c379">"data.txt"</span><span style="color:#56b6c2">)</span>
    
    <span style="color:#7f848e;font-style:italic">; We omit 'error' from the destructuring to implicitly discard it</span>
    <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">=</span> <span style="color:#61afef">open_file</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">path</span><span style="color:#56b6c2">)</span>
    
    <span style="color:#c678dd">if</span> <span style="color:#abb2bf">file</span> <span style="color:#56b6c2">==</span> <span style="color:#98c379">null</span> <span style="color:#56b6c2">{</span>
        <span style="color:#7f848e;font-style:italic">; Handle failure</span>
    <span style="color:#56b6c2">}</span>
<span style="color:#56b6c2">}</span></code></pre>

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
type_decl      = "type" identifier "=" type;
const_decl     = "const" identifier "=" expression;
type           = primitive_type | identifier | struct_def | array_def | pointer_type;
struct_def     = "{" [ field_decl { "," field_decl } [ "," ] ] "}";
field_decl     = identifier ":" type;
array_def      = "[" integer "]" type;
pointer_type   = type "*";
primitive_type = "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
               | "f32" | "f64" | "bool";
function_decl  = identifier ":" "(" [ param_list ] ")" "->" return_type block;
param_list     = parameter { "," parameter };
parameter      = identifier ":" type;
return_type    = type;
block          = "{" { statement } "}";
statement      = expression ";" | if_stmt | while_stmt | return_stmt | defer_stmt | variable_decl ";" | asm_block;
if_stmt        = "if" expression block [ "else" ( if_stmt | block ) ];
while_stmt     = "while" expression block;
return_stmt    = "return" [ expression ];
defer_stmt     = "defer" expression;
asm_block      = "asm" "{" { asm_line } "}";
asm_line       = { any_char - newline } newline;
expression     = assignment | binary_expr | unary_expr | primary_expr;
assignment     = lvalue "=" expression;
lvalue         = struct_destructure | dereference | field_access | array_index | identifier;
struct_destructure = "{" [ identifier ":" type { "," identifier ":" type } [ "," ] ] "}";
unary_expr     = unary_op expression;
unary_op       = "-" | "!" | "&" | "@";
primary_expr   = identifier | literal | "(" expression ")" | function_call | array_literal | struct_literal | asm_reg_expr;
asm_reg_expr   = "asm_reg" "(" abi_reg ")";
abi_reg        = "sp" | "fp" | "ra"
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

**Validation Rule:** Boolean expressions follow standard operator precedence: `and` binds tighter than `or`. If an expression would only be understood by relying on ambiguous parse recovery, it must be parenthesized; the parser rejects ambiguous precedence instead of guessing.

---

## 10. Memory Safety Framework

### 10.1 Formal Model
- **Pointer Type `T*`:** Set of memory addresses containing values of type `T`.
- **Dereference:** Partial functions `load: T* -> T` and `store: T* x T -> unit`, valid only for active pointers.
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
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">type</span> <span style="color:#e5c07b">Str</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">data</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span>
    <span style="color:#abb2bf">length</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span>
<span style="color:#56b6c2">}</span>

<span style="color:#7f848e;font-style:italic">; String literals like "Hello World" evaluate to compile-time anonymous inline structs with the exact shape: { data: u8*, length: u64 }</span>
<span style="color:#61afef">make_str</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">raw_str</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">data</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">length</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span> <span style="color:#56b6c2">})</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">{</span>
    <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">data</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">length</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span> <span style="color:#56b6c2">}</span> <span style="color:#56b6c2">=</span> <span style="color:#abb2bf">raw_str</span>
    <span style="color:#abb2bf">str_ptr</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">Str</span><span style="color:#56b6c2">)</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">str_ptr</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">data</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">data</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">length</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">length</span> <span style="color:#56b6c2">}</span>
    <span style="color:#c678dd">return</span> <span style="color:#abb2bf">str_ptr</span>
<span style="color:#56b6c2">}</span></code></pre>

### 11.2 Vector
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">type</span> <span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">data</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">T</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span>
    <span style="color:#abb2bf">length</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span><span style="color:#56b6c2">,</span>
    <span style="color:#abb2bf">capacity</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">new_vector</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;(</span><span style="color:#abb2bf">initial_capacity</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;*</span> <span style="color:#56b6c2">{</span>
    <span style="color:#abb2bf">vec</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;*</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;)</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">length</span> <span style="color:#56b6c2">=</span> <span style="color:#98c379">0</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">capacity</span> <span style="color:#56b6c2">=</span> <span style="color:#abb2bf">initial_capacity</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">data</span> <span style="color:#56b6c2">=</span> <span style="color:#c678dd">new</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">initial_capacity</span><span style="color:#56b6c2">)</span>
    <span style="color:#c678dd">return</span> <span style="color:#abb2bf">vec</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">push</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;(</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">value</span><span style="color:#56b6c2">:</span> <span style="color:#abb2bf">T</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">if</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">length</span> <span style="color:#56b6c2">&gt;=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">capacity</span> <span style="color:#56b6c2">{</span> <span style="color:#61afef">resize_vector</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">,</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">capacity</span> <span style="color:#56b6c2">*</span> <span style="color:#98c379">2</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">}</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">data</span><span style="color:#56b6c2">[</span><span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">length</span><span style="color:#56b6c2">]</span> <span style="color:#56b6c2">=</span> <span style="color:#abb2bf">value</span>
    <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">length</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">length</span> <span style="color:#56b6c2">+</span> <span style="color:#98c379">1</span>
<span style="color:#56b6c2">}</span>

<span style="color:#61afef">free_vector</span><span style="color:#56b6c2">:</span> <span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;(</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Vector</span><span style="color:#56b6c2">&lt;</span><span style="color:#abb2bf">T</span><span style="color:#56b6c2">&gt;*</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">free</span><span style="color:#56b6c2">(@</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">.</span><span style="color:#abb2bf">data</span><span style="color:#56b6c2">)</span>
    <span style="color:#c678dd">free</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">vec</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span></code></pre>

### 11.3 Memory Allocators
- **Arena:** `new_arena(size)`, `alloc_in_arena(arena, size)`, `free_arena(arena)`. Batch deallocation.
- **Pool:** `new_pool<T>(count)`, `acquire<T>(pool)`, `release<T>(pool, obj)`. Fixed-size object recycling.

### 11.4 I/O Abstraction
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">type</span> <span style="color:#e5c07b">File</span> <span style="color:#56b6c2">=</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">handle</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">buffer</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">buffer_size</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">buffer_pos</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">buffer_end</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u64</span> <span style="color:#56b6c2">}</span>
<span style="color:#61afef">open_file</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">path</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*:</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">file</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">error</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">Str</span><span style="color:#56b6c2">*</span> <span style="color:#56b6c2">}</span>
<span style="color:#61afef">read_byte</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">f</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*:</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">{</span> <span style="color:#abb2bf">byte</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">u8</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">eof</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">bool</span> <span style="color:#56b6c2">}</span>
<span style="color:#61afef">close_file</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">f</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">File</span><span style="color:#56b6c2">*:</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#56b6c2">(</span><span style="color:#56b6c2">)</span></code></pre>
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
<pre style="background:#282c34;color:#abb2bf;padding:12px;border-radius:6px;overflow-x:auto;font-family:monospace;font-size:14px;line-height:1.5;"><code><span style="color:#c678dd">external</span> <span style="color:#61afef">external_compute_sum</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">values</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">count</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#e5c07b">f32</span>

<span style="color:#61afef">compute_sum_wrapper</span><span style="color:#56b6c2">:(</span><span style="color:#abb2bf">values</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">f32</span><span style="color:#56b6c2">*</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">count</span><span style="color:#56b6c2">:</span> <span style="color:#e5c07b">i32</span><span style="color:#56b6c2">)</span> <span style="color:#56b6c2">-&gt;</span> <span style="color:#e5c07b">f32</span> <span style="color:#56b6c2">{</span>
    <span style="color:#c678dd">return</span> <span style="color:#61afef">external_compute_sum</span><span style="color:#56b6c2">(</span><span style="color:#abb2bf">values</span><span style="color:#56b6c2">,</span> <span style="color:#abb2bf">count</span><span style="color:#56b6c2">)</span>
<span style="color:#56b6c2">}</span></code></pre>
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
8. **Precedence:** Parenthesize ambiguous expressions. The compiler rejects ambiguous precedence rather than inferring it.
9. **FFI Boundaries:** Document ownership transfer. Compiler safety does not cross language boundaries.
10. **Compile-Time Functions:** Pure, deterministic, no memory allocation.