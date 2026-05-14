# Pre-OS TODO

This project is already a compiler + VM stack. The goal of this list is to finish the foundations needed before starting an OS kernel proper.

## Phase 0: Decide the OS contract
- [ ] Pick the first OS target: **bare metal on RISC-V RV64** running in the project VM and on QEMU.
- [ ] Define the boot contract: firmware/bootloader → kernel entry → console init → memory init.
- [ ] Decide whether the first OS will boot from a flat kernel binary, a kernel ELF, or both.
- [ ] Define the minimum kernel ABI for the compiler/runtime: calling convention, stack layout, panic behavior, and trap entry.
- [ ] Write the kernel bring-up goals in a separate spec so compiler/runtime changes stay aligned.

## Phase 1: Make the compiler truly freestanding
- [ ] Add a dedicated **bare-metal target mode** for HLL.
- [ ] Stop assuming Linux syscalls in the OS build path.
- [ ] Split the current runtime into two modes:
  - [ ] hosted stdlib/runtime for native app execution
  - [ ] freestanding kernel runtime for OS bring-up
- [ ] Make `runtime.hll` configurable so it can be replaced by a kernel runtime later.
- [ ] Ensure `main` is optional for kernel builds and can be replaced by a kernel entry symbol.
- [ ] Add a way to define custom entrypoints such as `kmain`, `kernel_main`, or `_start`.
- [ ] Make inline `asm { ... }` work cleanly in freestanding builds.
- [ ] Audit all codegen for hidden dependencies on host process APIs.
- [ ] Add compiler checks for unsupported hosted-only features in bare-metal mode.
- [ ] Improve diagnostics so OS-facing compile failures point to the exact source location and reason.

## Phase 2: Linker and image generation
- [ ] Add a real linker script / layout story for kernel images.
- [ ] Support placing `.text`, `.rodata`, `.data`, `.bss`, heap, stack, and trap vectors explicitly.
- [ ] Support kernel entry symbol selection without relying on the current userland `_start` flow.
- [ ] Add support for producing a kernel ELF with the correct load addresses.
- [ ] Add support for producing a flat binary image for bootloaders that want one.
- [ ] Support relocation records well enough for kernel code and global data.
- [ ] Add symbol export/import rules for kernel modules or future drivers.
- [ ] Add debug-symbol output if needed for kernel debugging later.
- [ ] Make the current assembly/ELF pipeline capable of building both hosted and freestanding images.

## Phase 3: Runtime foundation for an OS
- [ ] Replace Linux syscall-based I/O with a console abstraction.
- [ ] Provide a UART-backed console runtime for bare-metal boot.
- [ ] Add panic/abort support that writes to the console and halts cleanly.
- [ ] Add minimal formatting/printing support that does not depend on libc.
- [ ] Add `memcpy`, `memmove`, `memset`, `memcmp`, and other core freestanding helpers if missing.
- [ ] Make heap allocation usable in kernel mode, or replace it with a kernel allocator.
- [ ] Add a simple logging layer for early boot diagnostics.
- [ ] Add a minimal init/runtime split so the compiler runtime does not become the OS runtime.

## Phase 4: VM hardware completeness
- [ ] Finish the RISC-V privilege model needed for a real OS.
- [ ] Make Machine/Supervisor mode transitions work reliably.
- [ ] Implement or verify trap entry/exit behavior.
- [ ] Implement timer interrupts.
- [ ] Implement external interrupts.
- [ ] Finish CLINT behavior for timer and software interrupts.
- [ ] Finish PLIC behavior for external interrupt routing.
- [ ] Ensure UART behaves like a real serial console endpoint.
- [ ] Add a boot ROM / firmware path if the OS expects one.
- [ ] Complete Sv39 virtual memory behavior enough for kernel paging.
- [ ] Add TLB behavior if needed for realistic kernel testing.
- [ ] Add a device tree or equivalent machine description if the boot path needs one.
- [ ] Add disk/storage emulation for kernel filesystems and user programs.
- [ ] Add enough MMIO devices to support a minimal system image.
- [ ] Add reset-state tests so the VM starts in a predictable configuration.

## Phase 5: Kernel bring-up features
- [ ] Boot to a visible serial console message.
- [ ] Print CPU/board/memory information during early boot.
- [ ] Initialize page tables and virtual memory in the kernel.
- [ ] Add a physical frame allocator.
- [ ] Add a virtual memory/page allocator.
- [ ] Add a kernel heap allocator.
- [ ] Add trap handlers for exceptions and interrupts.
- [ ] Add a kernel panic path with useful diagnostics.
- [ ] Add a timer tick source for scheduling.
- [ ] Add a scheduler skeleton.
- [ ] Add context switching.
- [ ] Add a process/thread model.
- [ ] Define a syscall ABI for user processes.
- [ ] Add user-mode process loading from ELF.
- [ ] Add a minimal shell or init process.
- [ ] Add a filesystem or initramfs so the OS can load programs.
- [ ] Add device-driver scaffolding for serial, storage, and timer devices.

## Phase 6: Tooling and developer experience
- [ ] Add a kernel build command to the CLI / UI.
- [ ] Add a boot-image export path from the GUI.
- [ ] Add a freestanding target selector in the UI.
- [ ] Add a kernel debug view or boot log view.
- [ ] Add better assembly/ELF inspection for kernel images.
- [ ] Add regression snapshots for kernel-oriented codegen.
- [ ] Add end-to-end tests for booting a tiny kernel in the VM.
- [ ] Add QEMU/system-mode support if the current path remains user-mode only.
- [ ] Add CI coverage for hosted build, bare-metal build, and VM boot tests.

## Phase 7: Language/runtime polish before OS work becomes heavy
- [ ] Make sure struct layout, arrays, pointers, and generics are stable enough for kernel code.
- [ ] Verify inline asm syntax and highlighting are consistent across the compiler and editor.
- [ ] Add any missing integer widths, casts, or pointer arithmetic rules needed by kernel code.
- [ ] Improve standard-library modularity so kernel code can opt out of hosted features.
- [ ] Add clearer docs for what is hosted-only vs what is available in freestanding mode.
- [ ] Audit all examples and tests so they continue to compile under the chosen pre-OS contract.

## Done when
- [ ] The compiler can build a freestanding kernel image without Linux runtime assumptions.
- [ ] The VM can boot that image, print to serial, handle traps, and use paging.
- [ ] A minimal kernel can allocate memory, receive timer interrupts, and run at least one user process.
- [ ] The project has a clear hosted-vs-freestanding split so OS development does not fight the compiler runtime.

## Nice-to-have after the OS starts
- [ ] ELF loader for userland programs.
- [ ] Filesystem driver stack.
- [ ] Process isolation / permissions.
- [ ] Networking.
- [ ] SMP support.
- [ ] Persistent storage and package manager.
- [ ] Graphical display / framebuffer.

