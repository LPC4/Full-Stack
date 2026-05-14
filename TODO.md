# TODO

## Completed

- Renamed the example files to more professional names.
- Updated the GUI catalog so example display names are derived from the file name.
- Hardcoded the stdlib GUI label as `Standard Library`.
- Updated tests and example references to the renamed files.
- Made `new(T)` / `free(ptr)` compiler intrinsics (`heap_alloc` / `heap_free` IR).
- Removed registry-seeding infrastructure (`FunctionRegistry`, `TypeRegistry`,
  `extract_registries`, `compile_with_externs`, `seed_extern_fn_returns`, etc.).
- Adopted token-level linking: stdlib is compiled once to a `Vec<RvInstruction>`,
  prepended to the user token stream, then assembled in one pass.
- Cleaned up `vm_diag_test.rs`: removed all assertion-free debug scaffolding; the
  remaining four tests (`stdlib_provides_malloc`, `putchar_basic`, `printf_constexpr`,
  `functions_and_io`) exercise the full link-and-run path with real assertions.
- Updated PIPELINE.md to document the current linking model and runtime swap points.

---

## Inline assembly

**Goal:** move all runtime stubs (`putchar`, `puts`, `printf`, `print_int`, `exit`, `_start`)
out of the hardcoded Rust `extern_stubs()` in `compilation_pipeline.rs` and into the HLL
stdlib, so the compiler emits no code of its own — the full runtime is written in HLL.

### Two sub-features

**1 — `asm_reg(name)` expression**

Reads the current value of a named ABI register as an `i64`, usable anywhere an expression
is expected (including `if` conditions, arithmetic, assignments).

```hll
get_sp: () -> i64 {
    return asm_reg(sp)
}

stack_ok: () -> bool {
    return asm_reg(sp) > 0x10000
}
```

**2 — `asm { ... }` statement block**

Emits raw RISC-V instructions verbatim, interleaved with surrounding compiled code.
Used for syscall wrappers, `_start`, and anything else that requires direct hardware access.

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

**Allowed registers (both features):**
`sp`, `fp`/`s0`, `ra`, `a0`–`a7`, `s1`–`s11`.

Temp registers (`t0`–`t6`) are **not** allowed — the register allocator may have live values
in them at the asm site, and clobbering them would silently corrupt the compiled output.

**Restrictions on `asm { }` blocks:**
- No HLL variables or expressions inside — only raw assembly text.
- No data directives (`.asciz`, `.word`, etc.) — use HLL string/array literals for data.
- Branches and labels inside a block are allowed (needed for `_start` and loop constructs);
  they must not target labels outside the block.

### Implementation plan

Language front-end (parallel):

    src/1_high_level_language/lexer.rs
      - Token::Asm  (keyword "asm")

    src/1_high_level_language/ast.rs
      - Statement::AsmBlock { lines: Vec<String> }
      - PrimaryExpr::AsmReg { reg: String }

    src/1_high_level_language/parser.rs
      - parse `asm { ... }` into Statement::AsmBlock; split body by newline/semicolon
      - parse `asm_reg(name)` into PrimaryExpr::AsmReg

    src/1_high_level_language/compiler/utility/semantic_analyzer.rs
      - validate reg name against the allowed whitelist
      - AsmReg expression type is always i64

IR (parallel with language):

    src/2_intermediate_language/instruction.rs
      - IrInstruction::InlineAsm { lines: Vec<String> }
      - IrInstruction::ReadReg   { dest: IrRegister, reg: String }

    src/2_intermediate_language/asm_compiler/compiler_rv64.rs
      - InlineAsm  → emit each line as RvInstruction::Directive(line)
      - ReadReg    → emit `mv <dest_hw_reg>, <named_reg>` (pseudo MV)

Stdlib migration (after both passes above pass tests):

    programs/stdlib/runtime.hll (new)
      - _start, putchar, puts, print_int, printf, exit — all using asm { }
    programs/stdlib/io.hll
      - remove any remaining extern-stub duplicates; delegate to runtime.hll implementations
    src/1_high_level_language/stdlib.rs
      - include runtime.hll in get_stdlib_source()
    src/1_high_level_language/compilation_pipeline.rs
      - remove extern_stubs() entirely
      - assemble() becomes a plain Assembler::assemble() call with no extra tokens appended


