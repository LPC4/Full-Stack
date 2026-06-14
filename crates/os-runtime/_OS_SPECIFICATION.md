# RISC-V RV64 Bare-Metal Runtime Specification

This document specifies the runtime that lives in `crates/os-runtime/`: the boot
firmware (`boot/`), the reference kernel (`kernel/`), the three standard-library
bundles (`stdlib/`), and the userspace programs (`user/`). It is the contract those
components keep with each other and with the VM. The HLL language, IR, assembler,
and the machine itself are specified elsewhere and are only referenced here:

- HLL language: `crates/hll-to-ir/_LANG_SPECIFICATIONS.md`
- IR: `crates/ir-to-asm/_IR_SPECIFICATIONS.md`
- RISC-V backend and ABI: `crates/asm-to-binary/_RISCV_SPECIFICATIONS.md`
- VM, CPU, and device behavior: `crates/virtual-machine/_VM_SPECIFICATION.md`

## Contents

1. Overview
2. Boot firmware (`boot/`)
3. Kernel initialization (`kernel/entry.hll`, `my_kernel.hll`)
4. Memory management (`vmm.hll`, `pmm.hll`, allocator)
5. Processes and scheduling (`process.hll`, `scheduler.hll`)
6. Traps and interrupts (`trap_entry.hll`, `trap_handler.hll`)
7. Syscall interface (`syscall.hll`, `stdlib/hosted/syscalls.hll`)
8. Filesystem (`fs.hll`)
9. Standard library (`stdlib/`)
10. Userspace programs (`user/`)
- Appendix A: Memory map quick reference
- Appendix B: Syscall quick reference


## 1. Overview

### 1.1 Source map

Every component in the folder is documented by exactly one chapter below. Code
comments cite the chapter rather than repeating its content.

| Path | Component | Chapter |
|------|-----------|---------|
| `boot/startup.s`, `boot/trap.s` | M-mode ROM firmware and trap vectors | 2 |
| `kernel/entry.hll`, `my_kernel.hll` | S-mode entry and `kmain` boot sequence | 3 |
| `kernel/vmm.hll`, `pmm.hll` | Sv39 paging and physical page allocator | 4 |
| `kernel/process.hll`, `scheduler.hll` | PCB, scheduler, fork/exec | 5 |
| `kernel/trap_entry.hll`, `trap_handler.hll` | Trap entry and dispatch | 6 |
| `kernel/syscall.hll`, `stdlib/hosted/syscalls.hll` | Syscall ABI, table, wrappers | 7 |
| `kernel/fs.hll` | Inode filesystem | 8 |
| `kernel/utilities.hll`, `checks.hll` | HAL primitives, boot diagnostics | 3, 9 |
| `stdlib/common/`, `freestanding/`, `hosted/` | Standard-library bundles | 9 |
| `user/shell.hll`, `edit.hll`, `as.hll`, `cc.hll`, `cube.hll`, `fbdemo.hll`, `life.hll` | Userspace programs | 10 |

### 1.2 Layered model

- **ROM (boot firmware).** Immutable M-mode code run at reset. Configures PMP and
  exception/interrupt delegation, sets the M-mode trap vector, and `mret`s into
  S-mode at the kernel entry. Also services hosted `sys_write`/`sys_exit` ecalls
  for non-kernel (freestanding/hosted) programs that run without the kernel.
- **Kernel (S-mode).** Initializes hardware, heap, PMM, and Sv39 paging; installs
  the S-mode trap handler; spawns pid 1; then enters a timer-preempted scheduler.
- **User (U-mode).** Programs run under the kernel with per-process address-space
  isolation, talking to the kernel only via `ecall`.

### 1.3 Target hardware

RV64IMAFD with Zicsr/Zifencei, Sv39 paging, and M/S/U privilege modes. Primary
target is the project VM; QEMU `virt` is a secondary target. Device registers and
timing are defined in the VM spec; the addresses this runtime depends on are
collected in Appendix A.


## 2. Boot firmware (`boot/`)

### 2.1 Image format and linker layout

The kernel image is laid out from `0x8000_0000` as `.text`, `.rodata`, `.data`,
then `.bss` (zeroed by the loader using `__bss_start`/`__bss_end`), followed by a
64 KiB stack to `__stack_top` and the heap. Both an ELF (single `PT_LOAD`,
`p_vaddr == p_paddr == 0x8000_0000`, `RWX`, 4 KiB aligned) and a flat binary
(`.text+.rodata+.data` concatenation, BSS zero-filled by the loader) are produced
from the same linker script; the VM loads either at `0x8000_0000` and jumps to
`_start`. Exported symbols: `_start`, `__bss_start`, `__bss_end`, `__stack_top`,
`__heap_start`, `__image_size`.

### 2.2 M-mode ROM startup (`startup.s`, offset `0x000`)

At reset the VM sets PC to the ROM base and loads the kernel at `0x8000_0000`.
`_start` runs in M-mode and: grants all of memory via PMP (`pmpaddr0 = -1`,
`pmpcfg0 = 0x1F`); delegates exception causes 8/12/13/15 and interrupt causes
1/5/9 to S-mode (`medeleg`/`mideleg`); points `mtvec` at the M-mode trap handler
(offset `0x100`); sets `mstatus.MPP = Supervisor` and `mepc` to the kernel entry
(passed by the loader); and `mret`s into S-mode. It does not clear BSS or set `sp`
-- the VM initializes `sp` to the top of RAM and the kernel's prologues take over.

### 2.3 Trap vectors (`trap.s`)

`trap.s` holds two handlers. The **M-mode** handler (offset `0x100`) services only
hosted-program ecalls: `sys_write` (UART write loop) and `sys_exit` (SYSCON halt);
every other M-mode trap returns via `mret` to be handled in S-mode. Because
illegal-instruction (cause 2) is **not** delegated, a stray illegal instruction
traps here and is treated as exit -- see the assembler NOP-padding note in 10.3.
The **S-mode** entry path is in `trap_entry.hll` (chapter 6).


## 3. Kernel initialization (`kernel/entry.hll`, `my_kernel.hll`)

### 3.1 Entry stub

`_kernel_start` (`entry.hll`) is the S-mode entry. It calls `kmain()` and panics if
`kmain` ever returns.

### 3.2 `kmain` boot sequence

`kmain` (`my_kernel.hll`) runs the boot steps in order:

```
boot_console    UART online, print banner
boot_traps      trap_init: install stvec, enable STIE + SEIE
boot_timer      arm CLINT timer via timer_set(1_000_000)
boot_plic       enable the UART RX IRQ on PLIC S-mode context 1
diagnostics     memory_self_test, pmm_ops_test (checks.hll)
boot_heap       smoke-test kmalloc
boot_pmm        pmm_init(0x8010_0000, 0x87F0_0000) + alloc/free probe
boot_vmm        vmm_init, stamp kernel maps, vmm_enable (write satp, sfence)
process/sched   process_init, scheduler_init
spawn_user      copy + map the pid-1 binary, process_create, scheduler_add
boot_filesystem read FS metadata page, fs_init (mount the image)
boot_interrupts s_enable_interrupts (only after pid 1 is enqueued)
idle            WFI loop; the timer preempts into pid 1
```

### 3.3 Physical memory the kernel programs

RAM is `0x8000_0000` + 128 MiB. The kernel image and its 64 KiB stack sit at the
bottom; `pmm` owns `0x8010_0000 .. 0x87F0_0000`; the injected pid-1 binary, its
metadata, and the filesystem image sit in the top of RAM (Appendix A, and 5.x /
8.4 for the exact pages).


## 4. Memory management (`vmm.hll`, `pmm.hll`, allocator)

### 4.1 Physical page allocator (`pmm.hll`)

`pmm` hands out 4 KiB pages from `[start, end)` set by `pmm_init`. `pmm_alloc`
checks a free list first (each freed page stores a magic + next pointer; a magic
mismatch is skipped as corrupt) and otherwise bumps a pointer; it returns `null` on
exhaustion. `pmm_free` pushes onto the free list. The allocator never returns a
page that is still mapped: pages are only reused after an explicit `pmm_free`.

### 4.2 Kernel heap (`kmalloc` / `malloc`)

`kmalloc` (`utilities.hll`) wraps the stdlib `malloc` and panics on OOM. `malloc`
(`stdlib/common/memory_allocator.hll`) is a bump-plus-free-list allocator over a
static `heap_buffer: u8[65536]` living in kernel BSS -- distinct from `pmm` memory
and from user pages, so heap allocations never alias process address spaces.

### 4.3 Sv39 paging and per-process roots

The kernel runs Sv39 (39-bit VAs, 3-level tables). `vmm_init`/`vmm_enable` build
and activate the boot root. `vmm_map` / `vmm_map_1gib` install leaf/gigapage PTEs
into the *active* root; `vmm_alloc_pt` pulls intermediate tables from `pmm`.

Each process owns a private root (`vmm_new_root` allocates one and stamps the
shared kernel/RAM/MMIO mappings into it via `vmm_map_kernel`). Because every root
carries those kernel mappings, the kernel can run with any process's `satp` active
after a trap. **Isolation is physical, not by VA slotting:** all processes share
the same canonical user VAs (code at `0x4000_0000`, stack top at `0x8000_0000`),
mapped to disjoint physical pages in their own roots. `vmm_fork_copy` clones a
parent's user (U-flagged) pages into a child root for `fork`.

### 4.4 Active root vs `satp`; TLB

Two notions of "current root" exist and must not be confused:

- `satp` is the hardware-translated address space (what loads/stores use).
- `vmm_active_root` is bookkeeping for where `vmm_map` writes PTEs.

`vmm_set_root` updates only `vmm_active_root` (so the kernel can build a child's
tables while still running in the parent's `satp`). `vmm_switch` updates both,
writes `satp`, and issues `sfence.vma` to flush the TLB; the scheduler calls it on
every context switch. The VM's TLB is also flushed on any `satp` change.


## 5. Processes and scheduling (`process.hll`, `scheduler.hll`)

### 5.1 Process control block (PCB)

A process is a 384-byte PCB allocated with `kmalloc`:

```
Offset  Size  Field
0       8     pid             u64, assigned sequentially from 1
8       8     state           0=READY, 1=RUNNING, 2=BLOCKED, 3=EXITED
16      8     next            u64* to next PCB in the ready queue (0 = end)
24      8     user_stack_pa   physical address of the user-stack page
32      8     entry_pc        user entry-point VA
40      288   trap_frame      36 u64: x0..x31, sepc, scause, stval, sstatus
328     8     page_root       physical address of this process's Sv39 root
336     8     parent_pid      parent pid (set by fork/exec; 0 if none)
344     8     exit_code       exit code, read by the parent's wait
352     8     stdout_fd       redirected stdout kernel fd, 0 = console (7.7)
360     8     stdin_fd        redirected stdin kernel fd, 0 = console (7.7)
```

The `trap_frame` layout matches the on-stack frame built by the trap entry (6.1),
so `schedule` saves/restores context with a single `memcpy` of those 288 bytes.

### 5.2 Address space and initial trap frame

`process_create` gives the process a fresh root (4.3), maps 4 stack pages just
below `USER_STACK_BASE` (`0x8000_0000`) with R+W+U, and pre-populates the trap
frame so the first `sret` enters U-mode: `frame[2]` (sp) = stack top,
`frame[32]` (sepc) = entry, `frame[35]` (sstatus) = `0x13` (U-mode, IRQs on). The
full integer/FP register ABI is the standard RISC-V convention; see
`_RISCV_SPECIFICATIONS.md` section 8.

### 5.3 States and the ready queue

The scheduler keeps a FIFO ready queue (`ready_queue_head`), the running process
(`current_process`), an EXITED-but-unreaped list (`zombie_head`), and the single
process blocked on input (`input_waiter`, 6.4). States cycle
READY -> RUNNING -> {READY, BLOCKED, EXITED}.

### 5.4 Round-robin preemption

`schedule(frame, action)` saves the live frame into `current_process`, sets its
next state per `action` (7.3), dequeues the head of the ready queue, switches into
its root (`vmm_switch`), and restores its frame. The CLINT timer interrupt (6.3)
calls `schedule(frame, 1)` every 1,000,000 cycles, giving preemptive round-robin
even for compute-bound processes that never yield.

### 5.5 fork and exec

`fork` (syscall 220) allocates a child PCB and root, copies the parent's user pages
(`vmm_fork_copy`) and trap frame, sets the child's `a0` to 0, and enqueues it.
`exec` (syscall 103) loads a static ELF (7.4) into a fresh per-process root at
`0x4000_0000`, parents the child to its launcher (captured before the root
switches), and enqueues it. `process_peek_pid` reports the next pid to be assigned.

### 5.6 Zombies and reaping

`exit` marks a process EXITED and keeps the PCB on `zombie_head` holding its exit
code until a parent reaps it; the VM halts only when the last runnable process
exits. `wait`/`waitpid` (7.2) reap zombies. Liveness queries:
`scheduler_pid_in_queue` checks the ready queue only; `scheduler_pid_alive`
(backing `pidalive`) also counts the running and input-blocked process. `wait` and
`waitpid` use the alive-including-blocked test (not the ready-queue-only one), so a
child that sleeps on input -- e.g. the editor idling for a keystroke -- leaves the
ready queue but is never misreported to its waiting parent as finished.


## 6. Traps and interrupts (`trap_entry.hll`, `trap_handler.hll`)

### 6.1 S-mode trap entry

`stvec` points at the entry in `trap_entry.hll`. On any S-mode trap it allocates a
288-byte frame on a dedicated kernel stack (switched in via `sscratch`), saves
`x1..x31` and the S-mode CSRs (`sepc`/`scause`/`stval`/`sstatus` at frame offsets
256/264/272/280), calls `trap_handler(frame)`, then restores and `sret`s. The frame
doubles as the process context (5.1).

### 6.2 Dispatch by cause

`trap_handler` switches on `scause`: cause 5 = timer (6.3), cause 8 = U-mode ecall
(syscall dispatch, 7.1), cause 9 = external/UART-RX interrupt (6.4). The ecall path
advances `sepc` by 4 before returning.

### 6.3 Timer preemption

The timer interrupt re-arms `MTIMECMP` (`timer_set`) and calls `schedule(frame, 1)`
to round-robin the ready queue.

### 6.4 UART RX wake model

`readchar` is blocking. When no byte is ready, `sys_readchar_block` records the
caller as `input_waiter`, arms the UART RX interrupt (IER bit 0), rewinds `sepc` so
the ecall re-runs on wake, and returns the BLOCK action (7.3) so other processes
run. An arriving byte raises cause 9; the handler claims/completes the PLIC, masks
the RX IRQ (it stays asserted while the FIFO is non-empty and would otherwise
re-fire), wakes the waiter to the front of the ready queue, and preempts so it runs
next. An idle reader therefore costs no CPU and a compute-bound job gets the full
machine. If a BLOCK has nothing else to run, the scheduler declines it and leaves
the caller running (it retries the read).

### 6.5 Ctrl-C and the input pushback

While the shell blocks in `waitpid`, the still-running branch peeks one UART byte.
A `0x03` tears down that exact foreground pid (`scheduler_kill_pid`) and returns the
`-2` sentinel; any other byte is stashed in a one-deep pushback
(`input_pushback_*`) delivered ahead of the device by the next `readchar`, so an
interactive child still receives its input in order. Ctrl-C is reliable for
compute-bound children and best-effort for input readers (their `readchar` races
the wait peek).


## 7. Syscall interface (`syscall.hll`, `stdlib/hosted/syscalls.hll`)

### 7.1 Calling convention

U-mode programs trap with `ecall`. The number is in `a7`; up to four arguments in
`a0`-`a3`; the result is written back to `a0` in the trap frame. The S-mode handler
catches cause 8, dispatches via `syscall_dispatch`, and advances `sepc` by 4.
Standard numbers follow the Linux RISC-V ABI; the 100-range is project-specific.
`stdlib/hosted/syscalls.hll` provides the userspace `sc_*` wrappers and C-string
helpers that programs link against.

### 7.2 Syscall table

| Number | Name | Arguments | Return | Description |
|--------|------|-----------|--------|-------------|
| `2` | `yield` | -- | -- | Yield the CPU (SCHEDULE action) |
| `46` | `ftruncate` | `a0=fd`, `a1=len` | 0 or -1 | Shrink a file to an exact length |
| `56` | `open` | `a0=path*`, `a1=flags` | fd (>=2) or -1 | Open by absolute path. flags: 0=RO, 1=RW, 2=create |
| `62` | `lseek` | `a0=fd`, `a1=off`, `a2=whence` | new pos | Reposition a file fd. whence 0=SET, 1=CUR, 2=END (used for `>>` append) |
| `57` | `close` | `a0=fd` | 0 | Release a descriptor |
| `63` | `read` | `a0=fd`, `a1=buf*`, `a2=off`, `a3=len` | bytes or -1 | Read a file fd at an explicit offset |
| `64` | `write` | `a0=fd`, `a1=buf*`, `a2=len` | bytes or -1 | fd 0/1 -> redirected stdout file or UART (7.7); fd >=2 -> file at stored position |
| `82` | `rename` | `a0=old*`, `a1=new*` | 0 or -1 | Move a file or directory |
| `83` | `mkdir` | `a0=path*` | inode or -1 | Create a directory |
| `93` | `exit` | `a0=code` | -- | Terminate caller (EXIT_SCHEDULE); PCB lingers as a zombie; halts the VM if last runnable |
| `100` | `readchar` | -- | byte (0-255) | Read one UART byte; **blocks** (sleeps the caller, arming the RX interrupt) until one arrives (6.4) |
| `101` | `readdir` | `a0=path*`, `a1=index`, `a2=name*` | type or -1 | Look up the index-th dir entry; writes its name |
| `102` | `stat` | `a0=path*` | type or -1 | Inode type at a path (1=file, 2=dir) |
| `103` | `exec` | `a0=path*` | pid or -1 | Load a static ELF and enqueue it (5.5, 7.4) |
| `104` | `pidalive` | `a0=pid` | 1 or 0 | 1 while the pid is RUNNING/READY/BLOCKED; 0 for zombie/unknown |
| `105` | `unlink` | `a0=path*` | 0 or -1 | Remove a regular file; refuses directories |
| `106` | `rmdir` | `a0=path*` | 0 or -1 | Remove an empty directory; refuses root/non-empty |
| `107` | `map_fb` | -- | base VA | Map the framebuffer into the caller (7.5) |
| `108` | `poll_key` | -- | event or -1 | Pop a key event from the keyboard device; -1 if none (7.5) |
| `110` | `exec_redir` | `a0=path*`, `a1=arg*`, `a2=out*`, `a3=in*`, `a4=append` | pid or -1 | Exec with stdout/stdin bound to FS files (7.7); `out`/`in` may be null |
| `129` | `kill` | `a0=pid`, `a1=sig` | 0 or -1 | Signal a pid; only `SIGKILL` (9) is honoured (7.6); -1 if not a live killable pid; pid 1 is protected |
| `220` | `fork` | -- | child / 0 / -1 | Clone the caller (5.5) |
| `260` | `wait` | -- | code / -1 / -2 | Reap any exited child; -1 none; -2 Ctrl-C (fork test) |
| `261` | `waitpid` | `a0=pid` | code / -1 / -2 | Reap the specific child pid, else poll / Ctrl-C-tear-down; -1 if not ours |

The shell runs a foreground child by pairing `exec` with `waitpid`
(`sh_wait_foreground(pid)`): it blocks until the child becomes a zombie, reaps that
exact pid, prints `[exit N]`, and resumes. Background jobs (`run <file> &`) skip the
wait, record the pid in an 8-slot job table, and are reaped at the prompt; `jobs`
and `fg <id>` manage them (10.1). Pid-targeted waits ensure a foreground wait never
reaps a background job that finished first.

### 7.3 Scheduler actions

`syscall_dispatch` returns an action that `trap_handler` passes to `schedule`:

| Constant | Value | Meaning |
|----------|-------|---------|
| `SYSACT_CONTINUE` | 0 | Resume the current process unchanged |
| `SYSACT_SCHEDULE` | 1 | Yield: re-enqueue READY, switch to next |
| `SYSACT_EXIT_SCHEDULE` | 2 | Exit: mark EXITED, switch to next |
| `SYSACT_BLOCK` | 3 | Sleep the caller (input wait); not re-enqueued (6.4) |

### 7.4 Executable format (static ELF64)

Filesystem executables are static ELF64 RISC-V binaries with a single `PT_LOAD`
segment. Both the host toolchain and the in-VM assembler (`as.hll`, 10.3) emit
this format, linked so `e_entry`/`p_vaddr` land at `0x4000_0000`.

```
Offset  Size  Field
0       16    e_ident  0x7F 'E' 'L' 'F', ELFCLASS64, ELFDATA2LSB, EV_CURRENT
16      2     e_type   ET_EXEC (2)
18      2     e_machine EM_RISCV (243)
24      8     e_entry  absolute entry VA (0x4000_0000)
32      8     e_phoff  program-header offset (64)
54      2     e_phentsize (56)
56      2     e_phnum  (1)
... one PT_LOAD program header ...
0       4     p_type   PT_LOAD (1)
8       8     p_offset payload file offset (page-aligned, 4096)
16      8     p_vaddr  0x4000_0000
32      8     p_filesz bytes copied from file
40      8     p_memsz  virtual footprint incl. BSS (zero-filled beyond filesz)
```

`exec` validates the ELF magic, then walks each `PT_LOAD`: it maps `p_memsz` bytes
at `p_vaddr` (R+W+X+U), copies `[0, p_filesz)` from `p_offset`, and zeroes the rest
(BSS), then jumps `e_entry`. Convention: `.elf` for executables, `.bin` for raw
flat exports. The shell's `run` pre-checks the magic and reports `not an
executable` otherwise.

### 7.5 Framebuffer and keyboard devices

`map_fb` maps the framebuffer device (`0x1002_0000`, 76 pages: 75 for the
320x240 RGBA8888 buffer plus one control page) into the caller at `0x5000_0000`
(R+W+U) and returns that VA; the device buffer is shared, the mapping is
per-process. The control page exposes three word-write registers:

| Offset | Name | Effect |
| --- | --- | --- |
| `0` | `FILL` | Clear the draw buffer to the written RGBA colour (device-side). |
| `4` | `PRESENT` | Publish the back buffer to the front (no-op when single-buffered). |
| `8` | `DBMODE` | `1` enables double buffering, `0` returns to single buffering. |

`DBMODE` is one global device flag, not per-process, so the kernel reference-counts
framebuffer users (set on the first `map_fb` per process via PCB index 46, cleared
on exit) and resets `DBMODE` to `0` (single-buffered) only when the **last** user
exits. This way a program that maps the framebuffer after a double-buffering one
(e.g. the cube) has exited starts single-buffered instead of drawing into the
hidden back buffer, yet a still-running double-buffered program is left alone and
does not start flickering. A double-buffering program enables `DBMODE` itself right
after `map_fb`. (Concurrent graphical programs still share the one screen and one
`DBMODE`; the kernel does not composite them.)

The runtime has two input devices. **Text** (shell, editor) arrives over the UART
(`0x1000_0000`) and is read with `readchar`; the host GUI forwards printable text,
Enter/Backspace/Tab, and Ctrl+letter (Ctrl+C -> `0x03`). **Events** (graphical
programs like the cube) use `poll_key`, reading the keyboard device
(`0x1007_0000`): it returns a packed event (bit 16 = pressed, bits 15..0 =
scancode) or -1 when empty. The GUI pushes raw key events there while the FB tab is
focused (letters as uppercase ASCII, arrows in `0x80`-`0x83`) and mirrors
Ctrl+letter to the UART so Ctrl-C can still interrupt a graphical foreground job.

### 7.6 Signals (`kill`)

`kill` (syscall `129`) generalises the Ctrl-C teardown into a pid-targeted path a
program can invoke directly. Only `SIGKILL` (9) is implemented and it is
uncatchable: `sys_kill_impl` unlinks the target from the ready queue and marks it
`EXITED` via the same `scheduler_kill_pid` used by the Ctrl-C foreground teardown
(6.5), leaving a zombie its parent reaps. pid 1 (the shell) is protected; any other
signal number or an unknown/non-ready pid returns -1. There is no signal table,
handler delivery, or `SIGTERM`/`SIGINT` catch path yet -- those, plus Ctrl-Z stop
state, remain deferred. The shell exposes this as `kill <pid>` and `kill %<job>`
(the latter maps a job id through the job table); the killed job leaves the table
when `jobs_reap` next runs at the prompt (10.1).

### 7.7 I/O redirection

Each PCB carries two redirect slots, `stdout_fd` and `stdin_fd` (5.1), each a
kernel FS fd or 0 for the console. They are bound at exec time: `exec_redir`
(syscall `110`) opens the requested files **in the parent address space** (before
the root switch, while the path strings are still mapped), then stores the fds on
the freshly created child PCB. `>` truncates the target and seeks to 0; `>>` seeks
to end (`fs_fd_size`); `<` opens read-only.

With a slot bound, the standard syscalls reroute transparently: a fd 0/1 `write`
goes through `sys_stdout_write`, which appends to `stdout_fd` and advances its
position instead of hitting the UART; `readchar` returns the next byte of
`stdin_fd` (advancing its position) and `-1` at EOF, instead of blocking on the
UART. Because the hosted `putchar`/`readchar` already funnel through these
syscalls, existing programs inherit redirection with no code change. The fds are
released in `sys_exit` when the process ends.

The shell parses `> >> <` as space-separated operators in `cmd_exec` and launches
via `sh_launch_redir`.

**Pipes (`a | b | c`, PLAN v3 1.2).** Implemented as a *sequential temp-file
pipeline*, not a blocking ring-buffer object: each stage runs to completion with its
stdout bound to a temp file (`/.pipeN`) that the next stage reads as stdin, reusing
the redirection path above. The shell splits the line on `|` (`cmd_pipe`, max 4
stages), runs each stage via `sh_run_stage` with the connecting temp paths (an
explicit `<`/`>`/`>>` on the end stages overrides the pipe default), waits between
stages, then unlinks the temps. Builtin producers (`ls`/`cat`/`echo`/...) write
through the sink to the temp file; the consumer is either an external filter reading
`sc_readchar` (EOF at the file's end) or builtin `cat`, which reads the temp as its
input source. So `echo hi | cat` and `ls | cat` work. A trailing `&` is accepted and ignored
(pipelines run foreground). **Restriction:** because stages are sequential a
producer fully finishes before its consumer runs -- correct for finite output (all
this system produces) but not a streaming/concurrent pipe. The blocking
`sys_pipe`/`dup2` ring buffer is intentionally not built (no practical need at this
scale).

**Builtin redirection (PLAN v3 1.3).** Shell builtins (`ls cat echo pwd jobs help`)
run inside the shell process, not an exec'd child, so they bypass the per-PCB fd
table. They instead use a shell-side *output sink* and *input source*: two globals
`sh_sink_fd` and `sh_src_fd` (0 = console / none, else an open FS fd). The `sh_out*`
helpers write to the sink; `cat` with no file operand reads the source. Builtins are
run through `sh_run_stage`, which parses the line, opens the `>`/`>>` target as the
sink (truncating with `ftruncate` or seeking to end with `lseek`) and any `<` target
as the source, runs the builtin, then closes both. Only `cat` consumes the source;
the others ignore it. This makes `cat a b >> log`, `ls /home/demo > files.txt`,
`echo hi > note.txt`, and `cat < in.txt` behave like a real shell.


## 8. Filesystem (`fs.hll`)

### 8.1 On-disk layout

The image is a contiguous region of 4 KiB blocks:

| Block | Content |
|-------|---------|
| 0 | Superblock (magic `"HLLFS"`, counts, inode bitmap) |
| 1-8 | Inode table (256 inodes x 128 bytes) |
| 9 | Free-block bitmap (1 bit per data block) |
| 10+ | Data blocks |

### 8.2 Inodes and directories

Each 128-byte inode stores a type (free/file/dir), parent inode, size, a 32-byte
name, and up to 44 direct block pointers (176 KiB max file -- enough for executable
images). Directories store 36-byte entries (32-byte name + inode index). Paths are
absolute and resolved from the root directory at inode 0.

### 8.3 Descriptors and operations

Open files use a 16-slot kernel descriptor table; fds 0 and 1 are reserved for the
UART, so file fds start at 2. `fs_open`/`read`/`write`/`close` plus
`fs_create`/`mkdir`/`rename`/`stat`/`readdir`/`unlink`/`rmdir` back the
corresponding syscalls (7.2).

### 8.4 Image injection

The test harness and GUI place the image in RAM before boot; `boot_filesystem`
reads its metadata page and calls `fs_init` to validate the magic and mount it.
Addresses are in Appendix A.


## 9. Standard library (`stdlib/`)

Three bundles share `common/` and are selected by the compiler's target mode (see
`_LANG_SPECIFICATIONS.md` / `_IR_SPECIFICATIONS.md` for mode selection; this
chapter documents only what the bundles provide).

### 9.1 `common/`

`types` (width aliases), `mem` (`memset`/`memcpy`/`memmove`/`memcmp`),
`memory_allocator` (`malloc`/`free` over the static heap, 4.2), `string_utils`
(`str_*`), and `klog` (`klog_ok`/`warn`/`error`/`int`/`hex` -- formatted UART log).

### 9.2 `freestanding/`

`entry` (`_start` -> `main` -> SYSCON halt), `console` (direct NS16550A MMIO:
`console_putchar`/`write`/`writeln`/`print_int`/`print_hex`), and `runtime`
(`kpanic` = UART message + WFI loop). For bare-metal programs without a kernel.

### 9.3 `hosted/`

`runtime` (`_start` -> `main` -> `exit` via Linux ecalls) and `syscalls` (the
userspace `sc_*` wrappers for the syscalls in 7.2, plus C-string helpers). Linked
by U-mode programs that run under the kernel.

### 9.4 HAL primitives

The kernel bundle calls these directly (no syscall layer); implementations are in
`utilities.hll` and `freestanding/console.hll`. Console: `console_putchar`/`write`/
`writeln`/`print_int`/`print_hex`. Halt/panic: `kshutdown` (write exit code to
SYSCON `0x1001_0000`), `kpanic`. Timer/IRQ: `timer_get` (CLINT MTIME `0x0200_BFF8`),
`timer_set` (MTIMECMP hart 0 `0x0200_4000`), `plic_init` (enable UART source 10 on
S-mode context 1), `trap_init` (install `stvec`, enable STIE+SEIE). MMIO is reached
through `asm {}` blocks (a `li` of the device address then a load/store) because in
S-mode `sp` may hold a user VA and `ecall` is unavailable.


## 10. Userspace programs (`user/`)

### 10.1 Shell (`shell.hll`)

The interactive shell boots as pid 1. Built-ins: `ls [dir]`, `cd`, `cat <f>...`,
`echo`, `pwd`, `edit`, `run` (`[&]` to background), `as`, `touch`, `mkdir`, `rm`,
`rmdir`, `mv`, `jobs`, `fg <id>`, `kill <pid>` / `kill %<job>` (7.6), `help`,
`exit`. Any other line is treated as a program name and resolved via a PATH search
(`cwd`, `/bin`, `/home/demo`, with and without `.elf`), with optional `> file` /
`>> file` / `< file` redirection (7.7). It reads a line over the UART
(`sh_read_line`, echoing and handling Backspace), resolves relative paths against
`cwd` (`sh_join_path`), and dispatches on the first word (`sh_first_word`, tolerant
of leading spaces). Output-producing builtins honour `>`/`>>` via the shell output
sink, and `cat` with no file operand reads its input source (`<` file or a pipe),
so `cat`, `echo | cat`, and `ls | cat` all redirect (7.7). `ls` hides names starting
with `.` (Unix convention), which also keeps the `.pipeN` pipe temps out of a
listing. Foreground programs are run via `exec` + `waitpid`
(7.2); background jobs go in an 8-slot table (`job_pids`, job id = slot + 1).
Note: the table is `i64[8]` and must be indexed with `job_pids[i]` (type-scaled);
raw `base + i` is byte-scaled in HLL (lang spec 4.2) and would overlap slots.

### 10.2 Line editor (`edit.hll`)

`/bin/edit.elf` is an `ed`-style line editor over a line-array model (the GUI
terminal renders no cursor codes, so a visual editor is not possible). Commands:
`p` (numbered, current-line marker), `N` goto, `a`/`i` append/insert, `d [N]`
delete, `c` clear, `r` replace, `s/old/new/`, `w`/`q`/`h`. Launched by the shell's
`edit` with the absolute path as its argument.

### 10.3 In-VM assembler (`as.hll`)

`/bin/as.elf` closes the self-hosting loop: `as <src> <out>` assembles a `.s` file
into a runnable static ELF64 (7.4) inside the VM. It runs a two-pass label resolver
(pass 1 assigns byte offsets, pass 2 encodes), then wraps the flat binary in a
minimal ELF (one `PT_LOAD` at `0x4000_0000`, `e_entry` at the payload base) and
writes `<out>`; `run <out>` then execs it. Subset: `add sub and or xor sll srl sra
slt sltu` (R-type); `mul div divu rem remu` (M-extension); `addi addiw andi ori xori
slti sltiu slli srli srai` (register-immediate); `ld lw lwu lh lhu lb lbu` / `sd sw
sh sb` with `offset(reg)` syntax; `lui auipc`; `li`, `mv`; `j`, `jal jalr call la`;
`beq bne blt bge bltu bgeu`; `nop`, `ecall`, `ret`. Data directives `.text/.data/
.section/.globl/.byte/.word/.zero/.ascii/.asciz`. ABI or `x0`..`x31` register names;
`;`/`#` comments; decimal/hex/negative immediates. Encodings mirror the host
`asm-to-binary` backend.

It appends 8 trailing NOPs after the program. A flat program ending in `ecall` has
no valid instruction after it; the pipeline speculatively fetches the next word,
and the zero page-fill decodes as an illegal instruction. Illegal instructions are
not delegated by `medeleg`, so the trap goes to M-mode (`_m_trap`), which services
it as `sys_exit` to SYSCON and halts the VM -- bypassing the kernel's exit path. The
NOPs keep the speculative fetch valid so the exit `ecall` traps cleanly to S-mode.

### 10.4 In-VM compiler (`cc.hll`)

`/bin/cc.elf` is the self-hosting payoff: `cc <src.hll> <out.s>` compiles a program
in the HLL-0 subset (lang spec Appendix D) to an assembly `.s` file in the subset
`as` (10.3) covers, so the headline demo `cc hello.hll hello.s && as hello.s
hello.elf && hello` builds and runs a program with a toolchain that itself runs
inside the VM. `cc` tokenizes, recursive-descent parses into a flat node array
(statements linked by a `next` field; no heap graph), and walks it with naive
stack-machine codegen. Each function gets a fixed frame -- saved `ra`/`fp` at the
top, one 8-byte slot per local below them, addressed through `fp` -- while the
hardware stack below `sp` holds expression temporaries: a binary operator pushes
its left operand, evaluates the right into `a0`, pops the left into `t0`, and
combines. `main` is emitted as `_start` and exits via the `a7=93` ecall with its
return value; other functions `ret`. `putc` is the only I/O intrinsic: cc emits it
as a callable helper doing `write(1, &ch, 1)` (`a7=64`) and `call`s it like any
function. Integer arithmetic is normalized to 32 bits with a trailing `addiw`. The
frozen codegen target is `user/examples/hello.s`; `user/examples/cc_demo.hll` is a
ready-to-compile pure-HLL-0 sample installed at `/home/src/hello.hll`.

### 10.5 Framebuffer demos (`cube.hll`, `fbdemo.hll`, `life.hll`)

The demo gallery lives in `/home/demo`, reachable by bare name (PATH search, 10.1):
`cube` animates a spinning wireframe cube, `mandelbrot` renders a Mandelbrot set,
and `life` runs Conway's Game of Life on a 40x30 toroidal grid (B3/S23). All three
use `map_fb` (7.5); the cube and Mandelbrot use native `f64` math. The cube and
`life` enable double buffering, then `FILL`-clear and `PRESENT` each frame (no
flicker); the Mandelbrot renders one static frame straight to the single front
buffer. Because `DBMODE` resets to single-buffered once the last framebuffer user
exits (7.5), the Mandelbrot draws correctly when run after the cube/life exit. The
cube reads WASD via
`poll_key`; `life` reads `P` (pause), `R` (reseed), and space (single step). Run
them from the shell and view in the Machine window's FB tab.

### 10.6 Hello and examples

`user_hello.hll` prints a greeting via `sys_write` then yields in a loop (a minimal
pid-1). `user/examples/sum.s` (=55) and `fib.s` (=89) are sample assembly inputs for
the in-VM assembler, installed under `/home`.


## Appendix: Memory map quick reference

| Address | Device / region |
|---------|-----------------|
| `0x0000_0000` | Boot ROM (firmware) |
| `0x0200_0000` | CLINT (MSIP `+0`, MTIMECMP `+0x4000`, MTIME `+0xBFF8`) |
| `0x0C00_0000` | PLIC (S-mode context 1 claim/complete `0x0C20_1004`) |
| `0x1000_0000` | UART (NS16550A; IER `+1`, LSR `+5`) |
| `0x1001_0000` | SYSCON (write exit code to halt) |
| `0x1002_0000` | Framebuffer device (76 pages; control page after pixels) |
| `0x1007_0000` | Keyboard event device |
| `0x8000_0000` | RAM (128 MiB): kernel image + 64 KiB stack at base |
| `0x8010_0000 .. 0x87F0_0000` | PMM page pool |
| `0x87C0_0000` | Filesystem image |
| `0x87BF_F000` | FS metadata (PA, size) |
| `0x87EF_F000` | User metadata (entry VA, size) |
| `0x87F0_0000` | pid-1 user binary pages |

User virtual addresses (per process, in its own root): code `0x4000_0000`, stack
top `0x8000_0000`, framebuffer `0x5000_0000`.

