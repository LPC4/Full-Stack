# RISC-V RV64IMAFD Virtual Machine Specification

This document specifies the `virtual-machine` crate: a cycle-stepped RV64IMAFD emulator with
Machine, Supervisor, and User privilege modes and Sv39 virtual memory. It covers the CPU
pipeline, the physical and virtual memory model, CSR behavior, trap handling, the MMU, and
the emulated devices. It is the reference for both implementing against and understanding
the VM's observable behavior.

## 1. Architecture overview

### 1.1 VM Components
- **CPU Core:** In-order, single-issue 5-stage pipeline (Fetch -> Decode -> Execute -> Memory -> Writeback) with data forwarding, load-use hazard detection, and branch prediction
- **Branch Predictor:** 2-bit bimodal predictor backed by a Branch Target Buffer (BTB)
- **Cache Hierarchy:** Three levels (L1/L2/L3) of set-associative write-back caches in front of RAM; MMIO bypasses the caches
- **Memory System:** Byte-addressable, little-endian, flat physical address space with optional Sv39 virtual memory translation
- **MMU:** Sv39 page-based virtual memory (3-level page table, 39-bit virtual addresses)
- **CSR File:** Machine-mode and Supervisor-mode Control & Status Registers (Zicsr extension), plus single-entry PMP storage
- **Devices:** UART (serial I/O), CLINT (timer/software interrupts), PLIC (external interrupts), SYSCON (halt/exit), Framebuffer (linear RGBA8888 display)
- **Bus:** Memory-mapped I/O with address decoding

### 1.2 Execution Model
- **Privilege Modes:** Machine (M), Supervisor (S), and User (U) modes, with real privilege switching. The ROM boots in M-mode, delegates traps, and `mret`s into the S-mode kernel; the kernel `sret`s into U-mode processes.
- **Virtual Memory:** Sv39 paging (optional, controlled by SATP CSR; bypassed in M-mode)
- **Interrupts:** Synchronous traps (exceptions) and asynchronous interrupts (timer, software, external), delegatable to S-mode via `medeleg`/`mideleg`
- **Memory Ordering:** Sequential consistency (no weak ordering in this VM)
- **Atomic Operations:** Load-reserved/store-conditional (LR/SC) via reservation set, plus AMO read-modify-write


## 2. Memory Map

| Address Range | Device | Size | Description |
|---------------|--------|------|-------------|
| `0x0000_0000` - `0x0FFF_FFFF` | ROM | 256 MB | Boot ROM / firmware (read-only) |
| `0x0200_0000` - `0x0200_FFFF` | CLINT | 64 KB | Core Local Interruptor (mtime, mtimecmp, msip) |
| `0x0C00_0000` - `0x0CFF_FFFF` | PLIC | 16 MB | Platform-Level Interrupt Controller |
| `0x1000_0000` - `0x1000_0FFF` | UART | 4 KB | Serial console (NS16550A subset; 8 registers at the low offsets) |
| `0x1001_0000` - `0x1001_0FFF` | SYSCON | 4 KB | Halt/exit device (write an exit code to stop the VM) |
| `0x1002_0000` - `0x1006_AFFF` | Framebuffer | 300 KB | Linear RGBA8888 display, 320 x 240 (see 6.5) |
| `0x8000_0000` - ... | RAM | 128 MB | Main memory (DRAM); default size is 128 MB |

**Note:** Addresses are physical when virtual memory is disabled (SATP.mode = 0/Bare) or when the
hart is in M-mode. When Sv39 is enabled (SATP.mode = 8) in S/U-mode, all instruction fetches,
loads, and stores use virtual address translation through the MMU. RAM accesses pass through the
L1/L2/L3 cache hierarchy; device (MMIO) accesses bypass the caches.


## 2.1 Virtual Memory (Sv39)

### 2.1.1 Overview
The VM implements the RISC-V Sv39 virtual memory scheme:
- **Virtual Address Space:** 39-bit addresses (512 GB)
- **Physical Address Space:** Up to 56-bit addresses (implementation supports full 64-bit)
- **Page Size:** 4 KB (2^12 bytes)
- **Page Table Levels:** 3-level hierarchical page table
- **Translation:** Automatic in Fetch and Memory pipeline stages

### 2.1.2 SATP CSR (Supervisor Address Translation and Protection)
**Address:** `0x180`  
**Width:** 64 bits  
**Fields:**
- `[63:60]` **MODE**: Addressing mode
  - `0` = Bare (no translation, identity mapping)
  - `8` = Sv39 (39-bit virtual addressing)
  - Other values = Reserved (writes forced to Bare mode)
- `[59:44]` **ASID**: Address Space Identifier (not implemented, ignored)
- `[43:0]` **PPN**: Physical Page Number of root page table (shifted left by 12 bits)

**Example:**
```rust
// Enable Sv39 with root table at physical address 0x1000
let satp = (8u64 << 60) | (0x1000 >> 12); // MODE=8, PPN=1
csrs.write(addr::SATP, satp);
```

### 2.1.3 Page Table Structure
Each page table entry (PTE) is 8 bytes with the following format:

| Bits | Field | Description |
|------|-------|-------------|
| `[0]` | V | Valid bit (must be 1 for valid PTE) |
| `[1]` | R | Read permission |
| `[2]` | W | Write permission |
| `[3]` | X | Execute permission |
| `[4]` | U | User accessibility (0=supervisor only, 1=user accessible) |
| `[5]` | G | Global (not implemented) |
| `[6]` | A | Accessed (set by hardware on access) |
| `[7]` | D | Dirty (set by hardware on write) |
| `[8]` | RSW | Reserved for software |
| `[53:10]` | PPN | Physical Page Number |
| `[63:54]` | Reserved | Must be zero |

**Leaf vs Non-Leaf PTEs:**
- **Leaf PTE:** R=1 or X=1 (maps to physical page)
- **Non-Leaf PTE:** R=0, W=0, X=0 (points to next-level page table)

### 2.1.4 Address Translation Process
Sv39 divides the virtual address as follows:
- **VPN[2]:** Bits [38:30] - Level 2 index (9 bits)
- **VPN[1]:** Bits [29:21] - Level 1 index (9 bits)
- **VPN[0]:** Bits [20:12] - Level 0 index (9 bits)
- **Offset:** Bits [11:0] - Page offset (12 bits)

**Translation Algorithm:**
1. Start with root page table at `SATP.PPN << 12`
2. For each level (2 -> 1 -> 0):
   - Calculate PTE address: `(current_ppn << 12) | (vpn[level] x 8)`
   - Read PTE from physical memory
   - If V=0 -> Page Fault
   - If R=1 or X=1 -> Leaf PTE found, proceed to step 3
   - If W=1 -> Malformed PTE (W=1 but R=X=0), Page Fault
   - Otherwise, extract next-level PPN and continue
3. At leaf PTE:
   - Check permissions based on access type (read/write/execute)
   - Check privilege mode against U bit
   - Calculate physical address: `(leaf_ppn << 12) | offset`

### 2.1.5 Permission Checking
The MMU enforces the following permission checks:

**Execute Access (`is_execute=true`):**
- X bit must be 1
- Returns `InstructionAccessFault` if X=0

**Write Access (`is_write=true`):**
- W bit must be 1
- Returns `StoreAccessFault` if W=0

**Read Access (`is_write=false`, `is_execute=false`):**
- R bit must be 1
- Returns `LoadAccessFault` if R=0

**Privilege Mode Checks:**
- In M-mode: Bypass all translation (identity mapping)
- In S/U-mode with U=0: Supervisor-only page
- In U-mode with U=1: User-accessible page
- In U-mode with U=0: Returns `PageFault`

### 2.1.6 Canonical Address Validation
Sv39 requires that virtual addresses be "canonical": bits [63:39] must all equal bit 38.
- Valid: `0x0000_0000_0000_0000` to `0x0000_007F_FFFF_FFFF` (lower half)
- Valid: `0xFFFF_FF80_0000_0000` to `0xFFFF_FFFF_FFFF_FFFF` (upper half)
- Invalid: Any address where bits [63:39] are neither all-0 nor all-1 (i.e. not 0 and not 0x1FFFFF)

Non-canonical addresses trigger a Page Fault before translation begins.

### 2.1.7 Special Instructions
**SFENCE.VMA:** Synchronize page table updates
- **Opcode:** SYSTEM with funct3=0, funct12=0x105
- **Behavior:** No-op in this implementation (no TLB caching)
- **Purpose:** Ensures subsequent memory accesses use updated page tables


## 3. CPU Pipeline Stages

### 3.1 Fetch Stage
**Responsibility:** Read 32-bit instruction from memory at current PC.

**Inputs:**
- `pc`: Current program counter (must be 4-byte aligned)
- `bus`: System bus for memory access
- `satp`: SATP CSR value (for virtual address translation)
- `priv_mode`: Current privilege mode

**Outputs:**
- `instruction`: 32-bit encoded instruction word
- **Error:** `InstructionAccessFault(pc)` if misaligned, unmapped, or execute permission denied

**Validation:**
```rust
if pc & 0x3 != 0 {
    return Err(InstructionAccessFault(pc));
}

// Translate virtual address to physical address using MMU
let phys_addr = mmu::translate(pc, satp, priv_mode, bus, false, true)?;

let instruction = bus.read_word(phys_addr)
    .map_err(|_| InstructionAccessFault(pc))?;
```

**MMU Integration:**
- If SATP.mode = 0 (Bare) or priv_mode = Machine: Use identity mapping (physical = virtual)
- If SATP.mode = 8 (Sv39): Perform 3-level page table walk
- Execute permission checked: X bit must be set in leaf PTE
- Non-canonical addresses trigger Page Fault

**Performance Counter:** Increment `cycle` CSR each fetch.


### 3.2 Decode Stage
**Responsibility:** Parse instruction into operation fields and operands.

**Inputs:**
- `instruction`: 32-bit encoded instruction

**Outputs:**
- `DecodedInsn`: Enum variant with parsed fields (rd, rs1, rs2, imm, funct3, etc.)
- **Error:** `IllegalInstruction(opcode)` if opcode is invalid

**Decode Categories:**
- **R-type:** ALU operations (add, sub, mul, div, etc.)
- **I-type:** Immediate ALU, loads, jalr, fence, CSR ops
- **S-type:** Stores
- **B-type:** Branches
- **U-type:** lui, auipc
- **J-type:** jal
- **R4-type:** FP fused multiply-add/subtract

**Special Cases:**
- `opcode=0x00` -> Illegal instruction
- Unrecognized `funct3`/`funct7` combinations -> Illegal instruction


### 3.3 Execute Stage
**Responsibility:** Perform arithmetic/logic operations, calculate addresses, evaluate branches.

**Inputs:**
- `decoded`: Decoded instruction
- `regs`: Register file (integer + FP)
- `csrs`: CSR file

**Outputs:**
- `ExecResult`: Enum describing operation outcome
  - `WriteInt { rd, val, next_pc }` - Integer result to write back
  - `WriteFp { rd, bits, next_pc }` - FP result to write back
  - `Load { rd, addr, funct3, next_pc }` - Memory load request
  - `Store { addr, val, funct3, next_pc }` - Memory store request
  - `Jump { next_pc }` - Control transfer
  - `Branch { taken, target_pc, next_pc }` - Conditional branch
  - `Csr { funct3, rd, csr, operand, next_pc }` - CSR operation
  - `Ecall` / `Ebreak` - Environment calls
  - `Fence` / `FenceI` - Memory fences

**ALU Operations:**
- Integer: add, sub, sll, srl, sra, and, or, xor, slt, sltu
- Multiply/Divide: mul, mulh, div, rem, etc. (M extension)
- Shift amounts masked to 6 bits (5 bits for W variants)
- Division by zero returns special values (see M extension spec)

**Branch/Jump Calculations:**
```rust
// Branch target
target = pc + sext(imm << 1)

// Jump target
target = pc + sext(imm << 1)

// jalr target (clear bit 0)
target = (rs1 + sext(imm)) & !1
```

**Performance Counter:** Increment `instret` CSR after successful execution.


### 3.4 Memory Stage
**Responsibility:** Execute load/store operations against system bus with virtual address translation.

**Inputs:**
- `exec_result`: From execute stage
- `bus`: System bus for memory access
- `reservation`: Optional LR/SC reservation address
- `satp`: SATP CSR value (for virtual address translation)
- `priv_mode`: Current privilege mode

**Outputs:**
- `MemResult`: Transformed result ready for writeback
  - Converts `Load` -> `WriteInt` (with loaded value)
  - Converts `Store` -> `Jump` (after storing)
  - Passes through non-memory operations unchanged

**MMU Integration:**
All memory accesses (loads, stores, atomics) go through the MMU:
```rust
// For loads
let phys_addr = mmu::translate(addr, satp, priv_mode, bus, false, false)?;
let val = load_int(bus, phys_addr, funct3)?;

// For stores
let phys_addr = mmu::translate(addr, satp, priv_mode, bus, true, false)?;
store_int(bus, phys_addr, val, funct3)?;
```

**Load Operations:**
- Sign-extension for `lb`, `lh`, `lw`
- Zero-extension for `lbu`, `lhu`, `lwu`
- No extension for `ld` (already 64-bit)
- **Error:** `LoadAccessFault(addr)` if unmapped, misaligned, or read permission denied

**Store Operations:**
- Truncate value to appropriate width (`sb`=8-bit, `sh`=16-bit, etc.)
- **Error:** `StoreAccessFault(addr)` if unmapped, misaligned, or write permission denied

**Atomic Operations (A extension):**
- `lr.w/d`: Set reservation, return loaded value
- `sc.w/d`: If reservation valid, store and return 0; else return 1
- `amo*`: Atomic read-modify-write (add, xor, and, or, min, max, swap)
- Reservation cleared on any store or context switch
- All atomic operations use MMU translation

**FP Loads/Stores:**
- `flw`: Load 32-bit, NaN-box to 64-bit (upper 32 bits = 0xFFFF_FFFF)
- `fld`: Load 64-bit directly
- `fsw`: Store lower 32 bits
- `fsd`: Store all 64 bits
- FP loads/stores also use MMU translation


### 3.5 Writeback Stage
**Responsibility:** Commit results to register file and CSRs.

**Inputs:**
- `mem_result`: From memory stage
- `regs`: Register file (mutable)
- `csrs`: CSR file (mutable)

**Outputs:**
- `next_pc`: Next program counter
- **Error:** `Ecall` or `Ebreak` (trap to handler)

**Integer Register Writes:**
- `x0` is hardwired to zero (writes ignored)
- All other registers updated with result value

**FP Register Writes:**
- NaN-boxing enforced on 32-bit writes (upper 32 bits = 0xFFFF_FFFF)
- Invalid NaN-box detection on reads (returns NaN if upper bits != 0xFFFF_FFFF)

**CSR Operations:**
```rust
match funct3 {
    1 => { /* CSRRW */ new_val = operand; do_write = true; }
    2 => { /* CSRRS */ new_val = old | operand; do_write = (rs1 != 0); }
    3 => { /* CSRRC */ new_val = old & !operand; do_write = (rs1 != 0); }
    5 => { /* CSRRWI */ new_val = uimm; do_write = true; }
    6 => { /* CSRRSI */ new_val = old | uimm; do_write = (uimm != 0); }
    7 => { /* CSRRCI */ new_val = old & !uimm; do_write = (uimm != 0); }
}

if do_write {
    csrs.write(csr_addr, new_val)?;
}
regs.write_x(rd, old_val); // Always write old value to rd
```

**Special Instructions:**
- `ecall` -> Return `Err(Ecall)` (trap to M-mode handler)
- `ebreak` -> Return `Err(Ebreak)` (debug breakpoint)
- `fence` -> No-op in single-core VM (memory already sequentially consistent)
- `fence.i` -> No-op (no instruction cache in this VM)


## 4. Control & Status Registers (CSRs)

### 4.1 Machine Information Registers (Read-Only)

| Address | Name | Width | Description |
|---------|------|-------|-------------|
| `0xF11` | `mvendorid` | 32 | JEDEC manufacturer ID (0 = non-commercial) |
| `0xF12` | `marchid` | 64 | Architecture ID (0 = open standard) |
| `0xF13` | `mimpid` | 64 | Implementation ID (0 = default) |
| `0xF14` | `mhartid` | 64 | Hardware thread ID (0 = single-hart) |
| `0xF15` | `mconfigptr` | 64 | Pointer to config structure (0 = none) |

### 4.2 Machine Trap Setup

| Address | Name | Width | Description |
|---------|------|-------|-------------|
| `0x300` | `mstatus` | 64 | Machine status register |
| `0x301` | `misa` | 64 | ISA and extensions supported |
| `0x304` | `mie` | 64 | Machine interrupt-enable register |
| `0x305` | `mtvec` | 64 | Machine trap-vector base address |
| `0x340` | `mscratch` | 64 | Scratch register for trap handlers |
| `0x341` | `mepc` | 64 | Machine exception program counter |
| `0x342` | `mcause` | 64 | Machine trap cause |
| `0x343` | `mtval` | 64 | Machine bad address or instruction |
| `0x344` | `mip` | 64 | Machine interrupt-pending register |

### 4.3 Supervisor Trap Setup

| Address | Name | Width | Description |
|---------|------|-------|-------------|
| `0x100` | `sstatus` | 64 | Supervisor status register (subset of mstatus) |
| `0x104` | `sie` | 64 | Supervisor interrupt-enable register |
| `0x105` | `stvec` | 64 | Supervisor trap-vector base address |
| `0x140` | `sscratch` | 64 | Supervisor scratch register |
| `0x141` | `sepc` | 64 | Supervisor exception program counter |
| `0x142` | `scause` | 64 | Supervisor trap cause |
| `0x143` | `stval` | 64 | Supervisor bad address or instruction |
| `0x144` | `sip` | 64 | Supervisor interrupt-pending register |
| `0x180` | `satp` | 64 | Supervisor address translation and protection |

#### `satp` Fields (bit positions)
- `[63:60]` **MODE**: Virtual memory mode
  - `0` = Bare (no translation, physical addresses)
  - `8` = Sv39 (3-level page table, 39-bit virtual addresses)
  - Other values = Reserved (writes forced to Bare)
- `[59:44]` **ASID**: Address Space Identifier (not implemented, ignored)
- `[43:0]` **PPN**: Physical Page Number of root page table

**SATP Behavior:**
```rust
// Enable Sv39 with root page table at physical address 0x8000_0000
let ppn = 0x8000_0000 >> 12; // Shift right by 12 to get PPN
let satp = (8u64 << 60) | ppn;
csrs.write(addr::SATP, satp);

// Disable virtual memory (identity mapping)
csrs.write(addr::SATP, 0); // MODE=0 (Bare)
```

**Note:** When SATP.MODE != 0 and privilege mode is not Machine, all memory accesses go through the MMU for address translation.

#### `mstatus` Fields (bit positions)
- `[3]` **MPIE**: Previous M-mode interrupt enable (set on trap, restored on return)
- `[7]` **MPP**: Previous M-mode privilege (always 3 = M-mode in this VM)
- `[12]` **MPRV**: Modify privilege (0 = ignore, not implemented)
- `[13]` **MPV**: Previous virtualization mode (0 = not virtualized)
- `[63]` **SD**: State dirty (read-only, indicates FS/XS dirty)

#### `misa` Fields
- `[12:0]` **MXL**: Max XLEN (2 = 64-bit)
- `[25:0]` **Extensions**: Bit per extension (A=0, D=3, F=5, I=8, M=12)
- **WARL**: Writes may be ignored (this VM supports RV64IMAFD unconditionally)

#### `mtvec` Fields
- `[1:0]` **MODE**: 0 = direct, 1 = vectored (all traps jump to BASE)
- `[63:2]` **BASE**: Trap handler base address (must be 4-byte aligned)

#### `mcause` Encoding
- **Bit 63**: 0 = exception, 1 = interrupt
- **Bits [62:0]**: Exception/interrupt code

| Value | Type | Name | Description |
|-------|------|------|-------------|
| 0 | Exception | Instruction address misaligned | PC not 4-byte aligned |
| 1 | Exception | Instruction access fault | Unmapped instruction fetch or execute permission denied |
| 2 | Exception | Illegal instruction | Invalid opcode or fields |
| 3 | Exception | Breakpoint | `ebreak` executed |
| 4 | Exception | Load address misaligned | Misaligned load address |
| 5 | Exception | Load access fault | Unmapped load address or read permission denied |
| 6 | Exception | Store/AMO address misaligned | Misaligned store address |
| 7 | Exception | Store/AMO access fault | Unmapped store address or write permission denied |
| 8 | Exception | Environment call from U-mode | `ecall` in U-mode (not used) |
| 9-10 | Exception | Reserved | - |
| 11 | Exception | Environment call from M-mode | `ecall` in M-mode |
| 12 | Exception | Instruction page fault | Sv39: Execute permission fault or invalid page table entry |
| 13 | Exception | Load page fault | Sv39: Read permission fault or invalid page table entry |
| 14 | Exception | Reserved | - |
| 15 | Exception | Store/AMO page fault | Sv39: Write permission fault or invalid page table entry |
| 16+ | Interrupt | Various | See interrupt codes below |

**Interrupt Codes (bit 63 set):**
- `0` = User software interrupt (not used)
- `1` = Supervisor software interrupt (not used)
- `2` = Reserved
- `3` = Machine software interrupt (via CLINT MSIP)
- `4` = User timer interrupt (not used)
- `5` = Supervisor timer interrupt (not used)
- `6` = Reserved
- `7` = Machine timer interrupt (via CLINT MTIMECMP)
- `8` = User external interrupt (not used)
- `9` = Supervisor external interrupt (not used)
- `10` = Reserved
- `11` = Machine external interrupt (via PLIC)

### 4.4 Machine Memory Protection (Partial)
- `pmpcfg0` (`0x3A0`) and `pmpaddr0` (`0x3B0`) are implemented as a single PMP entry: writable
  storage plus a basic R/W/X allow/deny check when configured.
- The remaining `pmpcfg`/`pmpaddr` registers are not implemented. This single-entry support is
  enough for the ROM `_start` stub to open an all-address grant before delegating to S-mode.

### 4.5 Floating-Point CSRs

| Address | Name | Width | Description |
|---------|------|-------|-------------|
| `0x001` | `fflags` | 5 | FP exception flags (cumulative) |
| `0x002` | `frm` | 3 | FP dynamic rounding mode |
| `0x003` | `fcsr` | 8 | FP control/status (frm[7:5] + fflags[4:0]) |

#### `fflags` Bits
- `[0]` **NX**: Inexact
- `[1]` **UF**: Underflow
- `[2]` **OF**: Overflow
- `[3]` **DZ**: Divide by zero
- `[4]` **NV**: Invalid operation

**Behavior:** Flags accumulate via OR; cleared by writing 0 or `frcsr`.

#### `frm` Rounding Modes
- `0` = RNE (Round to Nearest, ties to Even)
- `1` = RTZ (Round toward Zero)
- `2` = RDN (Round Down, toward -infinity)
- `3` = RUP (Round Up, toward +infinity)
- `4` = RMM (Round to Nearest, ties to Max Magnitude)
- `5-6` = Reserved (illegal)
- `7` = Dynamic (use frm field in instruction)

### 4.6 Performance Counters

| Address | Name | Width | Description |
|---------|------|-------|-------------|
| `0xB00` | `mcycle` | 64 | Machine cycle counter |
| `0xB02` | `minstret` | 64 | Machine instructions-retired counter |
| `0xC00` | `cycle` | 64 | User-mode alias of mcycle |
| `0xC02` | `instret` | 64 | User-mode alias of minstret |
| `0xC80` | `mcycleh` | 32 | Upper 32 bits of mcycle (RV32 only) |
| `0xC82` | `minstreth` | 32 | Upper 32 bits of minstret (RV32 only) |

**Behavior:**
- `mcycle` increments every clock cycle (approximated by instruction count in this VM)
- `minstret` increments every retired instruction
- Aliases (`cycle`, `instret`) mirror the machine-mode counters
- Counters wrap on overflow (modulo 2^64)


## 5. Trap Handling

### 5.1 Trap Entry Sequence
When an exception or interrupt occurs:

1. **Save state:**
   ```rust
   csrs.mepc = trapped_pc;
   csrs.mcause = cause_code;
   csrs.mtval = fault_address_or_instruction;
   csrs.mstatus.MPIE = csrs.mstatus.MIE;
   csrs.mstatus.MIE = 0; // Disable further interrupts
   csrs.mstatus.MPP = 3; // Save privilege (always M)
   ```

2. **Calculate handler address:**
   ```rust
   let base = csrs.mtvec & !0x3;
   let mode = csrs.mtvec & 0x3;
   
   let handler_pc = if mode == 1 && interrupt {
       base + (cause_code << 2) // Vectored mode
   } else {
       base // Direct mode
   };
   ```

3. **Transfer control:**
   ```rust
   cpu.pc = handler_pc;
   ```

### 5.2 Trap Return (mret)
The `mret` instruction reverses trap entry:

```rust
cpu.pc = csrs.mepc;
csrs.mstatus.MIE = csrs.mstatus.MPIE;
csrs.mstatus.MPIE = 1;
priv_mode = csrs.mstatus.MPP; // Drop to the saved previous privilege (e.g. S-mode)
csrs.mstatus.MPP = 0;
```

**Note:** This VM implements real privilege switching. `mret` restores the privilege saved in
`mstatus.MPP`, and `sret` restores the privilege saved in `sstatus.SPP`. The boot ROM uses `mret`
to enter the S-mode kernel, and the kernel uses `sret` to enter U-mode processes.

### 5.3 Interrupt Checking
Before each instruction fetch, check for pending interrupts:

```rust
let pending = csrs.mip & csrs.mie;
let enabled = csrs.mstatus.MIE;

if enabled && pending != 0 {
    // Take highest-priority interrupt
    let irq_code = highest_set_bit(pending);
    take_trap(TrapCause::Interrupt(irq_code), pc);
}
```

**Priority:** Higher IRQ number = higher priority (machine external > timer > software).

### 5.4 Supervisor-Mode Traps and Delegation

The VM supports trap delegation so the S-mode kernel can handle its own traps without round-tripping
through M-mode:

- `medeleg` delegates selected exception causes (e.g. U-mode ecall, page faults) to S-mode.
- `mideleg` delegates selected interrupt causes (software, timer, external) to S-mode.

When a trap occurs in S/U-mode and its cause is delegated, the CPU traps to S-mode instead of
M-mode: it writes `sepc`, `scause`, and `stval`; updates `sstatus.SPIE`/`sstatus.SPP`; clears
`sstatus.SIE`; and jumps to `stvec`. `sret` reverses this, restoring `sstatus.SPP` as the new
privilege mode. The boot ROM configures `medeleg`/`mideleg` so the kernel's `stvec` handler
receives timer interrupts and U-mode ecalls directly.


## 6. Device Emulation

### 6.1 UART (Universal Asynchronous Receiver/Transmitter)
**Base Address:** `0x1000_0000`  
**Model:** NS16550A subset (8 single-byte registers at offsets `0x00`-`0x07`)

| Offset | Register | Access | Description |
|--------|----------|--------|-------------|
| `0x00` | RBR/THR | R/W | Receiver Buffer / Transmitter Holding |
| `0x01` | IER | R/W | Interrupt Enable Register |
| `0x02` | IIR/FCR | R/W | Interrupt Identification / FIFO Control |
| `0x03` | LCR | R/W | Line Control Register |
| `0x04` | MCR | R/W | Modem Control Register |
| `0x05` | LSR | R | Line Status Register |
| `0x06` | MSR | R | Modem Status Register |
| `0x07` | SCR | R/W | Scratch Register |

**Key Behaviors:**
- **Write to THR (offset 0):** Append byte to TX output buffer
- **Read from RBR (offset 0):** Pop byte from RX input buffer (returns 0 if empty)
- **LSR bit 5:** TX holding register empty (always 1 in this VM)
- **LSR bit 6:** TX shift register empty (always 1 in this VM)
- **LSR bit 0:** Data ready (set when RX buffer non-empty)

**VM Integration:**
- TX buffer accessible via `uart.drain_output()` for test verification
- RX buffer populated via `uart.receive(byte)` for simulated input


### 6.2 CLINT (Core Local Interruptor)
**Base Address:** `0x0200_0000`  
**Purpose:** Machine-mode timer and interprocessor interrupts

| Offset | Register | Access | Description |
|--------|----------|--------|-------------|
| `0x0000` | MSIP | R/W | Machine Software Interrupt Pending (per-hart) |
| `0x4000` | MTIMECMP | R/W | Machine Timer Compare (per-hart, 64-bit) |
| `0xBFF8` | MTIME | R/W | Machine Time (global, 64-bit, free-running) |

**Timer Behavior:**
- `MTIME` increments every clock cycle (simulated)
- When `MTIME >= MTIMECMP`, set `MIP.MTIP` (machine timer interrupt pending)
- Writing to `MTIMECMP` clears the interrupt if condition no longer holds

**Software Interrupt:**
- Writing non-zero to `MSIP` sets `MIP.MSIP`
- Writing zero clears `MIP.MSIP`

**Access Rules:**
- `MTIME` is read-only from CLINT perspective (written by simulation loop)
- `MTIMECMP` and `MSIP` are read/write
- All accesses must be naturally aligned (8-byte for 64-bit registers)


### 6.3 PLIC (Platform-Level Interrupt Controller)
**Base Address:** `0x0C00_0000`  
**Purpose:** Route external interrupts to harts with priority arbitration

**Memory Layout:**
- `0x0000-0x007C`: Priority registers (32 sources x 4 bytes)
- `0x1000-0x107C`: Pending bits (bitfield, 1 bit per source)
- `0x2000-0x207C`: Enable bits (per-context, 1 bit per source)
- `0x200000`: Threshold register (per-context)
- `0x200004`: Claim/Complete register (per-context)

**Operation:**
1. **Set IRQ:** External device calls `plic.set_irq(source_id)`
2. **Arbitration:** VM checks `plic.next_irq(hart_id)` before each instruction
3. **Claim:** Handler reads claim register -> returns highest-priority pending IRQ
4. **Complete:** Handler writes IRQ ID to complete register -> clears pending bit

**Priority Rules:**
- Sources with priority > threshold are eligible
- Highest-priority eligible source wins
- Equal priority -> lowest source ID wins
- Reading claim register automatically clears pending bit

**VM Integration:**
- `plic.set_irq(source)` called by devices (e.g., UART RX)
- `plic.next_irq(hart)` polled by CPU interrupt checker
- Single-context implementation (hart 0 only)


### 6.4 SYSCON (Halt / Exit)
**Base Address:** `0x1001_0000`  
**Purpose:** Stop the machine and report an exit code.

Writing an 8-byte value to SYSCON latches an exit code and signals the run loop to halt. The kernel's
`kshutdown` and the freestanding/hosted exit paths use this to terminate the VM; the integration
harness reads the exit code from the final `RunResult`.


### 6.5 Framebuffer (Linear Display)
**Base Address:** `0x1002_0000`  
**Purpose:** A flat pixel buffer the guest draws into and the GUI displays as an image.

The framebuffer is `320 x 240` pixels in RGBA8888 format: byte `n` is pixel `n / 4`, channel
`n % 4` (0 = R, 1 = G, 2 = B, 3 = A), for a total of `320 * 240 * 4 = 307200` bytes. Like the other
MMIO devices it bypasses the caches, so writes are visible to the display immediately without a
flush. Stores land in the pixel buffer; the GUI uploads the buffer to a texture each frame via the
bus `peek_framebuffer` accessor (which does not perturb device or cache state). Accesses past the end
of the buffer raise a bus error.

The kernel exposes the device to user programs through the `map_fb` syscall (number 107), which maps
the framebuffer's physical pages into the calling process and returns the base virtual address. See
the OS specification for the syscall and the `fbdemo` program.

### 6.6 Cache Hierarchy

RAM accesses pass through three levels of set-associative cache before reaching DRAM. MMIO device
regions are never cached. All levels use 64-byte blocks, true LRU replacement, and a write-back /
write-allocate policy.

| Level | Size | Associativity |
|-------|------|---------------|
| L1 | 4 KB | 2-way |
| L2 | 256 KB | 8-way |
| L3 | 8 MB | 16-way |

A miss at one level fetches the 64-byte block from the next level down (L1 -> L2 -> L3 -> RAM);
write-back evictions push dirty blocks toward RAM. Each level tracks read/write hit and miss
counts, exposed for the debugger's cache view. The caches are purely a performance/visualisation
model: they are kept coherent with RAM for debug reads, and bulk injection of binaries flushes the
hierarchy so the CPU observes the new bytes.


## 7. System Bus

### 7.1 Address Routing
The bus routes memory accesses to the correct device based on address ranges:

```rust
fn route(&mut self, addr: u64) -> Option<(&mut dyn MemoryAccess, u64)> {
    match addr {
        a if a >= UART_BASE  && a <= UART_END  => Some((&mut self.uart,  addr - UART_BASE)),
        a if a >= CLINT_BASE && a <= CLINT_END => Some((&mut self.clint, addr - CLINT_BASE)),
        a if a >= PLIC_BASE  && a <= PLIC_END  => Some((&mut self.plic,  addr - PLIC_BASE)),
        a if a >= ROM_BASE   && a <= ROM_END   => Some((&mut self.rom,   addr)),
        // Everything else routes to RAM through the L1 cache (which cascades to L2/L3/RAM).
        _ => Some((&mut self.l1_cache, addr)),
    }
}
```

UART, CLINT, and PLIC are checked before ROM because their physical addresses fall inside
the ROM range. ROM and RAM receive absolute addresses (the device subtracts its own base
internally); UART, CLINT, and PLIC receive a local offset (the bus subtracts the base).
SYSCON is not in this table: its writes are intercepted in the bus `MemoryAccess` impl,
which latches the exit code rather than dispatching to a device.

### 7.2 Memory Access Trait
All devices implement the `MemoryAccess` trait:

```rust
trait MemoryAccess {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError>;
    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError>;
    fn read_word(&mut self, addr: u64) -> Result<u32, VmError>;
    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError>;
    
    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError>;
    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError>;
    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError>;
    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError>;
}
```

**Default Implementations:**
- Multi-byte reads/writes decomposed into byte operations (little-endian)
- Devices may override for efficiency or alignment enforcement

**Error Handling:**
- `BusError(addr)`: Unmapped address or device-specific error
- `LoadAccessFault(addr)`: Read from unreadable region
- `StoreAccessFault(addr)`: Write to unwritable region (e.g., ROM)

### 7.3 Endianness
All multi-byte accesses use **little-endian** byte order:

```rust
// Example: read_word at address A
byte0 = bus.read_byte(A + 0)?;
byte1 = bus.read_byte(A + 1)?;
byte2 = bus.read_byte(A + 2)?;
byte3 = bus.read_byte(A + 3)?;
word = byte0 | (byte1 << 8) | (byte2 << 16) | (byte3 << 24);
```


## 8. Error Handling

### 8.1 VmError Enum
```rust
enum VmError {
    // Memory access errors
    InstructionAccessFault(u64),
    LoadAccessFault(u64),
    StoreAccessFault(u64),
    BusError(u64),
    
    // Instruction errors
    IllegalInstruction(u32),
    
    // Trap instructions
    Ecall,
    Ebreak,
    
    // Device errors
    WriteToRom,
    
    // CSR errors
    IllegalCsr(u16),
}
```

### 8.2 Error Propagation
- **Pipeline stages** return `Result<T, VmError>`
- **Fetch errors** -> `InstructionAccessFault` trap
- **Decode errors** -> `IllegalInstruction` trap
- **Memory errors** -> `LoadAccessFault` or `StoreAccessFault` trap
- **Ecall/Ebreak** -> Special trap (not an error, but a controlled exit)

### 8.3 Trap vs. Error
- **Traps** (exceptions/interrupts): Saved to CSRs, handler invoked
- **Fatal errors** (e.g., bus routing failure): Halt VM with error message


## 9. Performance Counters & Timing

### 9.1 Cycle Counter
- `mcycle` advances as the pipeline ticks; stall and flush cycles are counted as cycles in which no
  new instruction retires, so `mcycle` exceeds `minstret` under hazards.
- The pipeline tracks cumulative stats (instructions retired, stall cycles, flush cycles, branch
  mispredictions) separately, exposed through `pipeline_stats()` for the debugger.
- Wraps to 0 on overflow (modulo 2^64)

### 9.2 Instruction Retire Counter
- `minstret` increments after successful writeback
- Not incremented for trapped instructions
- Wraps to 0 on overflow (modulo 2^64)

### 9.3 Time Counter
- `mtime` (in CLINT) increments once per cycle
- Used for timer interrupts (compare with `mtimecmp`)
- Monotonically increasing, never wraps (u64 is sufficient)


## 10. Initialization & Reset

### 10.1 VM Construction

```rust
let mut vm = VirtualMachine::new(&assembled);          // flat program
let mut vm = VirtualMachine::new_kernel(&assembled);   // kernel image (boots via ROM)
let mut vm = VirtualMachine::from_elf(&elf_bytes)?;    // arbitrary ELF-64
```

**Steps (`new` / `from_elf`):**
1. Generate the ROM image (from `os-runtime` boot sources) and create the system bus.
2. Map every `PT_LOAD` segment into RAM starting at `RAM_BASE` (`0x8000_0000`), zero-filling any
   `.bss` tail where `mem_size > file_size`.
3. Record the heap base just past the highest mapped address.
4. Initialize the CPU:
   - `pc = entry_point` (from the ELF `e_entry`, e.g. `_start`)
   - `sp = RAM_BASE + RAM_SIZE - 16` (stack grows downward)
   - All other registers = 0; CSRs reset (except read-only `misa`, `mhartid`).

**Kernel construction (`new_kernel`):** the kernel ELF is loaded the same way with `_kernel_start`
as its entry, but the PC is then reset to `ROM_BASE` (`0x0`) and the resolved kernel entry is passed
to the ROM boot stub. The ROM `_start` configures PMP and delegation, sets `mepc` to the kernel
entry, and `mret`s into S-mode. See the OS specification for the full boot protocol.

### 10.2 Reset State
- **Registers:** All integer/FP registers = 0 (except `sp`)
- **CSRs:** All writable CSRs = 0
- **Memory:** RAM zeroed except loaded program sections
- **Devices:** UART buffers empty, CLINT timers = 0, PLIC priorities = 0
- **Reservation:** LR/SC reservation = None


## 11. Execution Loop

### 11.1 Pipelined Tick
The VM advances one clock cycle per `step()`/`tick()`. Each cycle moves every in-flight
instruction one stage forward, so up to five instructions are in flight at once. The five stage
functions (Section 3) operate on pipeline registers (IF/ID, ID/EX, EX/MEM, MEM/WB) rather than a
single instruction:

```
each tick:
    WB   = writeback(MEM/WB)        // retire, commit registers/CSRs
    MEM  = memory(EX/MEM)           // loads, stores, atomics via MMU + cache
    EX   = execute(ID/EX)           // ALU, branch resolve, forwarded operands
    ID   = decode(IF/ID)            // decode, read registers, detect hazards
    IF   = fetch(pc)                // translate + read instruction word

    // Hazard handling between stages:
    //  - data forwarding: EX/MEM and MEM/WB results fed back into EX
    //  - load-use stall:  hold IF/ID one cycle, inject a bubble after ID
    //  - branch mispredict: squash IF/ID, redirect fetch (2-cycle penalty)
    //  - the branch predictor (2-bit bimodal + BTB) supplies the next fetch PC
```

Interrupts are checked at the fetch boundary; when one is taken (or an exception is raised) the
pipeline is squashed and control transfers to the relevant trap vector (`mtvec` or, for delegated
causes, `stvec`). An `ecall` serviced in WB likewise squashes the younger in-flight instructions
before the handler runs.

### 11.2 Halt Condition
The VM halts when a write to the SYSCON device (`0x1001_0000`) latches an exit code. Kernel code
reaches this through `kshutdown`, and hosted/freestanding programs through their `exit` path
(which the ROM or kernel turns into a SYSCON write). The exit code is a signed 64-bit integer;
negative codes indicate errors. The host run loop also stops if a step returns an unrecoverable
`VmError` or the `max_steps` budget is exhausted.


## 12. Testing & Verification

### 12.1 Unit Tests
Test individual components:
- **ALU operations:** Edge cases (overflow, division by zero, shifts)
- **CSR read/write:** WARL behavior, side effects
- **Memory access:** Alignment, bounds checking, endianness
- **Device emulation:** UART I/O, timer interrupts, PLIC arbitration

### 12.2 Integration Tests
Test full program execution:
- **Arithmetic:** Correctness of calculations
- **Control flow:** Branches, loops, function calls
- **Memory:** Load/store correctness, stack operations
- **Traps:** Exception handling, interrupt response
- **I/O:** UART output verification

### 12.3 Golden Master Tests
Compare assembled output against known-good binaries:
- Verify instruction encoding matches RISC-V spec
- Validate section layout (text, rodata, data, bss)
- Check symbol table correctness


## Appendix A: Quick Reference

### A.1 Common CSR Addresses
```
mstatus    = 0x300
misa       = 0x301
mie        = 0x304
mtvec      = 0x305
mscratch   = 0x340
mepc       = 0x341
mcause     = 0x342
mtval      = 0x343
mip        = 0x344
sstatus    = 0x100
sie        = 0x104
stvec      = 0x105
sepc       = 0x141
scause     = 0x142
stval      = 0x143
satp       = 0x180
mcycle     = 0xB00
minstret   = 0xB02
cycle      = 0xC00
instret    = 0xC02
fflags     = 0x001
frm        = 0x002
fcsr       = 0x003
```

### A.2 Exception Codes
```
0  = Instruction address misaligned
1  = Instruction access fault
2  = Illegal instruction
3  = Breakpoint
4  = Load address misaligned
5  = Load access fault
6  = Store/AMO address misaligned
7  = Store/AMO access fault
11 = Environment call from M-mode
12 = Instruction page fault (Sv39)
13 = Load page fault (Sv39)
15 = Store/AMO page fault (Sv39)
```

### A.3 Interrupt Codes (bit 63 set)
```
3  = Machine software interrupt
7  = Machine timer interrupt
11 = Machine external interrupt
```

### A.4 Memory Map Summary
```
0x0000_0000 - ROM    (256 MB)
0x0200_0000 - CLINT  (64 KB)
0x0C00_0000 - PLIC   (16 MB)
0x1000_0000 - UART   (4 KB)
0x1001_0000 - SYSCON (4 KB)
0x8000_0000 - RAM    (128 MB)
```
