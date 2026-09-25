# os-runtime

Boot firmware, S-mode kernel, and standard-library sources for the Full-Stack VM.

This crate is not a Rust library to link against. It holds the RISC-V assembly (`.s`) and
HLL (`.hll`) sources that the compiler pipeline assembles into a bootable kernel image, plus
the stdlib bundles shared by hosted and freestanding programs. The Rust side (`src/lib.rs`)
exposes a generated path-keyed source registry so `hll-to-ir` and `virtual-machine` can embed
them.

## Folder layout

```
boot/       M-mode ROM firmware (assembly): reset stub and hosted trap handler
kernel/     S-mode kernel sources (HLL): traps, memory, processes, syscalls, fs
stdlib/     focused core modules and target-specific platform providers
src/        thin Rust crate exposing the generated source registry
tests/      Rust tests over ROM, boot sequence, allocator, PMM, and subsystems
```

Boot-FS userspace tools, demos, samples, and fixtures live in `programs/user/`.

## Compilation modes

The same toolchain assembles three different runtimes, selected by `TargetMode`:

| Mode | Entry | Privilege | I/O |
|------|-------|-----------|-----|
| Hosted | `_start` | U-mode (under a kernel or ROM) | Linux-style `ecall` |
| Freestanding | `_start` | bare-metal | direct UART MMIO |
| Kernel | `_kernel_start` | S-mode (ROM drops from M-mode) | MMIO + HAL primitives |

## Boot sequence

```
Power-on / VM reset
  boot/startup.s   _start (ROM 0x000): open PMP, delegate traps to S-mode,
                   set mepc to the kernel entry, mret -> S-mode
  boot/trap.s      _m_trap (ROM 0x100): M-mode ecall dispatch for hosted programs
                   (sys_write -> UART, sys_exit -> SYSCON halt)

  stdlib/platform/kernel/entry.hll  _kernel_start: calls kmain(), panics if it returns
  kernel/core/my_kernel.hll         kmain(): console, traps, timer, PLIC, diagnostics,
                                    heap, physical pages, Sv39 address spaces, tasks.init,
                                    spawn pid 1, mount filesystem, enable interrupts
```

After `kmain` enters the idle loop the scheduler takes over: the CLINT timer preempts the
running process every tick, and `sret` resumes whichever process is next in the ready queue.

## Standard-library environments

`stdlib/stdlib.build` maps logical import names to canonical repository-relative module IDs.
All modes include the focused modules in `stdlib/core/`. Freestanding and kernel modes add
the shared bare-metal heap, UART, and panic providers. Freestanding then adds its `_start`
and SYSCON shutdown entry; kernel adds `_kernel_start` and `klog`. Hosted mode instead adds
the `brk` heap and syscall-backed console providers plus its process runtime and C-string
boundary helpers.

## Kernel subsystems

| Source | Provides |
|--------|----------|
| `kernel/core/kernel_abi.hll`, `kernel_config.hll` | Stable trap/PCB layouts, memory ranges, and PTE flags |
| `kernel/arch/riscv64/` | CSR, timer, PLIC, UART, keyboard, framebuffer, and shutdown boundaries |
| `kernel/mm/kernel_heap.hll`, `physical_pages.hll` | Kernel allocation and reusable 4 KiB physical pages |
| `kernel/mm/page_tables.hll`, `address_space.hll` | Sv39 mechanics, shared mappings, activation, and fork copying |
| `kernel/mm/user_memory.hll` | Checked user ranges and bounded path copies |
| `kernel/sched/process.hll`, `scheduler.hll` | Internal PCB construction and round-robin queue ownership |
| `kernel/sched/tasks.hll` | Process lifecycle and scheduling facade |
| `kernel/exec/elf_loader.hll` | Checked static ELF64 segment loading |
| `kernel/syscall/` | Trap-facing dispatch, trace, process, memory, file, I/O, and device handlers |
| `kernel/fs/files.hll` | Mounted filesystem facade and single state owner |
| `kernel/fs/fs_*.hll` | Disk records, image access, allocation, directories, and descriptors |
| `kernel/trap/trap_entry.hll`, `trap_handler.hll` | Stable trap entry and short cause dispatcher |

## Filesystem

The `files` facade mounts a contiguous image (superblock, inode table, free-block bitmap, data
blocks).
Inodes are 128 bytes and index up to 44 direct 4 KiB blocks (176 KiB max file), enough to hold
executable images. The public API covers initialization, descriptor I/O, directory mutation,
metadata queries, and directory iteration, with absolute
path resolution rooted at inode 0. Native paths are slices; the unchanged 32-byte on-disk name
fields are decoded and compared with a 32-byte bound.

## Syscalls

U-mode processes trap in via `ecall` (number in `a7`, payload in `a0`-`a5`, result in `a0`).
The native ABI marshals each slice as consecutive pointer and length words. Path ranges are
validated and copied into the kernel before filesystem code receives them.
Standard numbers follow the Linux RISC-V ABI; the 100-range numbers are project-specific to
support the interactive shell.

| Num | Name | Purpose |
|-----|------|---------|
| 2 | yield | voluntary reschedule |
| 46 | ftruncate | shrink a file to an exact length |
| 56 / 57 | open / close | file descriptors over the filesystem |
| 63 / 64 | read / write | fd >= 2 hit the filesystem; fd 0/1 hit the UART |
| 82 / 83 | rename / mkdir | filesystem mutation |
| 93 | exit | terminate process; halts the VM when it is the last one |
| 100 | readchar | read one UART byte (-1 if none pending) |
| 101 | readdir | list a directory entry by index |
| 102 | stat | inode type at a path |
| 103 | exec | load and run an `ELF` executable from the filesystem |
| 104 | pidalive | 1 while a launched pid is still runnable |
| 105 / 106 | unlink / rmdir | remove a file / remove an empty directory |
| 107 | map_fb | map the framebuffer device into the caller; returns its base VA |
| 220 | fork | clone the current process; returns the child pid to the parent, 0 to the child |
| 260 | wait | reap an exited child; returns its exit code (-1 if none) |

`sys_exec` loads a static ELF from the filesystem into a fresh address space at its linked
virtual addresses, then creates a PCB and enqueues it. The shell uses `exec` + `waitpid` for
foreground children. Redirection passes a checked request containing path and argument slices
plus descriptors that the shell opened before exec.

## Userspace

- `user_hello.hll` -- writes a greeting via `sys_write`, then yields forever.
- `shell.hll` -- interactive shell booted as pid 1. Reads a line from the UART one byte at a
  time (yielding while idle) and runs built-ins: `ls`, `cd <dir>`, `cat <file>`,
  `edit <file>`, `as <src> <out>`, `run <file>`, `touch <file>`, `mkdir <dir>`,
  `rm <file>`, `rmdir <dir>`, `mv <old> <new>`, `help`, `exit`. A foreground child runs
  via `exec` + `wait`: the shell reaps it, prints `[exit N]`, and returns to the prompt;
  Ctrl-C tears the child down. Executable files use the `.elf` extension; `run` verifies
  the `ELF` magic before launching.
- `edit.hll` -- an `ed`-style line editor launched by the shell's `edit` built-in. It loads
  a file into a line array and accepts line commands over the UART (`p` print, `N` goto,
  `a`/`i` append/insert, `d` delete, `c` clear, `r` replace, `s/old/new/` substitute,
  `w`/`q`). Line-oriented because the GUI terminal renders no ANSI or cursor codes.
- `as.hll` -- a userspace assembler installed at `/bin/as.elf`, launched by the shell's
  `as` built-in. It reads a `.s` file, two-pass assembles a small RV64I subset, and writes
  a runnable ELF -- closing the self-hosting loop entirely inside the VM. See the OS
  specification (9.2.2) for the supported instructions.
- `cc.hll` -- a userspace HLL-0 compiler installed at `/bin/cc.elf`, launched by the shell's
  `cc` built-in. It reads an `.hll` file, parses the HLL-0 subset (lang spec Appendix D)
  into a flat node array, and emits naive stack-machine assembly in the `as` subset, so
  `cc foo.hll foo.s && as foo.s foo.elf && foo` builds and runs a program with a toolchain
  that itself runs inside the VM. See the OS specification (10.4).
- `ld.hll` -- a userspace static linker installed at `/bin/ld.elf`, launched by the shell's
  `ld` built-in. It reads N `ET_REL` objects (from `as <src> <out>.o`), merges their
  sections, resolves the global symbol table, applies relocations, and writes a runnable
  ELF, so `as` + `ld` give separate compilation entirely inside the VM. See the OS
  specification (10.4).
- `cube.hll` -- maps the framebuffer via `map_fb` and animates a spinning wireframe cube in
  native `f64`, double-buffered so it never flickers. Installed at `/home/demo/cube.elf`;
  reads WASD key events through `poll_key` to steer the rotation.
- `mandelbrot.hll` -- maps the framebuffer and renders a Mandelbrot set in native `f64`, then
  exits. Installed at `/home/demo/mandelbrot.elf`; `run mandelbrot` paints the FB tab,
  filling in progressively as it renders.

## Image injection

Integration tests place binaries and the filesystem image directly into physical RAM before the
kernel boots:

| Physical address | Content |
|------------------|---------|
| `0x87F0_0000` | pid-1 user binary pages (the shell or a test program) |
| `0x87EF_F000` | user metadata: `[0]` entry VA, `[8]` size in bytes |
| `0x87C0_0000` | filesystem image |
| `0x87BF_F000` | filesystem metadata: `[0]` image PA, `[8]` image size |

`kmain` copies the user pages into fresh PMM pages mapped at `0x4000_0000`, then mounts the FS
from its metadata page.

## What is not yet implemented

- Device tree parsing (boot logs a warning and continues).
- Block-device drivers; the filesystem lives in a RAM image, not on a device.
- SMP / multi-hart bring-up.
- Signals and a capability model; isolation is limited to per-process address spaces.

## Testing

`tests/boot.rs` checks ROM content, the boot sequence, allocator and PMM behaviour, and console
I/O. Integration tests in the workspace `tests/integration/kernel_*.rs` compile a kernel plus
user binaries (and a filesystem image), inject them, and assert on UART output and exit codes
end to end. See `_OS_SPECIFICATION.md` for the memory map, ABI, and boot protocol in detail.
