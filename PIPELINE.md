# Compilation Pipeline

## Startup — compiled once, cached

```
programs/stdlib/types.hll
programs/stdlib/memory_allocator.hll
programs/stdlib/string_utils.hll
programs/stdlib/io.hll
       │
       ▼  get_stdlib_source()
  stdlib HLL source
       │
       ▼  CompilationPipeline::compile()
  IrProgram (stdlib)
       │
       ├──▶  extract_registries()
       │         FunctionRegistry   name → (params, return_type)
       │         TypeRegistry       name → field layout / aliases
       │
       └──▶  compile_ir_to_assembly_with_tokens()
                 stdlib_tokens: Vec<RvInstruction>   (cached)
```

## Per edit — runs on every keystroke

```
  user source (.hll)
       │
       ▼  Lexer
  Vec<Token>                              → Tokens panel
       │
       ▼  Parser
  AST                                     → AST panel
       │
       ▼  SemanticAnalyzer  ← seeded with FunctionRegistry + TypeRegistry
  diagnostics
       │
       ▼  IR Compiler  ← seeded with FunctionRegistry (return types)
  IrProgram (user)                        → IR panel
       │
       ▼  compile_ir_to_assembly_with_tokens()
  user_tokens: Vec<RvInstruction>         → Assembly panel
       │
       ▼  token-level link
  [stdlib_tokens..., user_tokens...]
       │
       ▼  assemble()
       │    appends extern_stubs():
       │      putchar   (ecall 64 / sys_write)
       │      puts, print_int, printf
       │      exit      (ecall 93)
       │      _start    (calls main, then exit)
       │
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
       │   e_entry  = load_base + addr(_start)
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
