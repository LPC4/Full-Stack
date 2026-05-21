# Kernel

This directory contains the S-mode kernel written in HLL.
`runtime_kernel.hll` is the current entry point used by the GUI; the files below
describe the planned full kernel layout.

---

## Planned directory layout

```
kernel/
├── boot.hll             S-mode entry: set stvec, enable satp, call kmain
├── platform.hll         platform_putc / platform_halt / kprint / kprint_hex
├── mm/
│   ├── frame_alloc.hll  physical frame allocator (bump → free-list)
│   └── paging.hll       Sv39: map_page, map_range, paging_init
├── proc/
│   ├── task.hll         TaskControlBlock, task list
│   ├── scheduler.hll    round-robin tick scheduler, scheduler_start
│   └── context.hll      context_save / context_restore (asm blocks)
├── trap/
│   ├── trap_entry.s     assembly stub: save all regs → call handler → sret
│   └── trap_handler.hll dispatch: timer → scheduler, ecall → syscalls, faults
├── syscall/
│   └── syscall.hll      syscall table: write, exit, fork, wait, exec
└── fs/
    └── ramfs.hll        in-memory filesystem (add last, after syscalls work)
```

---

## Boot flow

The boot has three distinct layers. Each has one job and one handoff point.

```
Power on
  └─ ROM (_start, M-mode)          ← firmware, not the OS
       └─ PMP + delegate + mret
            └─ _kstart (asm stub, S-mode)
                 └─ stack + early stvec + BSS clear
                      └─ kmain() [HLL, S-mode]
                           ├─ uart_init
                           ├─ frame_alloc_init
                           ├─ paging_init          ← MMU on after this
                           ├─ trap_init            ← real stvec installed
                           ├─ timer_set            ← first scheduler tick armed
                           ├─ sstatus.SIE = 1      ← interrupts enabled
                           └─ scheduler_start()    ← never returns
```

---

## Layer 1 — ROM (`rom/rom.s`, M-mode)

The ROM is firmware, not the kernel. Its only job is to give the kernel a clean
S-mode environment. It does exactly four things, then steps back permanently.

```asm
_start:
    # 1. PMP — without this, S-mode cannot touch RAM at all.
    #    One entry covering the full address space, RWX.
    li    t0, -1
    csrw  pmpaddr0, t0
    li    t0, 0x1F              # TOR, R+W+X
    csrw  pmpcfg0, t0

    # 2. Delegate exceptions to S-mode
    #    bit  8 = ecall from U-mode   (user syscalls)
    #    bit 12 = instruction page fault
    #    bit 13 = load page fault
    #    bit 15 = store page fault
    li    t0, (1<<8)|(1<<12)|(1<<13)|(1<<15)
    csrw  medeleg, t0

    # 3. Delegate interrupts to S-mode
    #    bit 1 = supervisor software interrupt
    #    bit 5 = supervisor timer interrupt   (scheduler tick)
    #    bit 9 = supervisor external interrupt (PLIC / devices)
    li    t0, (1<<1)|(1<<5)|(1<<9)
    csrw  mideleg, t0

    # 4. mret into S-mode at the kernel entry point.
    #    mstatus.MPP = 01 → mret drops privilege to S-mode.
    li    t0, 0x1800            # mask: MPP field is bits [12:11]
    csrc  mstatus, t0           # clear MPP
    li    t0, 0x0800            # MPP = 01 = S-mode
    csrs  mstatus, t0
    la    t0, _kstart
    csrw  mepc, t0
    mret                        # jumps to _kstart in S-mode
```

After `mret` the ROM is done. It does not run again unless the kernel explicitly
requests an SBI service (e.g. setting the timer via an `ecall` back to M-mode).

---

## Layer 2 — Kernel entry stub (`_kstart`, assembly)

The CPU is now in S-mode but in a completely raw state: no stack, no trap
handler, BSS uninitialised. Three things must be fixed before calling any HLL:

```asm
.section .text.boot
.global _kstart
_kstart:
    # 1. Stack — every subsequent operation needs a valid sp.
    la    sp, __stack_top

    # 2. Early trap handler — if anything goes wrong before the real handler
    #    is installed, land somewhere sane rather than jumping to address 0.
    la    t0, _early_trap
    csrw  stvec, t0

    # 3. Clear BSS — the HLL compiler assumes all globals start as zero.
    #    Calling any HLL function before this is undefined behavior.
    la    t0, __bss_start
    la    t1, __bss_end
.clear:
    bgeu  t0, t1, .done
    sd    zero, 0(t0)
    addi  t0, t0, 8
    j     .clear
.done:
    # a0 = hart_id, a1 = dtb_ptr — passed through untouched from ROM
    call  kmain
    j     .                     # should never return

.align 4
_early_trap:
    j     _early_trap           # loop; replaced once trap_init() runs
```

---

## Layer 3 — `kmain` (HLL, S-mode)

Each step expands what the kernel can safely do. The order is not arbitrary.

```hll
# kernel/boot.hll

const RAM_BASE:      u64 = 0x80000000
const RAM_SIZE:      u64 = 0x80000000   # 2 GB
const PAGE_SIZE:     u64 = 4096
const TICK_INTERVAL: u64 = 1000000      # cycles between scheduler ticks

kmain: (hart_id: u64, dtb_ptr: u64) -> () {
    if hart_id != 0 {
        loop { asm { wfi } }             # park extra harts (single-core for now)
    }

    # UART first — nothing else can be debugged without output.
    uart_init()
    kprint("boot: uart\n")

    # Frame allocator — must know where the kernel image ends.
    # __bss_end is a linker symbol: first free physical byte after the image.
    frame_alloc_init(align_up(__bss_end, PAGE_SIZE), RAM_BASE + RAM_SIZE)
    kprint("boot: frame allocator\n")

    # Page tables, then enable Sv39.
    # After this returns every memory access goes through the MMU.
    paging_init()
    kprint("boot: paging\n")            # this UART write already uses MMU

    # Real trap handler replaces _early_trap.
    trap_init()
    kprint("boot: traps\n")

    # Arm the first timer tick.
    timer_set(TICK_INTERVAL)
    kprint("boot: timer\n")

    # Enable interrupts — only safe after the handler is installed.
    asm {
        csrsi sstatus, 0x2              # sstatus.SIE = 1
    }

    kprint("boot: kernel ready\n")
    scheduler_start()                   # never returns
}
```

---

## `paging_init` in detail

The critical insight: the CPU fetches the instruction *after* `csrw satp` using
the MMU. If the kernel isn't mapped at its own physical address, the next fetch
is an instant page fault. The solution is an **identity map** (virtual = physical)
for the kernel range — the address space switch becomes invisible.

```hll
# kernel/mm/paging.hll

const PTE_V: u64 = 1     # valid
const PTE_R: u64 = 2     # readable
const PTE_W: u64 = 4     # writable
const PTE_X: u64 = 8     # executable
const PTE_A: u64 = 64    # accessed (must be set or hw faults on first access)
const PTE_D: u64 = 128   # dirty    (must be set for writable pages)

global kernel_root_pt: u64

paging_init: () -> () {
    kernel_root_pt = alloc_frame()

    # Identity-map kernel code + data (va = pa).
    map_range(kernel_root_pt,
              RAM_BASE, RAM_BASE,
              align_up(__bss_end, PAGE_SIZE) - RAM_BASE,
              PTE_R | PTE_W | PTE_X)

    # Identity-map MMIO (no execute permission).
    map_range(kernel_root_pt, 0x10000000, 0x10000000, PAGE_SIZE,  PTE_R|PTE_W) # UART
    map_range(kernel_root_pt, 0x02000000, 0x02000000, 0x10000,    PTE_R|PTE_W) # CLINT
    map_range(kernel_root_pt, 0x0C000000, 0x0C000000, 0x1000000,  PTE_R|PTE_W) # PLIC

    # Enable Sv39: satp = (MODE=8 << 60) | root_ppn
    satp_val: u64 = (8 << 60) | (kernel_root_pt >> 12)
    asm {
        csrw  satp, a0              # a0 = satp_val
        sfence.vma zero, zero       # flush entire TLB
    }
    # Execution continues — identity mapping means same addresses, no visible jump.
}

# Walk / create 3-level page table and write a leaf PTE.
map_page: (root: u64, va: u64, pa: u64, flags: u64) -> () {
    vpn2: u64 = (va >> 30) & 0x1FF
    vpn1: u64 = (va >> 21) & 0x1FF
    vpn0: u64 = (va >> 12) & 0x1FF

    l2e: u64 = root + vpn2 * 8
    l1_table: u64 = 0
    if (*((u64*)l2e) & PTE_V) == 0 {
        l1_table = alloc_frame()
        *((u64*)l2e) = ((l1_table >> 12) << 10) | PTE_V
    } else {
        l1_table = (*((u64*)l2e) >> 10) << 12
    }

    l1e: u64 = l1_table + vpn1 * 8
    l0_table: u64 = 0
    if (*((u64*)l1e) & PTE_V) == 0 {
        l0_table = alloc_frame()
        *((u64*)l1e) = ((l0_table >> 12) << 10) | PTE_V
    } else {
        l0_table = (*((u64*)l1e) >> 10) << 12
    }

    l0e: u64 = l0_table + vpn0 * 8
    *((u64*)l0e) = ((pa >> 12) << 10) | flags | PTE_V | PTE_A | PTE_D
}

map_range: (root: u64, va: u64, pa: u64, size: u64, flags: u64) -> () {
    offset: u64 = 0
    while offset < size {
        map_page(root, va + offset, pa + offset, flags)
        offset = offset + PAGE_SIZE
    }
}
```

---

## Trap entry and handler

The assembly stub saves all 32 registers to a `TrapFrame` on the stack, calls
the HLL handler, restores everything, then executes `sret`.

```asm
# kernel/trap/trap_entry.s
.align 4
.global stvec_entry
stvec_entry:
    addi  sp, sp, -288          # TrapFrame: 32 regs (256 B) + 4 CSRs (32 B)

    sd    x1,   8(sp)           # save all GPRs (x0 omitted, x2/sp special)
    addi  t0, sp, 288
    sd    t0,  16(sp)           # original sp
    sd    x3,  24(sp)
    # ... x4-x31 at offsets 32-248 (one sd per register)

    csrr  t0, sepc;    sd t0, 256(sp)
    csrr  t0, scause;  sd t0, 264(sp)
    csrr  t0, stval;   sd t0, 272(sp)
    csrr  t0, sstatus; sd t0, 280(sp)

    mv    a0, sp                # TrapFrame* argument
    call  trap_handler

    ld    t0, 256(sp); csrw sepc, t0
    ld    t0, 280(sp); csrw sstatus, t0

    ld    x1,  8(sp)            # restore all GPRs (sp last)
    # ... x3-x31
    ld    x2, 16(sp)            # restores original sp, deallocating the frame

    sret                        # returns to sepc at the privilege in sstatus.SPP
```

```hll
# kernel/trap/trap_handler.hll

type TrapFrame = {
    x:       u64[32],
    sepc:    u64,
    scause:  u64,
    stval:   u64,
    sstatus: u64,
}

const INTR_BIT:      u64 = 1 << 63
const CAUSE_S_TIMER: u64 = INTR_BIT | 5    # supervisor timer interrupt
const CAUSE_S_EXT:   u64 = INTR_BIT | 9    # supervisor external (PLIC)
const CAUSE_ECALL_U: u64 = 8               # ecall from U-mode
const CAUSE_INST_PF: u64 = 12
const CAUSE_LOAD_PF: u64 = 13
const CAUSE_STOR_PF: u64 = 15

trap_handler: (frame: TrapFrame*) -> () {
    cause: u64 = frame.scause

    if cause == CAUSE_S_TIMER {
        timer_set(TICK_INTERVAL)    # rearm before rescheduling
        scheduler_tick(frame)       # may rewrite frame.sepc and frame.x[2]
        return
    }

    if cause == CAUSE_ECALL_U {
        syscall_dispatch(frame)     # a7 = number, a0-a5 = args, a0 = return
        frame.sepc = frame.sepc + 4 # skip past the ecall instruction
        return
    }

    if cause == CAUSE_S_EXT {
        plic_handle()
        return
    }

    kprint("trap: cause="); kprint_hex(cause)
    kprint(" sepc=");       kprint_hex(frame.sepc)
    kprint(" stval=");      kprint_hex(frame.stval)
    kprint("\n")
    panic("unhandled trap")
}
```

---

## Why each step must happen in this order

| Step | Why it can't move earlier |
|------|--------------------------|
| UART | Needed to see any of the others fail |
| Frame allocator | `paging_init` calls `alloc_frame`; must exist first |
| `paging_init` | Enables MMU; all subsequent code runs through it |
| `trap_init` | Sets `stvec`; any trap before this hits `_early_trap` |
| `timer_set` | Arming before `trap_init` would fire into `_early_trap` |
| `sstatus.SIE = 1` | Enabling before the handler is installed is a hard hang |

---

## VM changes required before any of this works

The VM currently does not track privilege mode — `mret` exists but does not
actually switch the CPU out of M-mode. M-mode always bypasses the MMU, so
`satp` has no effect, and paging never activates.

Required changes in `crates/virtual-machine/src/cpu/`:

1. **`pipeline.rs`** — add `priv_mode: PrivMode` (enum M/S/U) to CPU state,
   starting at M. Pass it into every pipeline stage that needs it.

2. **`traps.rs`** — `mret` must read `mstatus.MPP` and actually set `priv_mode`;
   add `sret` (reads `sstatus.SPP`, sets `priv_mode`, restores SIE from SPIE).

3. **`csr.rs`** — add writable `mideleg` and `medeleg`. Trap entry: if the cause
   bit is set in the delegation register, use S-mode CSRs (`stvec`, `sepc`,
   `scause`, `stval`, `sstatus`) instead of M-mode ones.

4. **`csr.rs`** — implement `pmpcfg0` + `pmpaddr0` (single TOR entry is enough).
   Without PMP, S-mode cannot access RAM and faults on the first instruction.

The MMU code already does Sv39 page walks and checks the U-bit against
privilege mode — it just always sees M-mode, which bypasses everything.
Once privilege tracking is real, paging activates automatically.
