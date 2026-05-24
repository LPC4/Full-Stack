# OS runtime (ROM, kernel, and OS roadmap)

This crate contains the project's bare-metal runtime, kernel HLL sources and boot/ROM support. It is not a Rust library to be linked into other Rust projects; instead it holds the HLL and assembly sources used by the Full-Stack VM and by examples that build a kernel image.

What this crate currently contains
- `boot/startup.s`, `boot/trap.s` - low-level M-mode entry / trap stubs used by the freestanding runtime. These are the pieces that initialize SP, clear BSS and provide the assembly trap-entry used by the kernel.
- `kernel/*.hll` - kernel-mode HLL sources (boot helpers, `pmm.hll`, `vmm.hll`, `my_kernel.hll` example). The minimal kernel entrypoint now lives in `kernel/entry.hll`; reusable kernel helpers (kmalloc, timers, trap prologue, etc.) have been moved to `stdlib/kernel/utilities.hll` and are part of the kernel stdlib bundle.
- `stdlib/` - small freestanding stdlib variants (freestanding and hosted forms) used by the compiler toolchain when targeting kernels vs. hosted programs.
- `_OS_SPECIFICATION.md` - detailed specification of the platform, memory map, calling conventions, boot protocol and HAL that kernel authors must follow.

What is implemented now
- Minimal ROM/boot semantics implemented in the VM and provided test startup assembly in `boot/`.
- A tiny freestanding runtime that provides `_start`, `panic` and simple memory primitives; the kernel example `my_kernel.hll` shows how the kernel initializes the heap, PMM and VMM and takes over the machine.
- Platform HAL primitives for UART, CLINT and PLIC used by the VM and kernel tests.

What will come later (roadmap)
- A richer kernel runtime: process model, basic syscall layer, simple scheduler and process isolation.
- Device drivers beyond UART: block device, simple filesystem support for testing, timer and interrupt improvements.
- SMP/hart bring-up and device-tree parsing for moving beyond single-hart tests.
- More complete ROM/firmware flow (bootloader stages, external boot media support) for running on real hardware.

How to use these sources
- Build a kernel image with the HLL compiler using the `riscv64-bare-metal` target. See `_OS_SPECIFICATION.md` for the expected symbols and linker layout.
- The VM and test harness pick up the kernel files from this crate for integration tests - the test suite runs `my_kernel.hll` and captures UART output to assert expected boot behaviour.

See `_OS_SPECIFICATION.md` for the authoritative, detailed specification of the memory map, ABI and boot protocol.
