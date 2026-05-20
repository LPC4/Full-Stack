# ROM TODO - What Real Hardware ROM/Firmware Does

This documents everything a real RISC-V M-mode ROM/firmware handles, compared to
what we currently split between `platform/rom/rom.s` (ROM) and Rust (`traps.rs` /
`pipeline.rs`).

Legend:
- `[ROM]`  - already in rom.s
- `[RUST]` - currently handled in Rust, should move to ROM
- `[NONE]` - not implemented anywhere yet

---

## 1. Boot / Reset Vector

`[NONE]` **Reset vector at ROM_BASE** - on reset, hart jumps to a fixed reset vector
(0x1000 on SiFive boards, our ROM_BASE=0x0). Real firmware starts here, not directly
at _trap_entry. The first few instructions set up the machine before any trap handler
is installed.

`[NONE]` **mtvec initialization** - ROM writes the trap handler address into `mtvec`
before jumping to user code. Currently Rust hard-codes `csrs.mtvec = 0x0000` in
`Pipeline::new()`. Real firmware does `la t0, _trap_entry; csrw mtvec, t0`.

`[NONE]` **mscratch initialization** - ROM sets `mscratch` to point at a per-hart
M-mode scratch/stack area. Currently `mscratch` is zero and unused.

`[NONE]` **Stack pointer for M-mode** - ROM establishes a separate M-mode stack
(separate from the user stack). Currently the trap handler shares the user stack,
which is wrong if the user stack is corrupted or full.

`[NONE]` **PMP (Physical Memory Protection) setup** - real ROM configures PMP
entries to grant S/U-mode access to RAM and deny access to ROM/MMIO. Without PMP,
U-mode can read/write ROM addresses, which is wrong. Entry: `pmpcfg0` / `pmpaddr0`.

`[NONE]` **mhartid check / multi-hart parking** - real ROM reads `mhartid`; only
hart 0 proceeds with init, all others spin on a flag (hart parking loop).

`[NONE]` **Jump to user entry point** - after init, ROM does `la t0, _start; jr t0`
(or `mret` to U-mode). Currently Rust sets `fetch_pc` to the ELF entry point directly.

---

## 2. Trap Entry - Register Context Save

`[RUST]` **Save mepc** - `csrs.mepc = pc` is done in `traps::take_trap()` in Rust.
On real hardware, the CPU saves mepc in hardware before jumping to mtvec; firmware
can just read it. The save itself is correct, but the *architectural action* is
hardware, not a software responsibility - so this one is fine as Rust.

`[RUST]` **Save mcause / mtval** - same as mepc, hardware fills these on trap entry.
Rust emulates this in `take_trap()`.

`[RUST]` **mstatus MIE→MPIE, MPP update** - hardware does this atomically on trap
entry; our Rust `take_trap()` emulates it. Correct behavior, but it lives in Rust
instead of being implicit in the pipeline.

`[NONE]` **Save all caller-saved registers (a0-a7, t0-t6, ra) to a trap frame** -
real M-mode trap handlers do `csrrw sp, mscratch, sp` then store all registers to
the scratch area. Without this, interrupted code's registers are trashed if the
trap handler uses them. Our ROM handler clobbers t0-t6 freely - this only works
because we never return to an interrupted instruction (ecall always advances mepc).
For non-ecall exceptions (illegal insn, fault) this is a real bug: regs are
corrupted on return.

`[NONE]` **Save callee-saved registers (s0-s11)** - needed if the trap handler calls
subroutines that might not preserve them. Currently not needed since our handler
never returns through non-ecall paths.

`[NONE]` **csrrw sp, mscratch, sp** - swap user sp with M-mode stack pointer at
trap entry. Real firmware first instruction: `csrrw sp, mscratch, sp`. Currently
the ROM handler just uses `addi sp, sp, -N` on whatever sp happens to be.

---

## 3. Trap Dispatch - Sync vs Async, Cause Routing

`[ROM]`  **Read mcause, branch on ecall causes (8/9/11)** - done in `_trap_entry`.

`[NONE]` **Check mcause bit 63 (interrupt vs exception)** - real dispatch first
checks if the trap is an interrupt (`mcause < 0` in signed terms). Currently ROM
only handles ecall and falls through to `mret` for everything else, silently ignoring
all exceptions and all interrupts.

`[NONE]` **Exception dispatch table** - real firmware has a jump table or chain of
compares for all synchronous exception causes:
  - cause 0: instruction address misaligned
  - cause 1: instruction access fault
  - cause 2: illegal instruction → could emulate the insn or signal SIGILL
  - cause 3: breakpoint (EBREAK)
  - cause 4: load address misaligned → software emulation handler
  - cause 5: load access fault
  - cause 6: store address misaligned → software emulation handler
  - cause 7: store access fault
  - cause 12: instruction page fault
  - cause 13: load page fault
  - cause 15: store/AMO page fault
  Currently ALL of these are handled in Rust (`flush_and_trap` → `take_trap`), which
  means ROM doesn't know they happened at all.

`[NONE]` **Interrupt dispatch table** - for async interrupts (bit 63 set):
  - cause 3: M-mode software interrupt (MSIP from CLINT)
  - cause 7: M-mode timer interrupt (MTIP from CLINT)
  - cause 11: M-mode external interrupt (MEIP from PLIC)
  ROM should read the cause, call the appropriate ISR, and clear the pending bit.

---

## 4. Interrupt Handlers

`[NONE]` **Timer interrupt (MTIP)** - on timer fire, ROM ISR should:
  1. Read `mtime` from CLINT (0x0200_0000)
  2. Set `mtimecmp` (0x0200_4000) to `mtime + interval` to clear the pending IRQ
  3. Optionally notify S-mode via a software interrupt (SBI timer callback)
  4. `mret`
  Currently: no timer ISR at all; CLINT exists on the bus but nothing handles MTIP.

`[NONE]` **Software interrupt (MSIP)** - triggered by writing CLINT MSIP register.
  ROM ISR should clear MSIP and do work (IPI in multi-hart systems).

`[NONE]` **External interrupt (MEIP)** - PLIC-driven. ROM ISR should:
  1. Claim the interrupt from PLIC claim register
  2. Dispatch to the correct device handler
  3. Complete the interrupt by writing the claim register back
  Currently: PLIC exists on the bus but no ISR handles it.

---

## 5. Trap Return (MRET / SRET)

`[RUST]` **MRET: restore mstatus (MPIE→MIE, MPP→priv), jump to mepc** - done in
`traps::handle_mret()`. The CPU executes the MRET instruction and Rust does the
CSR restore and PC redirect. On real hardware, MRET is a privileged instruction
that the CPU executes directly, updating mstatus atomically and setting PC=mepc.
This is as close to "hardware" as our simulator gets; moving it to ROM would mean
ROM re-implementing what MRET does - that makes no sense. Keep in Rust.

`[ROM]`  **Advance mepc by 4 before mret (for ecall)** - done in rom.s. Correct.

`[NONE]` **Restore all saved registers from trap frame before mret** - pairs with
the missing context save. Without it, any trap handler that used scratch registers
leaves them trashed.

`[FIXED]` **Prevent speculative fetch past MRET/SRET** - previously, the pipelined
CPU would speculatively fetch instructions past MRET while it was executing in
EX/MEM/WB stages. If those fetches went past ROM boundary, they triggered an
instruction access fault that corrupted MEPC before MRET completed, causing MRET
to return to the wrong address.

**Fix implemented**: Added `mret_in_flight` flag to Pipeline struct. When MRET/SRET
is decoded in ID stage, the flag is set and IF stage stalls until MRET completes in
WB and flushes the pipeline. This matches real RISC-V hardware behavior where
control-flow changing instructions prevent further speculation.

Files modified:
- `src/4_virtual_machine/cpu/pipeline.rs`: Added `mret_in_flight` flag, ID stage
  detection, IF stage stalling, and flag clearing in handle_mret/handle_sret
- `src/4_virtual_machine/cpu/traps.rs`: Removed hacky MEPC preservation logic,
  returned to simple correct behavior

---

## 6. Ecall / Syscall Dispatch

`[ROM]`  **a7-based syscall dispatch** - done. Handles 64/93/94/1000/1001/1002.

`[NONE]` **sys_brk (214)** - grow heap. sbrk-like.

`[NONE]` **sys_read (63)** - read from stdin (UART RX).

`[NONE]` **sys_getpid (172)**, **sys_gettid (178)** - trivially return 1.

`[NONE]` **sys_mmap (222)** - anonymous mmap, useful for malloc fallback.

`[NONE]` **sys_rt_sigaction / sys_rt_sigprocmask** - no-op stubs so libc doesn't fault.

`[NONE]` **Return -ENOSYS (-38) for unknown syscalls** - currently returns -1, which
is ambiguous. Proper Linux ABI returns the negated errno.

---

## 7. Misaligned Access Emulation

`[NONE]` **Misaligned load handler** - cause 4 (load address misaligned). Real
M-mode firmware can emulate misaligned loads with byte-at-a-time loads. Currently
our bus panics / returns a bus error; the trap is caught in Rust and not emulated.

`[NONE]` **Misaligned store handler** - cause 6 (store address misaligned). Same.

---

## 8. Delegation (medeleg / mideleg)

`[NONE]` **medeleg / mideleg CSRs** - real firmware writes these to delegate certain
traps to S-mode without going through M-mode at all. For example, a kernel sets
`medeleg = 0xB35D` to delegate page faults, syscalls, breakpoints to S-mode.
Currently both CSRs exist in the CsrFile but delegation logic is not implemented
in the trap dispatch - all traps always go to M-mode.

`[NONE]` **S-mode trap entry (stvec / sepc / scause / stval / sstatus)** - if a
trap is delegated, the firmware (or hardware) uses `stvec` not `mtvec`, and saves
to `sepc`/`scause`/`stval`. Currently only M-mode CSRs are used.

---

## 9. SBI (Supervisor Binary Interface)

`[NONE]` **SBI ecall from S-mode (cause 9)** - real M-mode firmware implements SBI:
  - EID 0x00 (legacy): `sbi_set_timer`
  - EID 0x01 (legacy): `sbi_console_putchar`
  - EID 0x10: SBI base extension (probe, get_spec_version)
  - EID 0x54494D45: Timer extension
  Currently ecall from S-mode is dispatched the same as U-mode by the same syscall
  table, which is wrong for a kernel that sends SBI calls.

---

## 10. Floating-Point Trap Handling

`[NONE]` **fflags trap-on-exception** - real hardware can be configured (via `mstatus.FS`
and fcsr) to generate traps on FP exceptions (NV, DZ, OF, UF, NX). We accumulate
fflags in Rust but never generate a trap from them.

---

## Summary: What Rust Should Stop Doing

| Responsibility | Currently In | Should Move To |
|---|---|---|
| Set mtvec | `Pipeline::new()` (Rust) | ROM boot sequence |
| Set mscratch | - (nowhere) | ROM boot sequence |
| mstatus MIE→MPIE, MPP | `traps::take_trap()` (Rust) | keep in Rust (hardware action) |
| All exception dispatch | Rust `flush_and_trap` | ROM dispatch table (cause checks) |
| Interrupt dispatch | - (nowhere) | ROM ISR table |
| EBREAK handling | Rust | ROM (could log/halt) |
| Register context save | - (nowhere) | ROM trap entry |
| MRET CSR restore | Rust `handle_mret` | keep in Rust (hardware action) |
| Register context restore | - (nowhere) | ROM before mret |
| mscratch sp swap | - (nowhere) | ROM trap entry / exit |
| Speculative fetch control on MRET | **FIXED** in Pipeline | N/A - properly handled |

---

## Recent Fixes

### Fixed: Nested Trap MEPC Corruption

**Problem**: Platform stdlib tests failing with truncated output due to nested traps
corrupting MEPC during MRET execution.

**Root Cause**: Pipelined CPU speculative fetch went past MRET instruction, triggered
instruction access fault at ROM boundary, which overwrote MEPC before MRET completed.

**Solution**: Implemented proper IF stage stalling when MRET/SRET is in flight:
- Added `mret_in_flight` flag to Pipeline
- ID stage detects MRET/SRET and sets flag
- IF stage stalls while flag is set (no speculative fetch)
- WB stage clears flag after MRET completes and flushes pipeline

**Result**: All 445 tests pass, including 17 platform stdlib tests. Fix matches real
RISC-V hardware behavior where control-flow instructions prevent speculation.
