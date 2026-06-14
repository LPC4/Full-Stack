# os-runtime

Boot firmware, S-mode kernel, standard library, and userspace sources for the Full-Stack VM.

This crate is not a Rust library to link against. It holds the RISC-V assembly (`.s`) and
HLL (`.hll`) sources that the compiler pipeline assembles into a bootable kernel image, plus
the stdlib bundles shared by hosted and freestanding programs. The Rust side (`src/lib.rs`)
only exposes each source file as a compile-time string constant so `hll-to-ir` and
`virtual-machine` can embed them.

## Folder layout

```
boot/       M-mode ROM firmware (assembly): reset stub and hosted trap handler
kernel/     S-mode kernel sources (HLL): traps, memory, processes, syscalls, fs
stdlib/     shared HLL stdlib, split into common / hosted / freestanding bundles
user/       U-mode programs (HLL): shell, line editor, in-VM assembler, cube + fbdemo
src/        thin Rust crate exposing every source file as a string constant
tests/      Rust tests over ROM, boot sequence, allocator, PMM, and subsystems
```

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

  kernel/entry.hll      _kernel_start: calls kmain(), panics if it returns
  kernel/my_kernel.hll  kmain(): console, traps, timer, PLIC, memory diagnostics,
                        heap, PMM, Sv39 VMM, process_init, scheduler_init,
                        spawn pid 1, mount filesystem, enable interrupts, idle WFI
```

After `kmain` enters the idle loop the scheduler takes over: the CLINT timer preempts the
running process every tick, and `sret` resumes whichever process is next in the ready queue.

## Kernel subsystems

| Source | Provides |
|--------|----------|
| `trap_entry.hll` | S-mode stvec prologue/epilogue, dedicated kernel trap stack via sscratch, `trap_init` |
| `trap_handler.hll` | Dispatch for timer (cause 5), external (cause 9), software (cause 1), U-mode ecall (cause 8) |
| `utilities.hll` | `kmalloc`, `kshutdown`, CLINT timer get/set, `plic_init` |
| `checks.hll` | Boot-time `memory_self_test`, `pmm_ops_test` diagnostics |
| `pmm.hll` | Physical page allocator (4 KiB pages, bump + free-list, double-free guard) |
| `vmm.hll` | Sv39 page tables: `vmm_init`, `vmm_enable`, `vmm_map`, `vmm_map_1gib`, `vmm_map_range` |
| `process.hll` | 352-byte PCB (pid, state, saved trap frame, page-table root, parent pid, exit code), per-pid user stack slots, `process_create`, `process_peek_pid` |
| `scheduler.hll` | Round-robin ready queue, `schedule`, queue introspection for exec/wait |
| `syscall.hll` | U-mode ecall dispatch table (see below) |
| `fs.hll` | Inode-based read-write filesystem mounted from an injected image |

## Filesystem

`fs.hll` mounts a contiguous image (superblock, inode table, free-block bitmap, data blocks).
Inodes are 128 bytes and index up to 44 direct 4 KiB blocks (176 KiB max file), enough to hold
executable images. The public API covers `fs_init`, `fs_open`, `fs_read`, `fs_write`,
`fs_close`, `fs_create`, `fs_mkdir`, `fs_rename`, `fs_stat`, and `fs_readdir`, with absolute
path resolution rooted at inode 0.

## Syscalls

U-mode processes trap in via `ecall` (number in `a7`, args in `a0`-`a2`, result back in `a0`).
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

`sys_exec` loads a position-independent flat binary from the FS and maps it at a per-pid 16 MiB
code slot starting at `0x4000_0000` (pid 1 lands at the base), then creates a PCB and enqueues
it. The shell uses `exec` + `pidalive` to run a child and wait for it cooperatively.

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
- `cube.hll` -- maps the framebuffer via `map_fb` and animates a spinning wireframe cube in
  native `f64`, double-buffered so it never flickers. Installed at `/bin/cube.elf`; reads
  WASD key events through `poll_key` to steer the rotation.
- `fbdemo.hll` -- maps the framebuffer and renders a Mandelbrot set in native `f64`, then
  exits. Installed at `/bin/fbdemo.elf`; `run /bin/fbdemo` paints the FB tab, filling in
  progressively as it renders.

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
