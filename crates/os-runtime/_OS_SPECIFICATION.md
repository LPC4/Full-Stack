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
  - Role: Minimal, immutable code executed at reset. Sets initial CPU state, performs device initialization required to load a kernel (if present) and jumps to the kernel entry point. ROM runs in Machine mode and provides only the smallest infrastructure required by the platform.
  - Where to find it in the repo: the VM embeds a small ROM image; the source-level assembly stubs used for tests/examples live in `crates/os-runtime/boot/` (e.g. `startup.s`, `trap.s`). The VM's ROM image is derived from these pieces for integration testing.
  - What is implemented now: simple reset entry that sets registers and transfers control to `_start` (kernel entry). Future ROM work will include multi-stage boot support and optional recovery menus.

- Kernel (freestanding runtime + core kernel code)
  - Role: Initialize hardware (UART, timers, interrupt controllers), set up memory allocators, optionally initialize and enable paging (Sv39), configure trap/interrupt handling, and enter the main kernel loop. The kernel may choose to identity-map the canonical lower half or relocate itself depending on VM/real-hardware needs.
  - Where to find it in the repo: `crates/os-runtime/kernel/` - contains `pmm.hll`, `vmm.hll`, `entry.hll` (minimal kernel entry) and `my_kernel.hll` (example kernel). Re-usable kernel helpers (kmalloc, timer helpers, trap prologue, etc.) live under `crates/os-runtime/stdlib/kernel/utilities.hll`. The freestanding stdlib used to build `_start` and minimal helpers lives under `crates/os-runtime/stdlib/freestanding/`.
  - What is implemented now: `my_kernel.hll` demonstrates canonical lower-half identity mappings, basic PMM and VMM initialization, and UART-based startup logging. The runtime entry `_start` (provided by the compiler's freestanding runtime) sets up the initial stack and clears BSS.
  - Planned kernel work: a pageable kernel configuration, richer allocators, more HAL drivers, and a small syscall surface for user processes.

- Eventual OS (services, processes, drivers)
  - Role: Layer that runs on top of the kernel: device drivers, userspace programs, filesystems, process management and scheduling, IPC and higher-level services.
  - Where this lives in the repo: not yet implemented as a single module - future work will be placed under `crates/os-runtime/os/` or `crates/os/` depending on how the design evolves. Test programs that exercise kernel interfaces live in `programs/`.
  - Planned items: a simple filesystem for tests, user-mode process support (with address-space isolation), syscall interface and a small set of user tools for integration testing.

The remainder of this specification documents the machine model, calling conventions and ABI that the kernel and eventual OS must follow.

---

## 2. Compiler Target Modes

The HLL compiler operates in two distinct modes, selected via the `--target` flag:

### 2.1 Hosted Mode (Default)
```bash
hllc --target=hosted program.hll -o program.elf
```

**Characteristics:**
- Links against full HLL stdlib (`runtime.hll`, `string_utils.hll`, etc.)
- Uses Linux syscalls for I/O (`ecall` with `a7=64` for write, `a7=93` for exit)
- Entry point: `_start` -> calls `main()` -> calls `exit(return_code)`
- Heap allocation: `new(T)` -> `call malloc`, `free(ptr)` -> `call free`
- Console output: `putchar`, `printf` use `sys_write(fd=1, ...)`
- Process termination: `exit(code)` uses `sys_exit(code)`

**Use Case:** User-space applications, testing, educational examples

### 2.2 Freestanding (Bare-Metal) Mode
```bash
hllc --target=riscv64-bare-metal --entry=kernel_main src/*.hll -o kernel.elf
```

**Characteristics:**
- Links against minimal freestanding runtime (intrinsics only)
- **No syscalls:** All `ecall` instructions are explicit in user code
- Entry point: `_start` (provided by runtime) -> calls `kernel_main(a0, a1)`
- Heap allocation: Not provided by runtime; kernel implements its own allocator
- Console output: Kernel provides `platform_putc(c: i32)` primitive
- No process termination: Kernel runs forever or halts explicitly

**Restrictions (compile-time errors if used):**
- `external putchar`, `external printf`, `external puts` - use platform primitives instead
- `external exit` - use `platform_halt()` instead
- Implicit dependency on `malloc`/`free` symbols from libc
- Any `external` declaration not provided by kernel or freestanding runtime

**Use Case:** OS kernels, bootloaders, firmware, embedded systems

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
1. Boot ROM/Firmware
   +- Loads kernel image from boot media (disk, network, etc.)
   +- Copies image to RAM at 0x8000_0000
   +- Sets a0 = hart_id, a1 = dtb_pointer (or 0)
   +- Jumps to _start (kernel entry point)

2. Kernel Entry Stub (_start) [provided by freestanding runtime]
   +- Sets sp = __stack_top
   +- Clears BSS (__bss_start to __bss_end)
   +- (Optional) Initializes global constructors
   +- Calls kernel_main(a0, a1)

3. kernel_main(a0: u64, a1: u64) [provided by kernel author]
   +- Initializes platform primitives (uart_init, etc.)
   +- Prints startup banner
   +- Sets up trap handlers (mtvec, stvec)
   +- Initializes memory allocators
   +- Enables interrupts (if ready)
   +- Enters main loop (never returns)

4. If kernel_main returns [error condition]
   +- Entry stub calls platform_halt()
```

### 5.3 Entry Stub Implementation

The freestanding runtime provides `_start` as an HLL function with inline assembly:

```hll
; Entry point provided by freestanding runtime
; This is automatically linked when --target=riscv64-bare-metal is used

external kernel_main: (hart_id: u64, dtb_ptr: u64) -> ()

_start: () -> () {
    ; Set up stack pointer
    asm {
        la    sp, __stack_top
    }
    
    ; Clear BSS section
    bss_clear()
    
    ; Call kernel main function
    ; Arguments already in a0 (hart_id) and a1 (dtb_ptr) from bootloader
    kernel_main(asm_reg(a0), asm_reg(a1))
    
    ; If kernel_main returns, halt the system (should never happen)
    platform_halt()
}

; Helper: Zero-fill BSS section
bss_clear: () -> () {
    start: u64 = asm_reg(a0)  ; Will be set by linker
    end: u64 = asm_reg(a1)    ; Will be set by linker
    
    ; Actual implementation uses linker symbols
    ; This is pseudocode to show the logic
    addr: u64 = __bss_start
    while addr < __bss_end {
        *((u8*)addr) = 0
        addr = addr + 1
    }
}
```

**Note:** The actual BSS clearing uses linker-provided symbols and may be implemented in assembly for efficiency.

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

The compiler provides two mutually exclusive runtime modes:

### 7.1 Hosted Runtime (User-Space Applications)

**Source:** `programs/stdlib/runtime.hll`, `programs/stdlib/string_utils.hll`, etc.

**Features:**
- Full HLL standard library
- Linux syscall-based I/O (`ecall` with `a7=64` for write, `a7=93` for exit)
- Dynamic memory allocation via `malloc`/`free` (linked from C runtime or provided by OS)
- Console output: `putchar`, `puts`, `printf`, `print_int`
- Process control: `exit(code)`

**Entry Flow:**
```
_start (from runtime.hll)
  v
main() (user-defined)
  v
exit(return_code) (syscall)
```

**Use Case:** Educational examples, algorithm testing, user-space tools

### 7.2 Freestanding Runtime (Kernel/Firmware)

**Source:** Minimal runtime built into compiler (not from `runtime.hll`)

**Features:**
- **Compiler intrinsics only:** `memcpy`, `memset`, `memmove`, `memcmp`, 64-bit arithmetic helpers
- **Panic support:** `panic(message: u8*)` calls `platform_putc` and then `platform_halt`
- **Entry stub:** `_start` sets up stack, clears BSS, calls `kernel_main`
- **No I/O functions:** `putchar`, `printf`, etc. are **not** provided
- **No dynamic allocation:** `new(T)` and `free(ptr)` are **not** linked (kernel provides its own allocator)

**Restrictions:**
- Using `external putchar`, `external printf`, etc. is a **compile-time error**
- Using `external exit` is a **compile-time error**
- `main()` is optional; the kernel defines `kernel_main` instead

**Entry Flow:**
```
_start (from freestanding runtime)
  v
Set sp = __stack_top
Clear BSS
  v
kernel_main(a0, a1) (kernel-defined)
  v
[Never returns; if it does, call platform_halt()]
```

**Provided Symbols:**

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `_start` | `() -> ()` | Entry stub (sets up stack, clears BSS, calls `kernel_main`) |
| `panic` | `(message: u8*) -> !` | Print message and halt (calls `platform_putc` + `platform_halt`) |
| `memcpy` | `(dest: u8*, src: u8*, n: u64) -> u8*` | Copy memory regions |
| `memset` | `(dest: u8*, value: i32, n: u64) -> u8*` | Fill memory with byte value |
| `memmove` | `(dest: u8*, src: u8*, n: u64) -> u8*` | Copy overlapping memory regions |
| `memcmp` | `(s1: u8*, s2: u8*, n: u64) -> i32` | Compare memory regions |

**Not Provided (kernel must implement):**
- `platform_putc(c: i32) -> ()`
- `platform_getc() -> i32` (optional)
- `platform_halt() -> !`
- `platform_timer_freq() -> u64` (optional)
- `platform_get_time() -> u64` (optional)
- Any heap allocator (`new`, `free`)

### 7.3 Compiler Enforcement

When `--target=riscv64-bare-metal` is used, the compiler validates:

1. **No hosted-only externals:** Rejects `external putchar`, `external printf`, `external exit`, etc.
2. **No implicit libc dependencies:** Ensures `malloc`/`free` are not referenced unless kernel provides them
3. **Entry point validation:** Verifies that `kernel_main` (or custom entry) is defined
4. **Inline assembly safety:** Allows `asm { }` blocks but warns about clobbering critical registers

**Error Example:**
```
Error: 'putchar' is not available in freestanding mode
  --> kernel.hll:42:5
   |
42 |     putchar(65)
   |     ^^^^^^^
   |
   = Help: Use 'platform_putc(65)' instead, or define your own putchar wrapper
```

---

## 8. Hardware Abstraction Layer (HAL)

The kernel **must** provide a small set of platform primitives that replace hosted I/O functions. These are the only hardware-specific functions the compiler runtime depends on.

### 8.1 Required Primitives

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `platform_putc` | `(c: i32) -> ()` | Output a single character to the console (UART). Blocking. |
| `platform_halt` | `() -> !` | Halt execution (infinite loop or WFI). Never returns. |

### 8.2 Optional Primitives

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `platform_getc` | `() -> i32` | Read a character from UART. Returns -1 if no input available. |
| `platform_timer_freq` | `() -> u64` | Returns the timer tick frequency (e.g., 10,000,000 for 10 MHz). |
| `platform_get_time` | `() -> u64` | Returns current `mtime` value from CLINT. |

### 8.3 Implementation Example

```hll
; ============================================================================
; Platform Primitives for QEMU virt / Project VM
; ============================================================================

