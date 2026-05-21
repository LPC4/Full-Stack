# virtual-machine

**Stage 5 of the Full-Stack compiler pipeline.**

A 5-stage in-order RISC-V RV64IMAFD pipeline emulator with caches, MMU (Sv39), M-mode traps, and memory-mapped devices.

## What it does

```
AssembledOutput / ELF bytes
  → VirtualMachine::new() / from_elf()
  → run(max_steps) → RunResult { uart_output, outcome }
```

## Public API

```rust
use virtual_machine::{VirtualMachine, RunResult};
use asm_to_binary::AssembledOutput;

let mut vm = VirtualMachine::new(&assembled);
let result = vm.run(1_000_000);
println!("{}", result.uart_output);
```

## Pipeline stages

`fetch → decode → execute → memory → writeback`

- Hazard unit handles data hazards (stall + forwarding)
- Branch predictor
- Sv39 MMU with bare-mode passthrough
- M-mode CSRs and trap handling / `mret`
 - Partial PMP CSR support (simple single-entry enforcement) to support ROM
   handoff into S-mode. This implements storage and a basic allow/deny check
   for R/W/X when configured; full PMP semantics are not yet implemented.

## Memory map

| Region | Address |
|--------|---------|
| ROM | `0x0000_0000` |
| RAM | `0x8000_0000` |
| UART | `0x1000_0000` |
| CLINT | `0x0200_0000` |
| PLIC | `0x0C00_0000` |
| SYSCON | `0x0010_0000` |

## ROM firmware

`rom/rom.s` is the M-mode trap handler and ecall dispatch, embedded at compile time via `include_str!`.
