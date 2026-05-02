## Future Enhancements

### Elf/image generation (FUTURE WORK)
If you eventually want to run a real OS kernel, you'll want to output ELF files. That's a future concern; for bringing up the VM, loading the raw SectionData bytes directly is fine.

**Potential Implementation:**
- Add ELF header generation to `AssembledOutput`
- Support program headers for LOAD segments
- Generate proper section headers
- Export as `.elf` binary format


## 1. What still needs to be added

### Language & Compiler Completeness

- **Generics fully integrated**  
  The semantic analyzer skips generic type checking for many integration tests (`run_semantic_analysis = false`). The compiler can lower generic specialisations, but full validation (type substitution, trait-like constraints) is missing. This is the biggest hole in language correctness.

- **Execution on WASM**  
  The WASM build has no execution. The internal VM exists but is not wired into the GUI. For a first release this is acceptable, but it must be documented as a limitation.

- **Error recovery in parser**  
  The parser stops on the first error. Robust error recovery (e.g., skipping to the next statement) would make the IDE experience smoother, but is not critical for an alpha.

- **Inline error annotations**  
  The source editor shows compilation errors only in a separate panel. Underlining or gutter marks in the source view would greatly improve usability.

### GUI / Tooling

- **Serialisation of UI layout**  
  The egui_dock layout and open views are not persisted, so the user loses their arrangement on restart.

- **Cross‑platform execution**  
  The “Run in WSL” button is Windows‑only. A native runner (qemu, spike, or the internal VM) should be offered on Linux/macOS.

- **Program management**  
  The catalog allows duplicating, renaming, deleting custom programs, but there is no export/import to/from disk. Saving only works via persistence.

- **Undo/redo in source editor**  
  Not currently implemented.

### Testing

- **Non‑Windows QEMU tests**  
  The QEMU integration tests only run on Windows via WSL. For release CI you’ll need to run them on Linux hosts where riscv64 toolchain can be installed directly.

- **VM integration tests**  
  The internal VM has unit tests for components, but no end‑to‑end tests of running compiled programs through it (only the assembler emitters are verified via goldens).

---

## 2. What should be refactored

### Compiler Internals

- **Semantic analyser vs compiler**  
  The semantic analyser re‑implements type resolution and type checking that the compiler also does. Merge them or have the compiler defer to the analyser’s resolved types.

- **Register allocation**  
  The current allocator is a trivial stack‑slot mapper with no reuse. While correct, it produces excessive spills. Replacing it with a linear‑scan register allocator would be a major improvement, but can wait.

- **String handling in assembler**  
  The assembler encodes `call`, `tail`, `la` pseudo‑instructions during encoding by manually splitting `%pcrel_hi`/`%pcrel_lo`. This duplicates relocation logic already present in the pseudo‑instruction definitions. Unify relocation calculation.

### GUI Views

- **CFG and Stack views**  
  Both parse assembly text (with comment‑based annotations) rather than working on the structured token stream or IR. This is fragile. Eventually they should use the structured `Vec<RvInstruction>` or derived IR‑level control‑flow graph.

### Testing

- **Golden‑file update workflow**  
  The golden IR/assembly tests require manually setting `UPDATE_GOLDENS` or `UPDATE_ASM_GOLDENS` environment variables. That’s fine, but consider a single `UPDATE=1` flag and a helper script.