# Platform

HLL source files for the standard library and kernel runtime. The compiler embeds these at build time via `src/1_high_level_language/stdlib.rs`.

## Directory layout

```
platform/
  stdlib/
    common/          # target-independent — included in every build
      types.hll          Str and HeapBlock type definitions
      memory_allocator.hll  64 KB static heap (bump + free-list)
      string_utils.hll   str_len, str_equals, str_copy, str_concat
      mem.hll            memset, memcpy, memmove, memcmp
      klog.hll           klog / klog_ok / klog_warn / klog_error / klog_int / klog_hex
    hosted/          # Linux userspace target
      runtime.hll        _start, putchar/puts/print_int/printf/exit, console_* shim
    freestanding/    # bare-metal target (no OS)
      runtime.hll        _kpanic, kpanic (inline UART, no dependencies)
      console.hll        console_putchar/write/writeln/print_int/print_hex via NS16550A UART
  kernel/
    runtime_kernel.hll   _kernel_start, kmalloc (OOM-panicking malloc wrapper)
  rom/
    rom.s                ROM firmware (RISC-V assembly, loaded at reset vector)
```

## Stdlib bundles

The compiler assembles two bundles depending on the target mode:

| Mode | Files linked (in order) |
|------|------------------------|
| **Hosted** | `common/types` → `common/memory_allocator` → `common/string_utils` → `hosted/runtime` |
| **Freestanding** | `common/types` → `common/memory_allocator` → `common/string_utils` → `freestanding/runtime` |

`common/mem.hll` and `common/klog.hll` are not auto-linked — include them explicitly when writing bare-metal code that needs them.

## Bare-metal / kernel builds

A kernel binary typically links:

```
common/types
common/memory_allocator
common/string_utils
common/mem
freestanding/runtime      ← provides _kpanic / kpanic
freestanding/console      ← provides console_* via UART
common/klog               ← depends on console_*
kernel/runtime_kernel     ← provides _kernel_start / kmalloc
<your kernel object>      ← must define kmain
```

`hosted/runtime` must **not** be linked in a kernel build.

## console_* API

Both `freestanding/console.hll` and `hosted/runtime.hll` export the same surface:

| Function | Signature |
|----------|-----------|
| `console_putchar` | `(c: i32) -> ()` |
| `console_write` | `(str: u8*) -> ()` — null-terminated |
| `console_writeln` | `(str: u8*) -> ()` — null-terminated + newline |
| `console_print_int` | `(n: i64) -> ()` |
| `console_print_hex` | `(n: u64) -> ()` — prints `0x` prefix + 16 hex digits |

`klog.hll` depends on this API, so it works in both hosted and bare-metal builds.

## UART

The freestanding console targets the NS16550A UART at physical address `0x10000000` (QEMU `virt` machine default). Writing a byte to offset 0 (THR) transmits it immediately.

## Heap

The allocator owns a single 64 KB BSS buffer (`heap_buffer`). Allocation uses a bump pointer; `free` marks blocks in a linked list for reuse. The heap is not thread-safe and does not support resizing.
