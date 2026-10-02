# Reference tables

Unpublished comparison content retained as Markdown for future Guide pages.
Use native tables when only rows and columns convey the relationship.

## Allocator zoo

Different lifetime strategies over heap storage.

| Strategy | Allocation unit | Release |
| --- | --- | --- |
| Heap allocator | Variable-size block | Individual block |
| Arena | Bump within region | Whole arena |
| Pool | Fixed-size slot | Return slot |
| `Vec<T>` | Contiguous capacity | Owned buffer |

Vec is a growable container, not a separate memory device. Each strategy still needs backing storage.

## Input paths

Host keyboard / two device interfaces.

| Path | Kernel interface | Consumer |
| --- | --- | --- |
| UART RX bytes | readchar (100) | Shell / text input |
| Keyboard events | poll_key (108) | Graphical input |

readchar blocks for console bytes; poll_key returns -1 when no event is queued. They are different queues.

## Memory hierarchy

Data-memory hierarchy / realistic teaching latency preset.

| Storage | Default capacity | Hit/access cycles |
| --- | --- | --- |
| L1 / 2-way | 4 KiB | 4 |
| L2 / 8-way | 256 KiB | 12 |
| L3 / 16-way | 8 MiB | 40 |
| DRAM | 128 MiB | 200 |

A miss proceeds to the next level. The default disabled latency mode charges 1 cycle; this preset is illustrative.

## Register file

32 integer registers / each register is 64 bits.

| Registers | ABI names | Role |
| --- | --- | --- |
| x0 | zero | Always reads zero |
| x1 / x2 | ra / sp | Return / stack |
| x3 / x4 | gp / tp | Global / thread |
| x5 to x7, x28 to x31 | t0 to t6 | Temporaries |
| x8 to x9, x18 to x27 | s0 to s11 | Callee-saved |
| x10 to x17 | a0 to a7 | Arguments / results |

s0 is also fp when a frame pointer is used. Only x0 has a hardware-enforced role.
