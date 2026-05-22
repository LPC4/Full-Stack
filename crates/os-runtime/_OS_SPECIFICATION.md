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
- Entry point: `_start` → calls `main()` → calls `exit(return_code)`
- Heap allocation: `new(T)` → `call malloc`, `free(ptr)` → `call free`
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
- Entry point: `_start` (provided by runtime) → calls `kernel_main(a0, a1)`
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
- `0x0000-0x007C`: Priority registers (32 sources × 4 bytes)
- `0x1000-0x107C`: Pending bits (bitfield, 1 bit per source)
- `0x2000-0x207C`: Enable bits (per-context, 1 bit per source)
- `0x200000`: Threshold register (per-context)
- `0x200004`: Claim/Complete register (per-context)

**Operation:**
1. External device calls `plic_set_irq(source_id)` to assert interrupt
2. CPU checks `plic_claim(hart_id)` before each instruction fetch
3. Handler reads claim register → returns highest-priority pending IRQ
4. Handler writes IRQ ID to complete register → clears pending bit

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
   ├─ Loads kernel image from boot media (disk, network, etc.)
   ├─ Copies image to RAM at 0x8000_0000
   ├─ Sets a0 = hart_id, a1 = dtb_pointer (or 0)
   └─ Jumps to _start (kernel entry point)

2. Kernel Entry Stub (_start) [provided by freestanding runtime]
   ├─ Sets sp = __stack_top
   ├─ Clears BSS (__bss_start to __bss_end)
   ├─ (Optional) Initializes global constructors
   └─ Calls kernel_main(a0, a1)

3. kernel_main(a0: u64, a1: u64) [provided by kernel author]
   ├─ Initializes platform primitives (uart_init, etc.)
   ├─ Prints startup banner
   ├─ Sets up trap handlers (mtvec, stvec)
   ├─ Initializes memory allocators
   ├─ Enables interrupts (if ready)
   └─ Enters main loop (never returns)

4. If kernel_main returns [error condition]
   └─ Entry stub calls platform_halt()
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
┌──────────────────────┐
│   Caller's frame     │
├──────────────────────┤
│   Return address (ra)│ ← sp + N - 8
│   Saved registers    │ ← sp + N - 16, sp + N - 24, ...
│   Local variables    │ ← sp + 0, sp + 8, ...
├──────────────────────┤
│   Current frame      │ ← sp (16-byte aligned)
└──────────────────────┘
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
  ↓
main() (user-defined)
  ↓
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
  ↓
Set sp = __stack_top
Clear BSS
  ↓
kernel_main(a0, a1) (kernel-defined)
  ↓
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

; UART base address
const UART_BASE = 0x10000000
const UART_LSR = 0x05  ; Line Status Register offset
const UART_THR = 0x00  ; Transmitter Holding Register offset

; Output a single character to UART (blocking)
platform_putc: (c: i32) -> () {
    ; Wait until TX holding register is empty (LSR bit 5 = 1)
    while (*((u8*)(UART_BASE + UART_LSR)) & 0x20) == 0 {
        ; spin
    }
    ; Write character to THR
    *((u8*)(UART_BASE + UART_THR)) = u8(c)
}

; Halt the system (infinite loop)
platform_halt: () -> ! {
    loop {
        ; Optionally use WFI (wait for interrupt) to reduce power
        asm {
            wfi
        }
    }
}

; Read a character from UART (non-blocking)
platform_getc: () -> i32 {
    if (*((u8*)(UART_BASE + UART_LSR)) & 0x01) != 0 {
        return i32(*((u8*)(UART_BASE + UART_THR)))
    }
    return -1
}

; Get timer frequency (hardcoded for QEMU virt)
platform_timer_freq: () -> u64 {
    return 10000000  ; 10 MHz
}

; Get current time from CLINT MTIME
platform_get_time: () -> u64 {
    return *((u64*)0x0200BFF8)
}
```

### 8.4 Panic Implementation

The freestanding runtime provides `panic` which uses the HAL primitives:

```hll
; Provided by freestanding runtime (compiler-built)
panic: (message: u8*) -> ! {
    ; Print "PANIC: " prefix
    panic_prefix: u8* = "PANIC: "
    i: u64 = 0
    while @panic_prefix[i] != 0 {
        platform_putc(i32(@panic_prefix[i]))
        i = i + 1
    }
    
    ; Print message
    i = 0
    while @message[i] != 0 {
        platform_putc(i32(@message[i]))
        i = i + 1
    }
    
    ; Newline
    platform_putc(10)
    
    ; Halt
    platform_halt()
}
```

**Usage in kernel code:**
```hll
kernel_main: (hart_id: u64, dtb_ptr: u64) -> () {
    if hart_id != 0 {
        panic("Only hart 0 is supported")
    }
    ; ... rest of kernel initialization
}
```

---

## 9. Trap Handling

### 9.1 Trap Types

RISC-V distinguishes between **exceptions** (synchronous) and **interrupts** (asynchronous):

**Exceptions (synchronous):**
- Instruction address misaligned
- Instruction access fault
- Illegal instruction
- Breakpoint (`ebreak`)
- Load/store address misaligned
- Load/store access fault
- Environment call (`ecall`) from U/S/M mode
- Instruction/load/store page fault (when Sv39 is enabled)

**Interrupts (asynchronous):**
- Machine software interrupt (MSIP in CLINT)
- Machine timer interrupt (MTIMECMP in CLINT)
- Machine external interrupt (PLIC)
- Supervisor/user variants (if delegation is enabled)

### 9.2 Trap Entry Sequence

When a trap occurs in Machine mode:

1. **Hardware saves state:**
    - `mepc` ← PC of trapped instruction (or next instruction for async interrupts)
    - `mcause` ← Exception/interrupt code (bit 63 = 1 for interrupts)
    - `mtval` ← Fault address (for address faults) or faulting instruction (for illegal instruction)
    - `mstatus.MPIE` ← `mstatus.MIE` (save interrupt enable)
    - `mstatus.MIE` ← 0 (disable further interrupts)
    - `mstatus.MPP` ← Current privilege mode (always 3 = M-mode in this spec)

2. **Hardware jumps to handler:**
    - If `mtvec.MODE` = 0 (direct): `pc` ← `mtvec.BASE`
    - If `mtvec.MODE` = 1 (vectored) and interrupt: `pc` ← `mtvec.BASE + (mcause × 4)`
    - If `mtvec.MODE` = 1 (vectored) and exception: `pc` ← `mtvec.BASE`

3. **Software handler executes:**
    - Save caller-saved registers to trap frame
    - Call HLL trap handler: `trap_handler(cause, epc, tval, &frame)`
    - Restore registers from returned frame
    - Execute `mret` to return from trap

### 9.3 Trap Frame Layout

The kernel defines a `TrapFrame` struct to save register state:

```hll
type TrapFrame = {
    regs: u64[32],      ; x0..x31 (general-purpose registers)
    fregs: u64[32],     ; f0..f31 (floating-point registers, optional)
    pc: u64,            ; Program counter (mepc)
    mstatus: u64,       ; Machine status (mstatus)
    mcause: u64,        ; Trap cause (mcause)
    mtval: u64,         ; Trap value (mtval)
}
```

**Memory Layout:**
```
Offset  Field
0       regs[0]  (x0, hardwired to 0)
8       regs[1]  (ra)
16      regs[2]  (sp)
...     ...
248     regs[31] (t6)
256     fregs[0] (ft0)
...     ...
512     pc
520     mstatus
528     mcause
536     mtval
Total: 544 bytes (without FP) or 800 bytes (with FP)
```

### 9.4 Trap Handler Implementation

**Assembly Stub (entry point):**
```assembly
.global trap_entry
trap_entry:
    ; Allocate space for trap frame on stack
    addi   sp, sp, -544
    
    ; Save general-purpose registers
    sd     x0, 0(sp)      ; x0 is always 0, but save for consistency
    sd     x1, 8(sp)      ; ra
    sd     x2, 16(sp)     ; sp (will be restored later)
    sd     x3, 24(sp)     ; gp
    ; ... save x4-x31 ...
    
    ; Save CSRs
    csrr   t0, mepc
    sd     t0, 512(sp)    ; pc
    csrr   t0, mstatus
    sd     t0, 520(sp)    ; mstatus
    csrr   t0, mcause
    sd     t0, 528(sp)    ; mcause
    csrr   t0, mtval
    sd     t0, 536(sp)    ; mtval
    
    ; Prepare arguments for HLL handler
    ; a0 = mcause, a1 = mepc, a2 = mtval, a3 = &frame
    mv     a0, t0         ; a0 = mcause (already in t0)
    ld     a1, 512(sp)    ; a1 = mepc
    ld     a2, 536(sp)    ; a2 = mtval
    addi   a3, sp, 0      ; a3 = &frame
    
    ; Call HLL trap handler
    jal    ra, trap_handler
    
    ; Restore CSRs
    ld     t0, 520(sp)    ; t0 = mstatus
    csrw   mstatus, t0
    ld     t0, 512(sp)    ; t0 = mepc
    csrw   mepc, t0
    
    ; Restore general-purpose registers
    ld     x1, 8(sp)      ; ra
    ; ... restore x3-x31 ...
    ld     x2, 16(sp)     ; sp (last, as we're deallocating stack)
    
    ; Return from trap
    mret
```

**HLL Handler:**
```hll
trap_handler: (cause: u64, epc: u64, tval: u64, frame: &TrapFrame) -> TrapFrame {
    is_interrupt: bool = (cause >> 63) != 0
    exception_code: u64 = cause & 0x7FFFFFFFFFFFFFFF
    
    if is_interrupt {
        handle_interrupt(exception_code, frame)
    } else {
        handle_exception(exception_code, epc, tval, frame)
    }
    
    return *frame
}

handle_exception: (code: u64, epc: u64, tval: u64, frame: &TrapFrame) -> () {
    case code {
        2 => {
            ; Illegal instruction
            panic_with_regs("Illegal instruction", epc, frame)
        }
        11 => {
            ; Environment call from M-mode
            handle_ecall(frame)
            frame.pc = frame.pc + 4  ; Skip ecall instruction
        }
        _ => {
            panic_with_regs("Unhandled exception", epc, frame)
        }
    }
}

handle_interrupt: (code: u64, frame: &TrapFrame) -> () {
    case code {
        7 => {
            ; Machine timer interrupt
            handle_timer_interrupt()
        }
        11 => {
            ; Machine external interrupt (PLIC)
            handle_external_interrupt()
        }
        _ => {
            panic_with_regs("Unhandled interrupt", frame.pc, frame)
        }
    }
}
```

### 9.5 Setting Up Trap Vector

The kernel must configure `mtvec` before enabling traps:

```hll
setup_traps: () -> () {
    ; Set trap vector to trap_entry (direct mode)
    asm {
        la     t0, trap_entry
        csrw   mtvec, t0
    }
    
    ; Enable machine timer interrupt in mie
    current_mie: u64 = asm_csr(mie)
    asm {
        csrs   mie, 0x80  ; Set MTIE bit (bit 7)
    }
    
    ; Enable global interrupts in mstatus
    asm {
        csrs   mstatus, 0x8  ; Set MIE bit (bit 3)
    }
}
```

---

## 10. Memory Management

### 10.1 Initial Memory Layout

At kernel entry (after BSS is cleared):

```
Address                  Content
0x00000000               Boot ROM (read-only, not part of kernel)
...
0x80000000               Kernel .text section (entry point _start)
0x80000000 + text_size   Kernel .rodata section
0x80000000 + rodata_size Kernel .data section
0x80000000 + data_size   Kernel .bss section (zero-filled)
__bss_end                End of kernel image
__bss_end                Start of available memory
...                      Free RAM for kernel allocators
__stack_top - 64KB       Bottom of initial stack (grows downward)
__stack_top              Top of initial stack (sp starts here)
0xFFFFFFFF               End of RAM
```

**Available Memory Regions:**
- **Heap:** From `__bss_end` to `__stack_top - 64KB` (kernel can adjust boundaries)
- **Stack:** 64 KB region ending at `__stack_top` (initial stack only; kernel can allocate more)

### 10.2 Physical Page Allocator

The kernel should implement a simple physical page allocator before enabling virtual memory:

**Bump Allocator (simplest):**
```hll
const PAGE_SIZE = 4096

type PhysicalAllocator = {
    next_page: u64,      ; Next available physical page
    end_page: u64,       ; End of available memory
}

global_phys_alloc: PhysicalAllocator

init_physical_allocator: () -> () {
    global_phys_alloc.next_page = align_up(__bss_end, PAGE_SIZE)
    global_phys_alloc.end_page = __stack_top - 0x10000  ; Reserve stack space
}

alloc_page: () -> u64 {
    if global_phys_alloc.next_page >= global_phys_alloc.end_page {
        panic("Out of physical memory")
    }
    page_addr: u64 = global_phys_alloc.next_page
    global_phys_alloc.next_page = global_phys_alloc.next_page + PAGE_SIZE
    return page_addr
}

free_page: (page_addr: u64) -> () {
    ; Bump allocator doesn't support free (use more sophisticated allocator for production)
    panic("free_page not implemented for bump allocator")
}

align_up: (addr: u64, alignment: u64) -> u64 {
    return (addr + alignment - 1) & !(alignment - 1)
}
```

### 10.3 Virtual Memory (Sv39)

Once the physical allocator is ready, the kernel can enable Sv39 paging:

**Page Table Structure:**
- 3-level page table (L2 → L1 → L0)
- Each page table occupies one 4 KB page
- Each PTE (Page Table Entry) is 8 bytes
- 512 PTEs per page table (4096 / 8)

**Enabling Paging:**
```hll
enable_paging: () -> () {
    ; Allocate root page table (L2)
    root_pt: u64 = alloc_page()
    
    ; Map kernel image identity (virtual = physical)
    map_region_identity(root_pt, 0x80000000, 0x80000000, kernel_image_size, READ | WRITE | EXECUTE)
    
    ; Map UART, CLINT, PLIC identity (MMIO devices)
    map_region_identity(root_pt, 0x10000000, 0x10000000, 0x1000, READ | WRITE)
    map_region_identity(root_pt, 0x02000000, 0x02000000, 0x10000, READ | WRITE)
    map_region_identity(root_pt, 0x0C000000, 0x0C000000, 0x1000000, READ | WRITE)
    
    ; Set SATP to enable Sv39
    satp_value: u64 = (8 << 60) | (root_pt >> 12)  ; MODE=8 (Sv39), PPN=root_pt/4096
    asm {
        csrw   satp, a0
        sfence.vma  ; Flush TLB
    }
}
```

**Note:** After enabling paging, all addresses are virtual. The kernel must ensure its own mappings are correct before switching.

---

## 11. Build System Integration

### 11.1 Compiler Command-Line Interface

**Basic kernel build:**
```bash
hllc --target=riscv64-bare-metal \
     --entry=kernel_main \
     -o kernel.elf \
     src/kernel.hll src/platform.hll src/memory.hll
```

**With custom linker script:**
```bash
hllc --target=riscv64-bare-metal \
     --entry=kernel_main \
     --linker-script=kernel.ld \
     -o kernel.elf \
     src/*.hll
```

**Generate flat binary:**
```bash
hllc --target=riscv64-bare-metal \
     --entry=kernel_main \
     --binary \
     -o kernel.bin \
     src/*.hll
```

**Debug symbols:**
```bash
hllc --target=riscv64-bare-metal \
     --entry=kernel_main \
     --debug \
     -o kernel.elf \
     src/*.hll
```

### 11.2 Build Options

| Option | Description | Default |
|--------|-------------|---------|
| `--target` | Target mode: `hosted` or `riscv64-bare-metal` | `hosted` |
| `--entry` | Kernel entry function name | `kernel_main` |
| `--linker-script` | Path to custom linker script | Built-in default |
| `--binary` | Generate flat binary in addition to ELF | Disabled |
| `--debug` | Include debug symbols in ELF | Disabled |
| `-o` | Output file path | `a.out` or `a.bin` |

### 11.3 IDE Integration

The project's GUI should provide:
- **Target selector dropdown:** Hosted vs. Bare-Metal
- **Entry point field:** Configurable kernel entry function
- **Build button:** Compiles with appropriate flags
- **Output panel:** Shows compilation diagnostics
- **Memory view:** Displays kernel image layout (sections, symbols)
- **Serial console:** Captures UART output from VM execution

---

## 12. Testing and Debugging

### 12.1 Minimal Kernel Test

A kernel that fulfills this contract must:

1. Provide `platform_putc` and `platform_halt`
2. Define `kernel_main(a0: u64, a1: u64)`
3. Print a startup message to UART
4. Enter an infinite loop (or halt)

**Example:**
```hll
; ============================================================================
; Minimal Kernel Test
; ============================================================================

external platform_putc: (c: i32) -> ()
external platform_halt: () -> !

; Print a string to UART
print_string: (str: u8*) -> () {
    i: u64 = 0
    while @str[i] != 0 {
        platform_putc(i32(@str[i]))
        i = i + 1
    }
}

; Kernel entry point
kernel_main: (hart_id: u64, dtb_ptr: u64) -> () {
    print_string("Hello from kernel!\n")
    print_string("Hart ID: ")
    print_digit(hart_id)
    print_string("\n")
    
    ; Infinite loop
    loop {
        asm {
            wfi
        }
    }
}

; Helper: Print a single digit (0-9)
print_digit: (d: u64) -> () {
    if d < 10 {
        platform_putc(i32(d) + 48)
    }
}
```

**Expected Output:**
```
Hello from kernel!
Hart ID: 0
```

### 12.2 QEMU Testing

**Run kernel in QEMU:**
```bash
qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -kernel kernel.elf \
    -m 2G
```

**Exit QEMU:** Press `Ctrl+A` then `X`

### 12.3 VM Testing

The project's built-in VM can load and execute the kernel:

1. Compile kernel with `--target=riscv64-bare-metal`
2. Load ELF into VM (parser reads sections, places at correct addresses)
3. Set PC to `_start` symbol address
4. Execute until `platform_halt()` is called (infinite loop detected)
5. Capture UART output for display in GUI

### 12.4 Debugging Techniques

**Serial Debugging:**
- Use `platform_putc` to print debug messages
- Print register values, memory contents, trap causes
- Add timestamps using `platform_get_time()`

**Trap Inspection:**
- On panic, print `mcause`, `mepc`, `mtval`
- Dump trap frame registers
- Decode exception type from `mcause`

**Memory Inspection:**
- Add commands to dump memory regions
- Print page table entries (if paging is enabled)
- Show allocator state (free lists, used pages)

---

## Appendix A: Quick Reference

### A.1 Memory Map Summary
```
0x00000000 - 0x0FFFFFFF : Boot ROM (256 MB)
0x10000000 - 0x10000007 : UART (8 bytes)
0x02000000 - 0x0200FFFF : CLINT (64 KB)
0x0C000000 - 0x0CFFFFFF : PLIC (16 MB)
0x80000000 - 0xFFFFFFFF : RAM (2 GB)
```

### A.2 Exception Codes
```
0  : Instruction address misaligned
1  : Instruction access fault
2  : Illegal instruction
3  : Breakpoint
4  : Load address misaligned
5  : Load access fault
6  : Store/AMO address misaligned
7  : Store/AMO access fault
8  : Environment call from U-mode
11 : Environment call from M-mode
12 : Instruction page fault (Sv39)
13 : Load page fault (Sv39)
15 : Store/AMO page fault (Sv39)
```

### A.3 Interrupt Codes (bit 63 set)
```
3  : Machine software interrupt
7  : Machine timer interrupt
11 : Machine external interrupt
```

### A.4 CSR Addresses
```
mstatus    = 0x300
mie        = 0x304
mtvec      = 0x305
mscratch   = 0x340
mepc       = 0x341
mcause     = 0x342
mtval      = 0x343
mip        = 0x344
satp       = 0x180
mcycle     = 0xB00
minstret   = 0xB02
```

### A.5 Build Commands
```bash
# ELF format
hllc --target=riscv64-bare-metal --entry=kernel_main -o kernel.elf src/*.hll

# Flat binary
hllc --target=riscv64-bare-metal --entry=kernel_main --binary -o kernel.bin src/*.hll

# QEMU execution
qemu-system-riscv64 -machine virt -nographic -kernel kernel.elf -m 2G
```

---

## Appendix B: Migration Guide

### B.1 Converting Hosted Code to Freestanding

**Step 1: Replace I/O functions**
```hll
; Before (hosted)
putchar(65)
printf("Value: %d\n", x)

; After (freestanding)
platform_putc(65)
print_string("Value: ")
print_int(x)
platform_putc(10)
```

**Step 2: Remove exit calls**
```hll
; Before (hosted)
if error {
    exit(1)
}

; After (freestanding)
if error {
    panic("Error occurred")
}
```

**Step 3: Provide custom allocator**
```hll
; Before (hosted)
ptr: i32* = new(i32)
free(ptr)

; After (freestanding)
ptr: i32* = kernel_alloc(sizeof(i32))
kernel_free(ptr)
```

**Step 4: Change entry point**
```hll
; Before (hosted)
main: () -> i32 {
    return 0
}

; After (freestanding)
kernel_main: (hart_id: u64, dtb_ptr: u64) -> () {
    ; Never returns
}
```

### B.2 Common Pitfalls

1. **Forgetting to clear BSS:** Uninitialized globals will contain garbage
2. **Using hosted I/O by accident:** Compiler will catch this, but be aware
3. **Stack overflow:** Initial stack is only 64 KB; allocate more if needed
4. **Not setting up trap handlers:** Traps will jump to undefined addresses
5. **Enabling interrupts too early:** Set up handlers before enabling interrupts

---

**End of Specification**
