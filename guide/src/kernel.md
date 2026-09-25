The kernel runs in supervisor mode. Its modules separate hardware access, memory ownership,
files, tasks, and syscall handling. Imports point toward the lower-level owner of each operation.
These listings include the actual source files, so the examples stay synchronized with the kernel.

## Boot and traps

### Ordered initialization

`kmain` initializes the subsystems, creates the first user task, mounts the filesystem, and enters
the idle loop. The boot helpers name each step while the injected-image records describe the
host-to-kernel boundary.

```hll
{{#include crates/os-runtime/kernel/core/my_kernel.hll}}
```

### Trap dispatch

The assembly entry saves registers in a `TrapFrame`. This dispatcher routes interrupts and
syscalls, then leaves register restoration to the entry boundary.

```hll
{{#include crates/os-runtime/kernel/trap/trap_handler.hll}}
```

## Syscalls and tasks

### Dispatch and tracing

The syscall facade records the request, dispatches by syscall number, and publishes the result
to the trace ring. Handler modules own validation and translate failures to the syscall ABI.

```hll
{{#include crates/os-runtime/kernel/syscall/syscalls.hll}}
```

### File requests

File handlers validate user paths before calling the filesystem facade. User pointers stay at
this boundary; filesystem path operations receive bounded kernel slices.

```hll
{{#include crates/os-runtime/kernel/syscall/file_syscalls.hll}}
```

### Task lifecycle

The task facade coordinates process construction, scheduling, and exit cleanup. Queue mechanics
live in the scheduler, which does not depend on syscall handlers.

```hll
{{#include crates/os-runtime/kernel/sched/tasks.hll}}
```

### Round-robin scheduling

One private state record owns the running task, ready queue, zombies, and input waiter.
Switching tasks saves the current frame, activates the next address space, and restores its frame.

```hll
{{#include crates/os-runtime/kernel/sched/scheduler.hll}}
```

## Memory and files

### Explicit address spaces

Mapping operations take a root address explicitly. Constructing a child's mappings therefore
does not change the parent's active address space. Only activation writes the hardware `satp`.

```hll
{{#include crates/os-runtime/kernel/mm/address_space.hll}}
```

### Mounted filesystem

The filesystem facade owns the mounted image and descriptor-table state. Internal modules handle
disk records, allocation, directories, and descriptor operations. The facade translates their
typed failures to the existing syscall-facing return values.

```hll
{{#include crates/os-runtime/kernel/fs/files.hll}}
```
