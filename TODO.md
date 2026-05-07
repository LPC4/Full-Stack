## Future Enhancements

fix this by adding my own stdlib with malloc and free that actually works, and then linking against it. 

## 1. What still needs to be added

### Language & Compiler Completeness

- **Generics fully integrated**  
  The semantic analyzer skips generic type checking for many integration tests (`run_semantic_analysis = false`). The compiler can lower generic specialisations, but full validation (type substitution, trait-like constraints) is missing. This is the biggest hole in language correctness.

- **Inline error annotations**  
  The source editor shows compilation errors only in a separate panel. Underlining or gutter marks in the source view would greatly improve usability.

### GUI / Tooling

- **Cross‑platform execution**  
  The “Run in WSL” button is Windows‑only. A native runner (qemu, spike, or the internal VM) should be offered on Linux/macOS.

### Testing

- **Non‑Windows QEMU tests**  
  The QEMU integration tests only run on Windows via WSL. For release CI you’ll need to run them on Linux hosts where riscv64 toolchain can be installed directly.

---

## 2. What should be refactored

### Compiler Internals

- **Semantic analyser vs compiler**  
  The semantic analyser re‑implements type resolution and type checking that the compiler also does. Merge them or have the compiler defer to the analyser’s resolved types.

- **Register allocation**  
  The current allocator is a trivial stack‑slot mapper with no reuse. While correct, it produces excessive spills. Replacing it with a linear‑scan register allocator would be a major improvement, but can wait.

- **String handling in assembler**  
  The assembler encodes `call`, `tail`, `la` pseudo‑instructions during encoding by manually splitting `%pcrel_hi`/`%pcrel_lo`. This duplicates relocation logic already present in the pseudo‑instruction definitions. Unify relocation calculation.