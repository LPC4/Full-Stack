/# os-runtime

Bare-metal boot firmware, S-mode kernel, and userspace sources for the Full-Stack VM.
This is not a Rust library to link into other projects; it holds the assembly and HLL sources that the compiler pipeline assembles into a bootable kernel image.

## Directory layout

```
boot/
  startup.s          M-mode ROM entry (_start): PMP, delegation, mret to S-mode
  trap.s             M-mode trap handler (_m_trap): ecall dispatch, sys_write, sys_exit

kernel/
  entry.hll          S-mode entry stub (_kernel_start -> kmain)
  my_kernel.hll      Reference kernel: full boot sequence, user-process spawn, idle loop
  trap_entry.hll     S-mode stvec prologue/epilogue (288-byte trap frame), trap_init
  trap_handler.hll   Trap dispatcher: timer, external IRQ, software, U-mode ecall
  utilities.hll      kmalloc, kshutdown, timer_get/set, plic_init
  checks.hll         memory_self_test, pmm_ops_test (boot-time diagnostics)
  pmm.hll            Physical memory manager (4 KiB pages, bump + free-list)
  vmm.hll            Sv39 VMM: vmm_init, vmm_enable, vmm_map, vmm_map_1gib, vmm_map_range
  process.hll        PCB (328 bytes, 41 u64s), process_init, process_create
  syscall.hll        Syscall dispatch: exit (93), write (64), yield (2)
  scheduler.hll      Round-robin scheduler: scheduler_init, scheduler_add, schedule

stdlib/
  common/
    types.hll          Str, HeapBlock type definitions
    memory_allocator.hll  Bump-pointer allocator with free-list reuse (malloc, free)
    string_utils.hll   str_len, str_equals, str_copy, str_concat
    mem.hll            memset, memcpy, memmove, memcmp
    klog.hll           klog, klog_ok, klog_warn, klog_error, klog_int, klog_hex
  hosted/
    runtime.hll        Linux syscall runtime: _start, putchar, puts, print_int, exit
  freestanding/
    runtime.hll        kpanic (direct UART write + WFI halt)
    console.hll        Direct NS16550A UART I/O: console_putchar, console_write,
                       console_writeln, console_print_int, console_print_hex
    entry.hll          _start wrapper for pure freestanding programs

user/
  user_hello.hll     Example user process: sys_write greeting, then yield loop

src/lib.rs           Rust crate: all sources as compile-time &str constants
tests/boot.rs        ROM, kernel boot, malloc/free, console, PMM, process/scheduler tests
```

## Boot sequence

```
Power-on / VM reset
  boot/startup.s     _start (ROM 0x000): PMP open, medeleg/mideleg to S-mode,
                     mepc = kernel entry address, mret -> S-mode
  boot/trap.s        _m_trap (ROM 0x100): ecall dispatch for hosted tests
                     (sys_write -> UART, sys_exit -> SYSCON halt)

  kernel/entry.hll   _kernel_start: calls kmain(); panics if kmain returns
  kernel/my_kernel.hll  kmain(): boot_console, boot_traps, boot_timer, boot_plic,
                     memory diagnostics, heap, pmm, vmm, process_init,
                     scheduler_init, spawn_user_process, idle WFI loop
```

## What is implemented

**ROM firmware (M-mode)**
- `startup.s`: PMP, exception/interrupt delegation to S-mode, mret into the kernel.
- `trap.s`: M-mode trap handler for hosted programs; dispatches `sys_write` (fd 1 -> UART) and `sys_exit` (SYSCON halt) ecalls.

**Kernel (S-mode)**
- S-mode stvec entry stub: saves all 32 GPRs plus sepc/scause/stval/sstatus into a 288-byte on-stack trap frame; calls `trap_handler`; restores state and `sret`.
- Trap dispatcher: handles Supervisor Timer Interrupt (cause 5), external PLIC interrupts (cause 9), software interrupts (cause 1), and U-mode ecalls (cause 8).
- CLINT timer: `timer_set` arms MTIMECMP for the next tick; timer interrupt re-arms and calls `schedule`.
- PLIC: `plic_init` enables UART source 10 for S-mode context 1.
- Physical memory manager: free-list + bump allocator over a configurable physical range. PMM_FREE_MAGIC guard detects double-free.
- Sv39 VMM: 3-level page tables (L2/L1/L0, 4 KiB pages). `vmm_enable` writes SATP and flushes TLB. Supports 4 KiB page and 1 GiB superpage mappings.
- Process model: 328-byte PCB with initial trap frame (sp, sepc, sstatus) pre-populated for `sret` into U-mode.
- Round-robin scheduler: linked-list ready queue; `schedule(frame, action)` saves the current PCB's trap frame, dequeues the next process, and restores its frame.
- Syscall dispatch: `sys_exit` triggers scheduler exit, `sys_write` writes to UART directly via MMIO (no ecall re-entry), `yield` triggers a voluntary reschedule.

**User process injection**
- The test harness places the user binary at physical address 0x87F00000 and a metadata page (entry VA, size) at 0x87EFF000.
- The kernel copies each user page into a fresh PMM-allocated physical page, maps it at user VA 0x40000000+, creates a PCB, and adds it to the scheduler.
- The first timer interrupt context-switches into the user process via `sret`.

**Stdlib**
- `memory_allocator.hll`: bump-pointer allocator with per-size free-list reuse; `malloc` + `free`.
- `console.hll`: direct UART MMIO writes (no ecall) -- required inside the syscall handler where sp may point at user virtual address space.
- `klog.hll`: `[  OK  ]` / `[ WARN ]` / `[ ERR  ]` formatted log helpers backed by console I/O.

## What is not yet implemented

- Device tree parsing (boot logs `[ WARN ] device tree: not implemented`).
- Filesystem drivers (no block device abstraction yet).
- SMP / multi-hart bring-up.
- Signals, capability model, richer process isolation beyond address-space separation.

## Testing

`tests/boot.rs` verifies ROM source content, kernel boot sequence, malloc/free behaviour,
console I/O correctness, PMM operations, and process/scheduler source structure.
Integration tests in `tests/integration/kernel_*.rs` compile a kernel + user binary and
verify UART output and exit codes end-to-end.

See `_OS_SPECIFICATION.md` for the hardware memory map, calling convention, and boot protocol.
