# Project Structure

A compact map of the repository, with the most important places called out first.

## Quick navigation

- **Application entry and UI:** `src/`
- **Compiler pipeline:** `crates/hll-to-ir/`, `crates/ir-to-asm/`
- **Assembler and binary output:** `crates/asm-to-binary/`
- **Virtual machine:** `crates/virtual-machine/`
- **OS/runtime pieces:** `crates/os-runtime/`
- **Example programs:** `programs/example/`
- **Test programs and fixtures:** `programs/test/` and `tests/`
- **Documentation and specs:** `README.md` and the spec files under `crates/*/`

## Where to go for what

- **Run or change the desktop/web app:** start in `src/main.rs`, then follow `src/app.rs` and `src/compilation_pipeline.rs`.
- **Change the editor panels and views:** `src/view/`.
- **Work on the HLL frontend or IR generation:** `crates/hll-to-ir/`.
- **Work on IR to assembly lowering:** `crates/ir-to-asm/`.
- **Change the assembler or ELF output:** `crates/asm-to-binary/`.
- **Change VM execution, devices, memory, or CPU pipeline:** `crates/virtual-machine/`.
- **Adjust the OS/runtime boot or kernel sources:** `crates/os-runtime/`.
- **Add or update language/compiler examples:** `programs/example/` and `programs/test/`.
- **Update regression tests or golden expectations:** `tests/` and `programs/test/`.

## Important docs

- `README.md` - project overview and setup
- `UI_KERNEL_REFACTOR.md` - UI/kernel refactor notes
- `Trunk.toml` - web build and dev server configuration
- `rustfmt.toml` - formatting rules
- `.typos.toml` - spelling configuration
- `crates/hll-to-ir/_LANG_SPECIFICATIONS.md` - HLL language spec
- `crates/hll-to-ir/_IR_SPECIFICATIONS.md` - IR spec
- `crates/asm-to-binary/_RISCV_SPECIFICATIONS.md` - RISC-V backend spec
- `crates/virtual-machine/_VM_SPECIFICATION.md` - VM spec
- `crates/os-runtime/_OS_SPECIFICATION.md` - OS/runtime spec

## Top-level folders and files

The repository contains these top-level entries:

- `.cargo/`
- `.claude/`
- `.github/`
- `.git/`
- `.idea/`
- `.run/`
- `.typos.toml`
- `Cargo.lock`
- `Cargo.toml`
- `LICENSE-APACHE`
- `LICENSE-MIT`
- `README.md`
- `Trunk.toml`
- `assets/`
- `build.rs`
- `dist/`
- `flake.nix`
- `index.html`
- `programs/`
- `PROJECT_STRUCTURE.md`
- `rust-toolchain`
- `rustfmt.toml`
- `src/`
- `target/`
- `tests/`

## `assets/`

```text
assets/
+-- icon/
|   +-- icon.png
|   +-- icon.svg
+-- manifest.json
+-- readme/
|   +-- debugger.png
|   +-- demo.png
|   +-- ide.png
+-- sw.js
```

## `src/`

```text
src/
+-- app.rs
+-- cli/
|   +-- main.rs
+-- compilation_pipeline.rs
+-- lib.rs
+-- machine_window.rs
+-- main.rs
+-- target_mode.rs
+-- view/
    +-- compilation_state.rs
    +-- common/
    |   +-- highlighter/
    |   |   +-- asm.rs
    |   |   +-- ast.rs
    |   |   +-- hll.rs
    |   |   +-- ir.rs
    |   |   +-- mod.rs
    |   +-- mod.rs
    |   +-- theme.rs
    |   +-- widgets.rs
    +-- debug/
    |   +-- cache_view.rs
    |   +-- cpu_state_view.rs
    |   +-- disassembly_view.rs
    |   +-- framebuffer_view.rs
    |   +-- io_view.rs
    |   +-- memory_view.rs
    |   +-- mod.rs
    |   +-- perf_view.rs
    |   +-- pipeline_view.rs
    |   +-- snapshot.rs
    +-- ide/
    |   +-- cfg_view.rs
    |   +-- code_views.rs
    |   +-- execution_view.rs
    |   +-- memory_map_view.rs
    |   +-- mod.rs
    |   +-- source_view.rs
    |   +-- stack_view.rs
    |   +-- vm_execution_view.rs
    +-- layout.rs
    +-- mod.rs
    +-- os/
    |   +-- interrupt_view.rs
    |   +-- mod.rs
    |   +-- page_table_view.rs
    |   +-- privilege_view.rs
    |   +-- syscall_trace_view.rs
    |   +-- trap_view.rs
    +-- program_catalog.rs
    +-- viewtrait.rs
```

## `crates/`

```text
crates/
+-- asm-to-binary/
|   +-- _RISCV_SPECIFICATIONS.md
|   +-- Cargo.toml
|   +-- README.md
|   +-- src/
|   |   +-- assembler/
|   |   |   +-- directive.rs
|   |   |   +-- encode.rs
|   |   |   +-- layout.rs
|   |   |   +-- link_layout.rs
|   |   |   +-- mod.rs
|   |   |   +-- output.rs
|   |   |   +-- parser.rs
|   |   |   +-- reg_parse.rs
|   |   |   +-- section.rs
|   |   |   +-- symbol_table.rs
|   |   |   +-- token.rs
|   |   +-- encode_decode.rs
|   |   +-- lib.rs
|   |   +-- linker.rs
|   |   +-- macros.rs
|   |   +-- pseudo.rs
|   |   +-- real.rs
|   |   +-- riscv/
|   |       +-- mod.rs
|   |       +-- rv64a.rs
|   |       +-- rv64fd.rs
|   |       +-- rv64i.rs
|   |       +-- rv64m.rs
|   |       +-- rv64zicsr.rs
|   +-- tests/
+-- hll-to-ir/
|   +-- Cargo.toml
|   +-- README.md
|   +-- _LANG_SPECIFICATIONS.md
|   +-- src/
|   |   +-- ast.rs
|   |   +-- compiler/
|   |   |   +-- compiler/
|   |   |   |   +-- control_flow.rs
|   |   |   |   +-- declarations.rs
|   |   |   |   +-- expressions.rs
|   |   |   |   +-- literals.rs
|   |   |   |   +-- types.rs
|   |   |   |   +-- utils.rs
|   |   |   +-- compiler.rs
|   |   |   +-- mod.rs
|   |   |   +-- utility/
|   |   |       +-- diagnostics.rs
|   |   |       +-- lowering_context.rs
|   |   |       +-- mod.rs
|   |   |       +-- semantic_analyzer.rs
|   |   |       +-- symbol_table.rs
|   |   |       +-- type_context.rs
|   |   +-- hll_compiler.rs
|   |   +-- ir/
|   |   |   +-- block.rs
|   |   |   +-- instruction.rs
|   |   |   +-- mod.rs
|   |   |   +-- ops.rs
|   |   |   +-- program.rs
|   |   |   +-- types.rs
|   |   |   +-- values.rs
|   |   +-- lexer.rs
|   |   +-- lib.rs
|   |   +-- parser.rs
|   |   +-- stdlib.rs
|   |   +-- token.rs
|   +-- tests/
+-- ir-to-asm/
|   +-- Cargo.toml
|   +-- README.md
|   +-- _IR_SPECIFICATIONS.md
|   +-- src/
|   |   +-- compiler/
|   |   |   +-- assembly_emitter.rs
|   |   |   +-- compiler_rv64.rs
|   |   |   +-- data_section.rs
|   |   |   +-- frame_context.rs
|   |   |   +-- function_context.rs
|   |   |   +-- mod.rs
|   |   |   +-- register_allocator.rs
|   |   |   +-- type_utils.rs
|   |   +-- lib.rs
|   +-- tests/
+-- os-runtime/
|   +-- _OS_SPECIFICATION.md
|   +-- boot/
|   |   +-- startup.s
|   |   +-- trap.s
|   +-- Cargo.toml
|   +-- kernel/
|   |   +-- kernel_runtime.hll
|   |   +-- my_kernel.hll
|   |   +-- README.md
|   |   +-- trap_handler.hll
|   +-- README.md
|   +-- src/
|   |   +-- lib.rs
|   +-- stdlib/
|   |   +-- common/
|   |   +-- freestanding/
|   |   +-- hosted/
|   +-- tests/
+-- virtual-machine/
    +-- Cargo.toml
    +-- README.md
    +-- _VM_SPECIFICATION.md
    +-- src/
    |   +-- bus.rs
    |   +-- cpu.rs
    |   +-- cpu/
    |   |   +-- alu.rs
    |   |   +-- csr.rs
    |   |   +-- decoder.rs
    |   |   +-- hazard_unit.rs
    |   |   +-- mmu.rs
    |   |   +-- pipeline.rs
    |   |   +-- pipeline/
    |   |   |   +-- decode.rs
    |   |   |   +-- execute.rs
    |   |   |   +-- fetch.rs
    |   |   |   +-- memory.rs
    |   |   |   +-- registers.rs
    |   |   |   +-- writeback.rs
    |   |   +-- predictor.rs
    |   |   +-- registers.rs
    |   |   +-- traps.rs
    |   +-- devices.rs
    |   +-- devices/
    |   |   +-- clint.rs
    |   |   +-- plic.rs
    |   |   +-- uart.rs
    |   +-- elf_parser.rs
    |   +-- error.rs
    |   +-- lib.rs
    |   +-- linker.rs
    |   +-- memory.rs
    |   +-- memory/
    |   |   +-- cache.rs
    |   |   +-- ram.rs
    |   |   +-- rom.rs
    |   +-- rom.rs
    |   +-- virtual_machine.rs
    +-- tests/
```

## `programs/`

```text
programs/
+-- example/
|   +-- array_initialization.hll
|   +-- casting_and_pointers.hll
|   +-- compile_time_math.hll
|   +-- control_flow_basics.hll
|   +-- core_basics.hll
|   +-- generics_and_strings.hll
|   +-- pointer_arrays.hll
|   +-- struct_binding.hll
+-- kernel/
+-- test/
    +-- compiler_suite/
    |   +-- arithmetic/
    |   |   +-- 01_basic_arithmetic.hll
    |   |   +-- 01_basic_arithmetic.ir
    |   |   +-- 01_basic_arithmetic.s
    |   +-- control_flow/
    |   |   +-- 02_conditional_and_loop.hll
    |   |   +-- 02_conditional_and_loop.ir
    |   |   +-- 02_conditional_and_loop.s
    |   |   +-- 05_constants_and_loops.hll
    |   |   +-- 05_constants_and_loops.ir
    |   |   +-- 05_constants_and_loops.s
    |   +-- functions/
    |   |   +-- 07_simple_assign.hll
    |   |   +-- 07_simple_assign.ir
    |   |   +-- 07_simple_assign.s
    |   |   +-- 11_constexpr_pure_functions.hll
    |   |   +-- 11_constexpr_pure_functions.ir
    |   |   +-- 11_constexpr_pure_functions.s
    |   |   +-- 12_constexpr_while_loops.hll
    |   |   +-- 12_constexpr_while_loops.ir
    |   |   +-- 12_constexpr_while_loops.s
    |   +-- pointers/
    |   |   +-- 03_basic_pointers.hll
    |   |   +-- 03_basic_pointers.ir
    |   |   +-- 03_basic_pointers.s
    |   |   +-- 08_chained_deref.hll
    |   |   +-- 08_chained_deref.ir
    |   |   +-- 08_chained_deref.s
    |   +-- types/
    |       +-- 04_struct_types.hll
    |       +-- 04_struct_types.ir
    |       +-- 04_struct_types.s
    |       +-- 06_tuple_destructuring.hll
    |       +-- 06_tuple_destructuring.ir
    |       +-- 06_tuple_destructuring.s
    |       +-- 09_string_literals.hll
    |       +-- 09_string_literals.ir
    |       +-- 09_string_literals.s
    |       +-- 10_generic_naming_collision_free.hll
    |       +-- 10_generic_naming_collision_free.ir
    |       +-- 10_generic_naming_collision_free.s
    |       +-- 15_signed_unsigned_casts.hll
    |       +-- 15_signed_unsigned_casts.ir
    |       +-- 15_signed_unsigned_casts.s
    +-- fixtures/
    |   +-- lexer/
    |   |   +-- 01_comments_and_newlines.hll
    |   +-- parser/
    +-- integration/
    |   +-- arrays/
    |   +-- functions/
    |   +-- generics/
    |   |   +-- generic_types_test.hll
    |   |   +-- nested_generics_test.hll
    |   +-- pointers/
    |   |   +-- pointer_heavy_flow_test.hll
    |   +-- structs/
    |       +-- struct_destructuring_test.hll
    +-- qemu/
        +-- 01_arithmetic_and_types.hll
        +-- 02_control_flow.hll
        +-- 03_structs_and_destructuring.hll
        +-- 04_pointers_and_memory.hll
        +-- 05_functions_and_io.hll
```

## `tests/`

```text
tests/
+-- all.rs
+-- common/
|   +-- golden_support.rs
+-- integration/
|   +-- asm_fixes.rs
|   +-- assembly_golden_suite.rs
|   +-- cli_pipeline.rs
|   +-- compiler_suite.rs
|   +-- golden_support.rs
|   +-- highlighter.rs
|   +-- integration_fixtures.rs
|   +-- ir_generation.rs
|   +-- kernel_boot_device_tree.rs
|   +-- linker.rs
|   +-- platform_stdlib.rs
|   +-- qemu_execution.rs
|   +-- relocation_tests.rs
|   +-- rv64_codegen.rs
|   +-- spec_rules.rs
|   +-- struct_destructuring.rs
|   +-- vm_execution.rs
+-- vm_diag_test.rs
```

## Quick orientation

- `src/` contains the application and visualizer UI.
- `crates/` contains the compiler, assembler, runtime, and VM pieces.
- `programs/` contains example and test HLL programs.
- `tests/` contains the Rust test suite.
