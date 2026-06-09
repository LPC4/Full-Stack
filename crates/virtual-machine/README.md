# virtual-machine

**Stage 5 of the Full-Stack compiler pipeline.**

A cycle-stepped RISC-V RV64IMAFD emulator. It runs a 5-stage in-order pipeline with data
forwarding, hazard detection, and branch prediction over a three-level write-back cache
hierarchy, plus an Sv39 MMU, M/S/U privilege modes with trap handling, and memory-mapped
devices (UART, CLINT, PLIC, SYSCON, Framebuffer).

## Flow

```
AssembledOutput / ELF bytes
  -> VirtualMachine::new() | new_kernel() | from_elf()
  -> step() repeatedly, or run(max_steps)
  -> RunResult { steps, uart_output, outcome }
```

Each `step()` advances the pipeline by one cycle. `new()` loads a flat program and starts
at its entry point; `new_kernel()` loads a kernel image but resets the PC to the ROM
boot stub so firmware runs first and `mret`s into S-mode.

## Public API

```rust
use virtual_machine::{VirtualMachine, RunResult, StepOutcome};
use asm_to_binary::AssembledOutput;

let mut vm = VirtualMachine::new(&assembled);
let result: RunResult = vm.run(1_000_000);
print!("{}", result.uart_output);

// Or drive it manually and inspect state:
vm.uart_receive(b'a');                 // feed UART input
let outcome = vm.step()?;              // StepOutcome::{Continue, Halted(code)}
let regs = vm.peek_all_xregs();
let (l1, l2, l3) = vm.get_cache_stats();
let pipeline = vm.pipeline_snapshot();
```

The VM exposes rich debug accessors (registers, CSRs, raw memory, cache snapshots, and
per-cycle pipeline state) used by the egui debugger in the parent app.

## Pipeline

```
IF -> ID -> EX -> MEM -> WB
```

- Data forwarding from EX/MEM and MEM/WB into EX (most recent producer wins).
- Load-use hazards stall one cycle (IF held, bubble injected after ID).
- Branch mispredicts flush IF/ID (2-cycle penalty); a 2-bit bimodal predictor with a
  Branch Target Buffer drives speculation, defaulting to not-taken for unseen PCs.

## Cache hierarchy

All levels use 64-byte blocks, true LRU replacement, and write-back / write-allocate.

| Level | Size | Associativity |
|-------|------|---------------|
| L1 | 4 KB | 2-way |
| L2 | 256 KB | 8-way |
| L3 | 8 MB | 16-way |

RAM accesses cascade L1 -> L2 -> L3 -> RAM; MMIO regions bypass the caches.

## Memory map

| Region | Base | Size |
|--------|------|------|
| ROM | `0x0000_0000` | 256 MB |
| CLINT | `0x0200_0000` | 64 KB (mtime/mtimecmp/msip) |
| PLIC | `0x0C00_0000` | 16 MB |
| UART | `0x1000_0000` | NS16550A subset |
| SYSCON | `0x1001_0000` | halt/exit device |
| Framebuffer | `0x1002_0000` | 320x240 RGBA8888 linear display |
| RAM | `0x8000_0000` | 128 MB |

## Module layout

```
src/
  cpu/        pipeline stages, ALU, decoder, CSRs, MMU, hazard unit, predictor, traps
  memory/     RAM, ROM, and the L1/L2/L3 cache
  devices/    UART, CLINT, PLIC, Framebuffer
```

The system bus, ELF parser, ROM image generator, and top-level `VirtualMachine` sit
alongside these as sibling modules.

## ROM firmware

The M-mode boot ROM and trap handler are not stored in this crate. They come from
`os-runtime` (`boot/startup.s`, `boot/trap.s`) and are assembled into the ROM image at
construction time. See `crates/os-runtime` and the OS specification for the boot protocol.
