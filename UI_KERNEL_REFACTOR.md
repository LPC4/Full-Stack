# UI Kernel Refactor Plan

## Goal

Clean up the kernel/OS workflow by:
- Adding `TargetMode::Kernel` so the IDE compiles kernel programs natively (no special program kind)
- Removing `ProgramKind::Kernel` and the embedded `KernelView` panel
- Building a separate **Machine window** (egui secondary viewport) that owns all OS-level views
- Making the IDE→Debug transition kernel-aware so you can step through kernel code

---

## What Gets Removed

| Item | Location | Why |
|---|---|---|
| `ProgramKind::Kernel` | `src/view/program_catalog.rs` | Replaced by `TargetMode::Kernel` on any program |
| `KernelView` panel | `src/view/ide/kernel_view.rs` | Machine window replaces it |
| Kernel tab in IDE dock | `app.rs: reset_layout()` | No longer a dock tab |
| `ProgramFile::kernel()` constructor | `program_catalog.rs` | Use `ProgramFile::example()` instead |
| `is_kernel()` on `ProgramFile` | `program_catalog.rs` | Use `app.target_mode == TargetMode::Kernel` |
| `kernel-boot` as `ProgramKind::Kernel` | `built_in_programs()` | Becomes a regular example program |

---

## Step 1 — Add `TargetMode::Kernel`

**`crates/hll-to-ir/src/lib.rs`**
```rust
pub enum TargetMode {
    Hosted,
    Freestanding,
    Kernel,   // ← new
}
```

**`crates/hll-to-ir/src/stdlib.rs`**  
Add `TargetMode::Kernel => get_kernel_stdlib_source()` to `get_stdlib_source_for_mode`.

**`src/compilation_pipeline.rs`**  
- When `target_mode == Kernel`, use kernel stdlib, `"_kernel_start"` entry point, and `LinkLayout::freestanding_kernel()`
- `compile_and_run()` / `build_vm()` calls `VirtualMachine::new_kernel()` instead of `new()`

---

## Step 2 — Make Debug Mode Kernel-Aware

**`app.rs: enter_debug_mode()`**  
Currently always calls `VirtualMachine::new()`. Change to:

```rust
let vm = if self.target_mode == TargetMode::Kernel {
    VirtualMachine::new_kernel(&assembled)
} else {
    VirtualMachine::new(&assembled)
};
```

CPU starts at `ROM_BASE` for kernel programs; the debugger steps through `_start` → S-mode → `kmain`. No other changes needed — registers, memory, pipeline view all work identically.

---

## Step 3 — Remove `ProgramKind::Kernel`

**`src/view/program_catalog.rs`**
- Remove `ProgramKind::Kernel` variant
- Remove `ProgramFile::kernel()` constructor
- Remove `is_kernel()` method
- Change `kernel-boot` catalog entry to `ProgramFile::example()`
- Remove `ProgramKind::Kernel` arm from `ensure_consistency()`

**`src/view/ide/kernel_view.rs`** — delete file  
**`src/view/ide/mod.rs`** — remove `kernel_view` module and `KernelView` re-export  
**`app.rs`** — remove `KernelView` from imports, `views[9]` from `reset_layout()`, and the Kernel tab from the split

---

## Step 4 — Build the Machine Window

New file: **`src/machine_window.rs`**

```
MachineWindow {
    vm: Option<Box<VirtualMachine>>,   // live kernel VM instance
    boot_result: Option<BootResult>,   // uart output, exit code, steps
    is_running: bool,
    viewport_id: egui::ViewportId,
}
```

**Layout inside the viewport:**
```
┌────────────────────────────────────┬───────────────────────┐
│                                    │  Boot Log (UART)      │
│        Framebuffer                 │  (scrollable terminal)│
│        (primary, ~60% width)       ├───────────────────────┤
│                                    │  [tabs]               │
│                                    │  Privilege | Traps    │
│                                    │  Interrupts | Syscalls│
│                                    │  Page Table           │
└────────────────────────────────────┴───────────────────────┘
  [ Boot ]  [ Reset ]   Mode: S  Steps: 1,204,830   Exit: 0
```

**Behaviour:**
- Boot button compiles the current IDE program using `TargetMode::Kernel` pipeline, calls `new_kernel()`, runs to completion (or step limit), populates boot log and framebuffer
- Reset clears state
- Window can remain open while editing in the IDE — it holds its own VM instance

**Existing views to wire in (currently dead):**
- `src/view/os/privilege_view.rs`
- `src/view/os/interrupt_view.rs`
- `src/view/os/syscall_trace_view.rs`
- `src/view/os/trap_view.rs`
- `src/view/os/page_table_view.rs`
- `src/view/debug/framebuffer_view.rs` (move from debug dock to Machine window)

---

## Step 5 — Wire Machine Window into App

**`app.rs`**
- Add `machine_window: Option<MachineWindow>` to `FullStackApp`
- Add `show_machine: bool` flag
- Top bar: add **Machine** button (monitor icon) that sets `show_machine = true`
- In `update()`: if `show_machine`, call `machine_window.show(ctx, &self.compilation_state, &self.catalog)`
- Machine window receives a reference to `CompilationState` so it can recompile when Boot is clicked; it does not write back to it

**`src/app.rs` top bar (before / after):**
```
Before:  [ IDE ]  [ Debug ]
After:   [ IDE ]  [ Debug ]  [ ⬛ Machine ]
```

`[ ⬛ Machine ]` opens/closes the secondary viewport. It does not replace IDE or Debug — it floats independently.

---

## Step 6 — Target Mode Switcher Update

**`app.rs`** target mode radio/buttons — add `Kernel` option:

```
Target:  ○ Hosted   ○ Freestanding   ○ Kernel
```

When `Kernel` is selected:
- Stdlib switches to kernel stdlib
- Entry point fixed to `_kernel_start`
- Link layout uses `freestanding_kernel()`
- VM Output panel is hidden or shows a message ("Use Machine window to boot kernel programs")
- Debug mode uses `new_kernel()`

---

## File Summary

| Action | File |
|---|---|
| Modify | `crates/hll-to-ir/src/lib.rs` |
| Modify | `crates/hll-to-ir/src/stdlib.rs` |
| Modify | `src/compilation_pipeline.rs` |
| Modify | `app.rs` |
| Modify | `src/view/program_catalog.rs` |
| Modify | `src/view/ide/mod.rs` |
| Delete | `src/view/ide/kernel_view.rs` |
| Create | `src/machine_window.rs` |
