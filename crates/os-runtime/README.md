# operating-system

OS specification and kernel firmware for the Full-Stack platform.

This crate is **not a compiled Rust library**. It holds:

- `rom/rom.s` - canonical M-mode ROM firmware (trap handler, ecall dispatch). Copied into `crates/virtual-machine/rom/rom.s` for embedding.
- `kernel/runtime_kernel.hll` - kernel entry point and `kmalloc`; prepended to kernel-mode HLL programs.
- `_OS_SPECIFICATION.md` - full OS and memory-map specification.

## Memory map (summary)

| Region | Base address | Size |
|--------|-------------|------|
| ROM | `0x0000_0000` | 64 KB |
| CLINT | `0x0200_0000` | 64 KB |
| PLIC | `0x0C00_0000` | 64 MB |
| UART 0 | `0x1000_0000` | 4 KB |
| SYSCON | `0x0010_0000` | 4 KB |
| RAM | `0x8000_0000` | 128 MB |

See `_OS_SPECIFICATION.md` for the full memory map, ecall ABI, and trap handling details.
