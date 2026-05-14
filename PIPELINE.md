# Compilation Pipeline

`new(T[, count])` and `free(ptr)` are compiler intrinsics, they lower directly to
`heap_alloc`/`heap_free` IR instructions and need no `external` declaration.
The backend emits `call malloc` / `call free`; whichever runtime token stream is linked
must define those symbols (current: HLL stdlib).

Everything else exposed by the stdlib (`putchar`, `puts`, `printf`, `print_int`, `exit`,
`_start`, string helpers, etc.) is ordinary HLL source in `runtime.hll` / stdlib modules.

## Startup

```
programs/stdlib/types.hll
programs/stdlib/memory_allocator.hll
programs/stdlib/string_utils.hll
programs/stdlib/runtime.hll
       │
       ▼  get_stdlib_source()
  stdlib HLL source
       │
       ▼  CompilationPipeline::compile()
  IrProgram (stdlib)
       │
       └──▶  compile_ir_to_assembly_with_tokens()
                 stdlib_tokens: Vec<RvInstruction>   (cached in app.stdlib_tokens)
```

## Per edit

```
  user source (.hll)
       │
       ▼  Lexer
  Vec<Token>                              → Tokens panel
       │
       ▼  Parser
  AST                                     → AST panel
       │
       ▼  SemanticAnalyzer
  diagnostics
       │
       ▼  IR Compiler
  IrProgram (user)                        → IR panel
       │
       ▼  compile_ir_to_assembly_with_tokens()
  user_tokens: Vec<RvInstruction>         → Assembly panel
       │
       ▼  token-level link
  [stdlib_tokens..., user_tokens...]
       │
       ▼  assemble()
       │    Pass 0  RvInstruction → Vec<AsmToken>
       │    Pass 1  layout: section-relative label addresses
       │            section order: Text, Data, RoData, Bss (always last)
       │    Pass 2  encode: absolute addresses, PC-relative offsets
       │
  AssembledOutput
       │   .sections       Vec<SectionData>  (Text, RoData, Data, Bss)
       │   .symbol_table   HashMap<name, addr>
       │   .global_symbols Vec<String>
       │
       ▼  to_elf(ELF_LOAD_BASE = 0x10000)
  ELF-64 bytes
       │   single PT_LOAD  R|W|X
       │   p_vaddr  = 0x10000
       │   p_filesz = Text + RoData + Data   (BSS excluded)
       │   p_memsz  = p_filesz + Bss         (loader zero-fills BSS)
       │   e_entry  = load_base + addr(_start)  (fallback: `main`, then first section)
       │
       ├──▶  VirtualMachine::from_elf()              → Execution panel (native + WASM)
       │         map PT_LOAD → RAM_BASE (0x80000000)
       │         zero-fill BSS, write heap bump-pointer
       │         sp = top of RAM,  PC = _start
       │         PipelinedCpu  5-stage in-order
       │         ecall 64 → UART buffer → uart_output
       │
       └──▶  temp .elf → qemu-riscv64                → QEMU tab (native only)
```

## Linking model

The token stream is the linkable unit. To swap runtimes, substitute the stdlib token
stream with any `Vec<RvInstruction>` that defines the symbols user code calls externally:

| Runtime              | Provides                              | How linked          |
|----------------------|---------------------------------------|---------------------|
| HLL stdlib (current) | malloc, free, string utils, runtime I/O + `_start` | token-level prepend |
| Custom allocator     | malloc, free (drop-in replacement)    | token-level prepend |
| C stdlib (future)    | everything via libc + crt0            | ld / lld            |

Current runtime behavior is implemented in `programs/stdlib/runtime.hll` via inline asm:

- `putchar` uses Linux `ecall 64` (`sys_write`)
- `exit` uses Linux `ecall 93` (`sys_exit`)
- `_start` calls `main()` then `exit(code)`

`assemble()` performs pure assembly passes only (parse/layout/encode) and injects no code.
