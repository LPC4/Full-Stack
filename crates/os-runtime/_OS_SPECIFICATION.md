# RISC-V RV64 Bare-Metal Kernel Specification

**Version:** 1.0.0  
**Target Architecture:** RISC-V 64-bit (RV64IMAFD) with Machine/Supervisor Privilege and Sv39 Virtual Memory  
**Document Purpose:** Defines the contract between the HLL compiler/runtime and bare-metal kernel code. Covers boot protocol, image format, ABI, runtime split, and hardware abstraction layer for OS development.

---

## 1. Overview

### 1.1 Design Goals
This specification establishes a clean separation between **hosted application code** (user-space programs using Linux syscalls) and **freestanding kernel code** (bare-metal OS running directly on RISC-V hardware). The goal is to enable the same HLL compiler toolchain to produce both types of binaries without conflict.

**Key Principles:**
- **No hidden dependencies:** Freestanding code must not implicitly rely on host OS services
- **Explicit hardware access:** All I/O goes through defined platform primitives
- **Predictable boot flow:** Kernel entry state is fully specified and reproducible
- **Compiler-enforced safety:** Bare-metal mode rejects hosted-only constructs at compile time

### 1.2 Target Platforms
- **Primary:** Project's built-in RV64IMAFD virtual machine (with MMU, CLINT, PLIC, UART)
- **Secondary:** QEMU `virt` machine (RV64 system emulation)
- **Future:** Real RISC-V hardware (SiFive HiFive, StarFive VisionFive, etc.)

All platforms share the same memory map and device layout defined in Section 3.

### 1.3 Layered view: ROM, Kernel, and the eventual OS

To make the relationship between firmware, kernel and the higher-level OS clearer we adopt a layered view:

- ROM (boot firmware)
  - Role: Minimal, immutable code executed at reset. Configures CPU state (PMP, exception/interrupt delegation), sets up the M-mode trap vector, and drops to Supervisor mode via `mret`. ROM runs in Machine mode and handles hosted `sys_write` and `sys_exit` ecalls for non-kernel programs.
  - Where to find it in the repo: `crates/os-runtime/boot/startup.s` (M-mode entry, offset 0x000) and `boot/trap.s` (M-mode trap handler, offset 0x100). The VM assembles these into its ROM image at startup.
  - What is implemented: full PMP grant, `medeleg`/`mideleg` delegation to S-mode, `mtvec` setup, `mepc` set to the kernel entry address, and `mret` into S-mode. `trap.s` handles `sys_write` (UART write loop) and `sys_exit` (SYSCON halt) for the hosted runtime.

- Kernel (S-mode kernel code)
  - Role: Initialize hardware (UART, CLINT, PLIC), set up the heap and PMM, configure and enable Sv39 paging, install the S-mode trap handler, spawn user processes, and enter the preemptive scheduler idle loop.
  - Where to find it in the repo: `crates/os-runtime/kernel/` -- `entry.hll` (S-mode entry stub), `my_kernel.hll` (reference kernel / `kmain`), `trap_entry.hll` (stvec prologue/epilogue), `trap_handler.hll`, `utilities.hll` (kmalloc, timer, PLIC), `checks.hll` (boot diagnostics), `pmm.hll`, `vmm.hll`, `process.hll`, `syscall.hll`, `scheduler.hll`. Shared stdlib lives under `crates/os-runtime/stdlib/`.
  - What is implemented: full boot sequence including MMU enable (Sv39 canonical lower-half identity mapping), PCB-based process creation with VMM-mapped user pages, round-robin preemptive scheduler driven by CLINT timer interrupts, and syscall dispatch (`sys_exit`, `sys_write`, `yield`).

- User processes and services
  - Role: User-mode programs that run under kernel supervision with address-space isolation. Communicate with the kernel via ecall.
  - Where to find it in the repo: `crates/os-runtime/user/` (example programs). The test harness injects user binaries by placing them at physical address 0x87F00000; the kernel reads metadata, copies pages, maps them, creates a PCB, and adds it to the scheduler.
  - What is implemented: `user_hello.hll` (prints a greeting via `sys_write`, then yields in a loop). The injection mechanism and user-process lifecycle (create, run, exit) work end-to-end in integration tests.
  - Not yet implemented: filesystem, block devices, signals, multi-hart support.

The remainder of this specification documents the machine model, calling conventions and ABI that the kernel and eventual OS must follow.

---

## 2. Compiler Target Modes

The HLL compiler operates in three modes, selected via `TargetMode` in the API or `--mode` on the `fsc` CLI:

### 2.1 Hosted Mode (Default)
```bash
fsc run program.hll --mode hosted
```

**Characteristics:**
- Links against the hosted stdlib (`types`, `memory_allocator`, `string_utils`, `runtime`).
- Uses Linux syscalls for I/O (`ecall` with `a7=64` for write, `a7=93` for exit).
- Entry point: `_start` (from `stdlib/hosted/runtime.hll`) calls `main()`, then `exit(return_code)`.
- Console output: `putchar`, `printf` use `sys_write(fd=1, ...)`.
- Heap allocation via `malloc`/`free` provided by `memory_allocator.hll`.

**Use Case:** User-space applications, algorithm tests, educational examples.

### 2.2 Freestanding Mode
```bash
fsc run program.hll --mode freestanding
```

**Characteristics:**
- Links against the freestanding stdlib (`types`, `memory_allocator`, `string_utils`, `runtime`, `console`, `entry`).
- Entry point: `_start` (from `stdlib/freestanding/entry.hll`) calls `main()`, then halts via SYSCON.
- Console I/O via direct NS16550A UART MMIO writes (no ecall).
- Panic via `kpanic` (direct UART write + WFI loop).
- No Linux syscalls; all I/O is explicit.

**Use Case:** Bare-metal programs without a kernel, firmware utilities.

### 2.3 Kernel Mode
```bash
fsc link src/*.hll --mode freestanding   # kernel images use Kernel internally
```

**Characteristics:**
- Links against the full kernel stdlib bundle (`types`, `memory_allocator`, `string_utils`, `mem`,
  `runtime`, `console`, `klog`, `trap_entry`, `utilities`, `checks`, `entry`, `trap_handler`,
  `pmm`, `vmm`, `process`, `syscall`, `scheduler`).
- Entry point: `_kernel_start` (from `kernel/entry.hll`) calls `kmain()`.
- No Linux syscalls; all I/O and hardware access is via MMIO or the provided HAL primitives.
- Full kernel infrastructure provided: PMM, Sv39 VMM, trap handling, scheduler, syscall dispatch.

**Use Case:** OS kernels, the reference `my_kernel.hll`, integration tests.

---

## 3. Hardware Platform

### 3.1 ISA and Extensions
- **Base ISA:** RV64I (64-bit integer)
- **Extensions:** M (multiply/divide), A (atomics), F (single-precision FP), D (double-precision FP)
- **Privileged Extensions:** Zicsr (CSR access), Zifencei (instruction fence)
- **Virtual Memory:** Sv39 (39-bit virtual addresses, 3-level page tables)
- **Privilege Modes:** Machine (M), Supervisor (S), User (U) - kernel starts in M-mode

### 3.2 Memory Map

| Address Range | Device | Size | Description |
|---------------|--------|------|-------------|
| `0x0000_0000` - `0x0FFF_FFFF` | ROM | 256 MB | Boot ROM / firmware (read-only) |
| `0x1000_0000` - `0x1000_0007` | UART | 8 bytes | NS16550A serial console |
| `0x0200_0000` - `0x0200_FFFF` | CLINT | 64 KB | Core Local Interruptor (timer + IPI) |
| `0x0C00_0000` - `0x0CFF_FFFF` | PLIC | 16 MB | Platform-Level Interrupt Controller |
| `0x8000_0000` - `0xFFFF_FFFF` | RAM | 2 GB | Main memory (DRAM) |

**Notes:**
- All addresses are **physical** until the kernel enables Sv39 paging
- ROM contains minimal boot firmware (not part of the kernel)
- UART, CLINT, PLIC are memory-mapped I/O (MMIO) devices
- RAM is zero-initialized except where the kernel image is loaded

### 3.3 UART (Serial Console)

**Base Address:** `0x1000_0000`  
**Model:** NS16550A subset (8 registers, 8-byte stride)

| Offset | Register | Access | Description |
|--------|----------|--------|-------------|
| `0x00` | THR/RBR | W/R | Transmitter Holding / Receiver Buffer |
| `0x01` | IER | R/W | Interrupt Enable Register |
| `0x02` | IIR/FCR | R/W | Interrupt Identification / FIFO Control |
| `0x03` | LCR | R/W | Line Control Register |
| `0x04` | MCR | R/W | Modem Control Register |
| `0x05` | LSR | R | Line Status Register |
| `0x06` | MSR | R | Modem Status Register |
| `0x07` | SCR | R/W | Scratch Register |

**Minimal Implementation:**
```hll
; Write a byte to UART (blocking)
uart_putchar: (c: i32) -> () {
    ; Wait until TX holding register is empty (LSR bit 5 = 1)
    while (u8(0x10000005) & 0x20) == 0 {
        ; spin
    }
    ; Write character to THR
    u8(0x10000000) = u8(c)
}

; Read a byte from UART (non-blocking, returns -1 if empty)
uart_getchar: () -> i32 {
    if (u8(0x10000005) & 0x01) != 0 {
        return i32(u8(0x10000000))
    }
    return -1
}
```

### 3.4 CLINT (Timer and Interprocessor Interrupts)

**Base Address:** `0x0200_0000`

| Offset | Register | Access | Description |
|--------|----------|--------|-------------|
| `0x0000` | MSIP | R/W | Machine Software Interrupt Pending (per-hart) |
| `0x4000` | MTIMECMP | R/W | Machine Timer Compare (per-hart, 64-bit) |
| `0xBFF8` | MTIME | R/W | Machine Time (global, 64-bit, free-running) |

**Timer Behavior:**
- `MTIME` increments every clock cycle (simulated as instruction count in VM)
- When `MTIME >= MTIMECMP`, sets `MIP.MTIP` (machine timer interrupt pending)
- Writing to `MTIMECMP` clears the interrupt if condition no longer holds

**Example: Set timer interrupt for 1 second (assuming 10 MHz clock)**
```hll
set_timer_interrupt: (interval_cycles: u64) -> () {
    current_time: u64 = *((u64*)0x0200BFF8)
    *((u64*)0x02004000) = current_time + interval_cycles
}
```

### 3.5 PLIC (External Interrupt Controller)

**Base Address:** `0x0C00_0000`

**Purpose:** Routes external interrupts (e.g., UART RX, disk I/O) to CPU harts with priority arbitration.

**Memory Layout:**
- `0x0000-0x007C`: Priority registers (32 sources x 4 bytes)
- `0x1000-0x107C`: Pending bits (bitfield, 1 bit per source)
- `0x2000-0x207C`: Enable bits (per-context, 1 bit per source)
- `0x200000`: Threshold register (per-context)
- `0x200004`: Claim/Complete register (per-context)

**Operation:**
1. External device calls `plic_set_irq(source_id)` to assert interrupt
2. CPU checks `plic_claim(hart_id)` before each instruction fetch
3. Handler reads claim register -> returns highest-priority pending IRQ
4. Handler writes IRQ ID to complete register -> clears pending bit

**Example: Claim next interrupt**
```hll
plic_claim: (hart_id: u64) -> u32 {
    claim_addr: u64 = 0x0C200004 + (hart_id * 0x1000)
    return *((u32*)claim_addr)
}

plic_complete: (hart_id: u64, irq_id: u32) -> () {
    complete_addr: u64 = 0x0C200004 + (hart_id * 0x1000)
    *((u32*)complete_addr) = irq_id
}
```

---

## 4. Kernel Image Format

The compiler produces one of two formats, selected at build time:

### 4.1 ELF Format (Preferred)

**Format:** 64-bit ELF, little-endian, RISC-V machine type (EM_RISCV = 243)

**Sections:**
- `.text`: Executable code (read + execute)
- `.rodata`: Read-only data (strings, constants)
- `.data`: Initialized data (read + write)
- `.bss`: Zero-initialized data (read + write, not stored in file)

**Program Headers:**
- Single `PT_LOAD` segment covering all sections
- Flags: `PF_R | PF_W | PF_X` (read + write + execute)
- Virtual address (p_vaddr) = Physical address (p_paddr) = `0x8000_0000`
- Alignment: 4096 bytes (page-aligned)

**Entry Point:** Symbol `_start` (address stored in ELF header `e_entry`)

**Required Symbols (exported by linker):**

| Symbol | Type | Description |
|--------|------|-------------|
| `_start` | Function | Kernel entry point (provided by freestanding runtime) |
| `__bss_start` | Address | Start of BSS section (inclusive) |
| `__bss_end` | Address | End of BSS section (exclusive) |
| `__stack_top` | Address | Top of initial kernel stack (grows downward) |
| `__heap_start` | Address | Start of available heap memory (optional) |
| `__image_size` | Value | Total size of kernel image in bytes |

**Loading:**
- QEMU: `qemu-system-riscv64 -kernel kernel.elf -nographic`
- VM: Loader parses ELF headers, loads segments into RAM at `p_paddr`
- Bootloader: Custom loader must parse ELF and jump to `e_entry`

### 4.2 Flat Binary Format

**Format:** Raw memory image (no headers, no metadata)

**Contents:**
- Concatenation of `.text` + `.rodata` + `.data` sections (in that order)
- BSS is **not** included (must be zero-filled by loader based on `__bss_start`/`__bss_end`)

**Entry Point:** First byte of the image (assumed to be at address `0x8000_0000`)

**Loading:**
- Copy binary to `0x8000_0000` in RAM
- Zero-fill BSS from `__bss_start` to `__bss_end` (symbols embedded in binary or provided separately)
- Jump to `0x8000_0000`

**Use Case:** Simple bootloaders, direct ROM programming, minimal environments

### 4.3 Linker Script

Both formats use the same linker script to control layout:

```ld
/* kernel.ld */
ENTRY(_start)

SECTIONS {
    /* Load address: start of RAM */
    . = 0x80000000;
    
    /* Text section: executable code */
    .text : {
        *(.text)
        *(.text.*)
    }
    
    /* Read-only data: strings, constants */
    .rodata : {
        *(.rodata)
        *(.rodata.*)
    }
    
    /* Data section: initialized globals */
    .data : {
        *(.data)
        *(.data.*)
    }
    
    /* BSS section: zero-initialized globals */
    __bss_start = .;
    .bss : {
        *(.bss)
        *(.bss.*)
        *(COMMON)
    }
    __bss_end = .;
    
    /* Stack: 64 KB, aligned to 16 bytes */
    . = ALIGN(16);
    . += 0x10000;
    __stack_top = .;
    
    /* Heap starts after stack (kernel can adjust this) */
    __heap_start = .;
    
    /* Discard unnecessary sections */
    /DISCARD/ : {
        *(.comment)
        *(.note*)
    }
}
```

---

## 5. Boot Protocol

### 5.1 Initial State (Before Kernel Entry)

When the boot ROM/firmware transfers control to the kernel:

**Register State:**
- `a0`: Hart ID (always 0 for single-core systems)
- `a1`: Device tree blob pointer (or 0 if not provided)
- `a2`-`a7`: Undefined (do not rely on these values)
- `sp`: Undefined (kernel must set up its own stack)
- All other registers: Undefined

**CSR State:**
- `mstatus`: MIE = 0 (interrupts disabled), MPIE = 0, MPP = 3 (Machine mode)
- `mie`: All bits = 0 (no interrupts enabled)
- `mip`: All bits = 0 (no pending interrupts)
- `mtvec`: Undefined (kernel must set trap vector)
- `satp`: MODE = 0 (Bare, no virtual memory translation)
- `mepc`, `mcause`, `mtval`: Undefined

**Memory State:**
- Kernel image loaded at `0x8000_0000` (ELF segments or flat binary)
- BSS region (`__bss_start` to `__bss_end`) may contain garbage (kernel must zero it)
- Rest of RAM (`0x8000_0000 + image_size` to `0xFFFF_FFFF`) is available for allocation
- Devices (UART, CLINT, PLIC) are in reset state

**Privilege Mode:** Machine mode (highest privilege)

### 5.2 Boot Sequence

The boot process follows this sequence:

```
1. VM / Hardware reset
   +- PC set to ROM base (0x0000_0000)
   +- Kernel ELF loaded into RAM at 0x8000_0000

2. ROM _start (boot/startup.s, M-mode, offset 0x000)
   +- PMP: open all-address grant (pmpaddr0 = -1, pmpcfg0 = 0x1F)
   +- medeleg: delegate exception causes 8, 12, 13, 15 to S-mode
   +- mideleg: delegate interrupt causes 1, 5, 9 to S-mode
   +- mtvec = ROM offset 0x100 (_m_trap)
   +- mstatus.MPP = Supervisor (01)
   +- mepc = kernel entry address (passed in a0 by the VM loader)
   +- mret -> drops to S-mode, jumps to kernel entry

3. _kernel_start (kernel/entry.hll, S-mode)
   +- Calls kmain()
   +- If kmain returns: calls kpanic (should never happen)

4. kmain() (kernel/my_kernel.hll, S-mode)
   +- boot_console: UART online, log initial banner
   +- boot_traps:   install S-mode stvec (trap_init), enable STIE + SEIE
   +- boot_timer:   arm CLINT timer via timer_set(1_000_000)
   +- boot_plic:    enable UART IRQ on PLIC context 1
   +- memory diagnostics: memory_self_test, pmm_ops_test
   +- boot_heap:    smoke-test kmalloc
   +- boot_pmm:     pmm_init(0x8010_0000, 0x87F0_0000); alloc/free probe
   +- boot_vmm:     vmm_init; 1 GiB identity maps for low/RAM/high ranges;
                    vmm_enable (write SATP, sfence.vma)
   +- process_init, scheduler_init
   +- spawn_user_process: read metadata, copy pages, map U-mode VAs,
                          process_create, scheduler_add
   +- boot_interrupts: s_enable_interrupts
   +- Idle WFI loop (scheduler takes over via timer preemption)

5. M-mode trap handler (boot/trap.s, offset 0x100)
   +- Handles ecalls from hosted programs only (sys_write, sys_exit)
   +- All other traps -> mret (passed back to S-mode handler)
```

### 5.3 S-mode Trap Entry

The stvec is pointed at `stvec_entry` (inside `_s_trap_host` in `trap_entry.hll`).
On any S-mode trap or interrupt the CPU jumps here:

```assembly
; Allocate 288-byte trap frame on the kernel stack
addi  sp, sp, -288
; Save x1..x31 (x0 is always zero, skip)
sd    x1, 8(sp)
; x2 (sp): save original sp = sp + 288
addi  t0, sp, 288
sd    t0, 16(sp)
; ... remaining registers ...
; Save S-mode CSRs at offsets 256-280
csrr  t0, sepc    ; offset 256
csrr  t0, scause  ; offset 264
csrr  t0, stval   ; offset 272
csrr  t0, sstatus ; offset 280
; Call HLL trap handler with frame pointer in a0
mv    a0, sp
call  trap_handler
; Restore CSRs and GPRs, then sret
```

The trap frame is also the process context: `schedule()` copies it in and out of the PCB to perform context switches.

**Note:** The ROM `_start` stub does not clear BSS or set sp before mret. The kernel's HLL function prologues establish stack frames relative to sp, which the VM initialises to the top of RAM before execution begins.

---

## 6. Calling Convention and ABI

The kernel uses the standard RISC-V calling convention without modification:

### 6.1 Integer Register Usage

| Register | ABI Name | Caller/Callee Saved | Purpose |
|----------|----------|---------------------|---------|
| `x0` | `zero` | Hardwired 0 | Constant zero |
| `x1` | `ra` | Caller-saved | Return address |
| `x2` | `sp` | Caller-saved | Stack pointer |
| `x3` | `gp` | Caller-saved | Global pointer (not used) |
| `x4` | `tp` | Caller-saved | Thread pointer (not used) |
| `x5`-`x7` | `t0`-`t2` | Caller-saved | Temporaries |
| `x8` | `s0`/`fp` | Callee-saved | Saved register / Frame pointer |
| `x9` | `s1` | Callee-saved | Saved register |
| `x10`-`x11` | `a0`-`a1` | Caller-saved | Arguments / Return values |
| `x12`-`x17` | `a2`-`a7` | Caller-saved | Arguments |
| `x18`-`x27` | `s2`-`s11` | Callee-saved | Saved registers |
| `x28`-`x31` | `t3`-`t6` | Caller-saved | Temporaries |

### 6.2 Floating-Point Register Usage

| Register | ABI Name | Caller/Callee Saved | Purpose |
|----------|----------|---------------------|---------|
| `f0`-`f7` | `ft0`-`ft7` | Caller-saved | FP temporaries |
| `f8`-`f9` | `fs0`-`fs1` | Callee-saved | FP saved registers |
| `f10`-`f17` | `fa0`-`fa7` | Caller-saved | FP arguments / Return values |
| `f18`-`f27` | `fs2`-`fs11` | Callee-saved | FP saved registers |
| `f28`-`f31` | `ft8`-`ft11` | Caller-saved | FP temporaries |

### 6.3 Stack Layout

**Stack Growth:** Downward (from high addresses to low addresses)

**Alignment:** 16-byte aligned at all times (RISC-V requirement)

**Frame Structure:**
```
High addresses
+----------------------+
|   Caller's frame     |
+----------------------+
|   Return address (ra)| <- sp + N - 8
|   Saved registers    | <- sp + N - 16, sp + N - 24, ...
|   Local variables    | <- sp + 0, sp + 8, ...
+----------------------+
|   Current frame      | <- sp (16-byte aligned)
+----------------------+
Low addresses
```

**No Red Zone:** RISC-V does not have a red zone. The compiler will not access memory below `sp` without adjusting it first.

### 6.4 Function Prologue/Epilogue

**Typical Prologue:**
```assembly
addi   sp, sp, -N          # Allocate stack frame (N is multiple of 16)
sd     ra, N - 8(sp)       # Save return address
sd     s0, N - 16(sp)      # Save callee-saved registers as needed
addi   s0, sp, N           # Set frame pointer
```

**Typical Epilogue:**
```assembly
ld     ra, N - 8(sp)       # Restore return address
ld     s0, N - 16(sp)      # Restore callee-saved registers
addi   sp, sp, N           # Deallocate stack frame
jalr   zero, 0(ra)         # Return to caller
```

The HLL compiler generates this automatically.

---

## 7. Runtime Split

The compiler provides three mutually exclusive runtime bundles (see Section 2 for mode selection):

### 7.1 Hosted Runtime

**Source:** `stdlib/hosted/runtime.hll`, `stdlib/common/{types,memory_allocator,string_utils}.hll`

**Entry flow:**
```
_start (runtime.hll)  ->  main() (user code)  ->  exit(code) via sys_exit ecall
```

**Provided symbols:** `_start`, `putchar`, `puts`, `print_int`, `exit`, `malloc`, `free`, `str_*`.

**Use Case:** Educational examples, algorithm tests, user-space tools.

### 7.2 Freestanding Runtime

**Source:** `stdlib/freestanding/{runtime,console,entry}.hll`, `stdlib/common/{types,memory_allocator,string_utils}.hll`

**Entry flow:**
```
_start (entry.hll)  ->  main() (user code)  ->  SYSCON halt
```

**Provided symbols:**
| Symbol | Description |
|--------|-------------|
| `_start` | Calls `main()`, then writes to SYSCON to halt |
| `kpanic` | Writes message to UART (direct MMIO), then WFI loop |
| `_kpanic` | Minimal panic with no message (pre-init safe) |
| `console_putchar` | Single-byte write to NS16550A at 0x10000000 |
| `console_write` | Null-terminated string write to UART |
| `console_writeln` | `console_write` + newline |
| `console_print_int` | Decimal integer to UART |
| `console_print_hex` | 64-bit hex to UART (16 digits, `0x` prefix) |
| `malloc` / `free` | Bump-pointer allocator with free-list |
| `memset` / `memcpy` / `memmove` / `memcmp` | Low-level memory ops |

**Use Case:** Bare-metal programs, firmware utilities, simple MMIO tests.

### 7.3 Kernel Runtime

**Source:** All freestanding sources plus the full kernel bundle from `crates/os-runtime/kernel/`.

**Entry flow:**
```
_kernel_start (entry.hll)  ->  kmain() (my_kernel.hll / user kernel)
```

Everything in the freestanding bundle plus:

| Symbol | Source | Description |
|--------|--------|-------------|
| `_kernel_start` | `entry.hll` | S-mode entry; calls `kmain`, panics on return |
| `klog_ok` / `klog_warn` / `klog_error` | `klog.hll` | Formatted kernel log to UART |
| `klog_int` / `klog_hex` | `klog.hll` | Labelled integer/hex log |
| `kmalloc` | `utilities.hll` | `malloc` wrapper that panics on OOM |
| `kshutdown` | `utilities.hll` | Write exit code to SYSCON (halts VM/QEMU) |
| `timer_get` / `timer_set` | `utilities.hll` | CLINT MTIME / MTIMECMP access |
| `plic_init` | `utilities.hll` | PLIC S-mode setup for UART IRQ (source 10) |
| `memory_self_test` / `pmm_ops_test` | `checks.hll` | Boot-time diagnostics |
| `trap_init` | `trap_entry.hll` | Install stvec, enable STIE + SEIE |
| `trap_handler` | `trap_handler.hll` | Timer / IRQ / ecall dispatcher |
| `pmm_init` / `pmm_alloc` / `pmm_free` | `pmm.hll` | 4 KiB page allocator |
| `vmm_init` / `vmm_enable` / `vmm_map` / `vmm_map_1gib` | `vmm.hll` | Sv39 page table management |
| `process_init` / `process_create` | `process.hll` | PCB allocation and initialisation |
| `syscall_dispatch` | `syscall.hll` | U-mode ecall handler (exit / write / yield) |
| `scheduler_init` / `scheduler_add` / `schedule` | `scheduler.hll` | Round-robin preemptive scheduler |

**Use Case:** OS kernels, integration tests, the reference `my_kernel.hll`.

---

## 8. Hardware Abstraction Layer (HAL)

The kernel **must** provide a small set of platform primitives that replace hosted I/O functions. These are the only hardware-specific functions the compiler runtime depends on.

### 8.1 Console and Halt Primitives

These are provided by the kernel stdlib bundle (`console.hll`, `utilities.hll`).
Kernel code calls them directly; they do not go through any syscall layer.

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `console_putchar` | `(c: i32) -> ()` | Write one byte to NS16550A UART TX (0x10000000). Direct MMIO, no ecall. |
| `console_write` | `(str: u8*) -> ()` | Write null-terminated string to UART. |
| `console_writeln` | `(str: u8*) -> ()` | `console_write` followed by a newline. |
| `console_print_int` | `(n: i64) -> ()` | Decimal integer to UART. |
| `console_print_hex` | `(n: u64) -> ()` | 64-bit hex to UART (`0x` prefix, 16 digits). |
| `kshutdown` | `(code: i64) -> ()` | Write exit code to SYSCON (0x10010000); halts VM/QEMU. |
| `kpanic` | `(msg: u8*) -> ()` | Write message to UART then WFI loop (never returns). |

### 8.2 Timer and Interrupt Primitives

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `timer_get` | `() -> u64` | Read current MTIME from CLINT (0x0200_BFF8). |
| `timer_set` | `(interval: u64) -> ()` | Set MTIMECMP to MTIME + interval for hart 0. |
| `plic_init` | `() -> ()` | Enable UART source 10 on PLIC S-mode context 1, threshold 0. |
| `trap_init` | `() -> ()` | Point stvec at `stvec_entry`; enable STIE + SEIE in `sie`. |

### 8.3 Implementation Example

The HAL primitives are provided by the kernel stdlib bundle.
The implementations below are from `utilities.hll` and `stdlib/freestanding/console.hll`.

```hll
; Write a single character directly to NS16550A UART TX (0x10000000).
; Direct MMIO -- never use ecall here; S-mode sp may point at user VA space.
console_putchar: (c: i32) -> () {
    asm {
        li   t0, 0x10000000
        sb   a0, 0(t0)
    }
}

; Halt by writing the exit code to SYSCON (0x10010000).
; The VM stops on this write; the WFI loop is a safety net for real hardware.
kshutdown: (code: i64) -> () {
    asm {
        li   t0, 268500992   ; 0x10010000
        sd   a0, 0(t0)
    .Lkshutdown_halt:
        wfi
        j    .Lkshutdown_halt
    }
}

; Read MTIME counter from CLINT (0x0200_BFF8).
timer_get: () -> u64 {
    asm { li t0, 33603576 ; ld a0, 0(t0) }
}

; Set MTIMECMP for hart 0 to MTIME + interval cycles (0x0200_4000).
timer_set: (interval: u64) -> () {
    asm {
        li  t0, 33603576    ; CLINT MTIME
        ld  t1, 0(t0)
        add t1, t1, a0
        li  t0, 33570816    ; CLINT MTIMECMP hart 0
        sd  t1, 0(t0)
    }
}
```

---

## 9. Syscall Interface

U-mode processes communicate with the kernel via the `ecall` instruction.
The S-mode trap handler catches cause 8 (U-mode ecall), dispatches via `syscall_dispatch`,
and advances `sepc` by 4 before returning.

### 9.1 Calling Convention

Syscall number in `a7`; up to three arguments in `a0`-`a2`; return value written back to `a0` in the trap frame.

### 9.2 Syscall Table

| Number | Name | Arguments | Return | Description |
|--------|------|-----------|--------|-------------|
| `64` | `sys_write` | `a0=fd`, `a1=buf*`, `a2=len` | bytes written | Write `len` bytes from `buf` to UART (fd is ignored) |
| `93` | `sys_exit` | `a0=code` | -- | Terminate the calling process; triggers EXIT_SCHEDULE action |
| `2` | `sys_yield` | -- | -- | Voluntarily yield the CPU; triggers SCHEDULE action |

### 9.3 Scheduler Actions

`syscall_dispatch` returns an action code that `trap_handler` passes to `schedule`:

| Constant | Value | Meaning |
|----------|-------|---------|
| `SYSACT_CONTINUE` | 0 | Resume current process unchanged |
| `SYSACT_SCHEDULE` | 1 | Yield: re-enqueue as READY, switch to next process |
| `SYSACT_EXIT_SCHEDULE` | 2 | Exit: mark EXITED, do not re-enqueue, switch to next |

---

## 10. Process Model

### 10.1 Process Control Block (PCB)

Each process is represented by a 328-byte PCB allocated with `kmalloc`.

```
Offset  Size  Field
------  ----  -----
0       8     pid             (u64, assigned sequentially from 1)
8       8     state           (0=READY, 1=RUNNING, 2=BLOCKED, 3=EXITED)
16      8     next            (u64* to next PCB in ready queue, 0 = end)
24      8     user_stack_pa   (physical address of the user-stack 4 KiB page)
32      8     entry_pc        (user-space entry point virtual address)
40      288   trap_frame      (36 u64s: x0..x31, sepc, scause, stval, sstatus)
```

The `trap_frame` layout matches the on-stack frame built by `stvec_entry`, so
`schedule` can `memcpy(pcb+40, frame, 288)` to save and `memcpy(frame, pcb+40, 288)` to restore.

### 10.2 Initial Trap Frame

`process_create` pre-populates the trap frame so that the first `sret` drops into U-mode:

- `frame[2]` (sp) = `0x8000_0000` (top of the 4 KiB user stack at VA `0x7FFFF000`)
- `frame[32]` (sepc) = `entry_pc`
- `frame[35]` (sstatus) = `0x13` (UIE=1, SIE=1, SPIE=1, SPP=0 for U-mode on `sret`)

### 10.3 Scheduler

The scheduler maintains a singly-linked FIFO ready queue (`ready_queue_head`) and a pointer
to the currently-running process (`current_process`).

`schedule(frame, action)`:
1. If `current_process != null`: copy the live trap frame into `current_process.trap_frame`.
   - `action == SYSACT_EXIT_SCHEDULE`: mark state EXITED (not re-enqueued).
   - Otherwise: mark state READY and append to the tail of the ready queue.
2. Dequeue the head of the ready queue as `next`.
3. Copy `next.trap_frame` over the live trap frame; `sret` restores it.

The CLINT timer interrupt (S-mode cause 5) calls `schedule(frame, SYSACT_SCHEDULE)` after
re-arming MTIMECMP, implementing preemptive round-robin at 1,000,000-cycle intervals.

### 10.4 User Process Injection

The test harness places a user binary into physical RAM before the kernel starts:

| Physical Address | Content |
|-----------------|---------|
| `0x87F0_0000` | User binary pages (raw, starting at offset 0) |
| `0x87EF_F000` | Metadata: bytes `[0..8)` = entry VA, `[8..16)` = size in bytes |

During `spawn_user_process` the kernel:
1. Reads entry VA and size from the metadata page.
2. Allocates `ceil(size/4096)` physical pages via `pmm_alloc`.
3. Copies each source page with `memcpy`.
4. Maps each page at user VA `0x4000_0000 + offset` with flags R+W+X+U (VMM_V added internally).
5. Calls `process_create(entry_va)` then `scheduler_add(pcb)`.

The first timer interrupt context-switches into the new process via `sret`.
