//! OS-level inspector: captures kernel scheduler/PCB state from guest memory into
//! plain data structs. PCB layout knowledge lives here; rendering is the UI's job.

use asm_to_binary::AssembledOutput;
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::VirtualMachine;

// PCB field byte offsets (kernel/sched/process.hll; OS spec 5.1). The PCB is 384 bytes;
// the ready queue and zombie list both link through `next` at offset 16.
const PCB_OFF_PID: u64 = 0;
const PCB_OFF_STATE: u64 = 8;
const PCB_OFF_NEXT: u64 = 16;
const PCB_OFF_PAGE_ROOT: u64 = 328;
const PCB_OFF_PARENT_PID: u64 = 336;
const PCB_OFF_EXIT_CODE: u64 = 344;
const PCB_OFF_STDOUT_FD: u64 = 352;
const PCB_OFF_STDIN_FD: u64 = 360;
const PCB_OFF_FB_MAPPED: u64 = 368;

// Saved trap-frame registers (process.hll: frame at off 40 = x0; off 40 + n*8 = xn).
// We read the user sp (x2) and fp/s0 (x8) to walk the process's stack.
const PCB_OFF_TF_SP: u64 = 40 + 2 * 8; // x2
const PCB_OFF_TF_FP: u64 = 40 + 8 * 8; // x8

// Canonical user-stack top, shared by all processes (process.hll USER_STACK_BASE).
// The stack grows down from here, so live data sits in [sp, USER_STACK_BASE).
const USER_STACK_BASE: u64 = 0x8000_0000;

// Stack window size: process_create maps 4 pages (16 KiB) below the base. Used to
// tell a live user sp from a kernel sp when reading the running process's frame.
const USER_STACK_SPAN: u64 = 4 * 0x1000;

// Bound list walks so a corrupted `next` pointer can never spin the render thread.
const MAX_WALK: usize = 64;

// Bound the stack walk: a full 4-page (16 KiB) stack is 2048 words; cap the render.
const MAX_STACK_WORDS: usize = 256;

// Syscall trace ring layout (kernel/syscall/syscall_trace.hll). Must match the HLL constants.
const TRACE_ENTRIES: usize = 256;
const TRACE_FIELDS: usize = 10;
const TRACE_F_SEQ: u64 = 0;
const TRACE_F_PID: u64 = 1;
const TRACE_F_NUM: u64 = 2;
const TRACE_F_A0: u64 = 3;
const TRACE_F_RET: u64 = 9;

// How many of the most recent ecalls the panel shows (newest first).
const MAX_TRACE_ROWS: usize = 64;

/// Guest-physical addresses of the kernel scheduler globals. Kernel data is
/// identity-mapped, so PA = `RAM_BASE + symbol offset`.
#[derive(Clone, Copy)]
pub struct OsSymbols {
    current_process: u64,
    ready_queue_head: u64,
    zombie_head: u64,
    input_waiter: u64,
    trace_ring: u64,
    trace_seq: u64,
}

impl OsSymbols {
    /// Resolve the scheduler globals, or `None` if the image lacks them
    /// (e.g. a non-kernel binary, where the panel hides itself).
    pub fn from_kernel(assembled: &AssembledOutput) -> Option<Self> {
        let pa = |n: &str| assembled.symbol_address(n).map(|off| RAM_BASE + off);
        let scheduler_state = pa("scheduler__scheduler_state")?;
        Some(Self {
            current_process: scheduler_state,
            ready_queue_head: scheduler_state + 8,
            zombie_head: scheduler_state + 16,
            input_waiter: scheduler_state + 24,
            trace_ring: pa("syscall_trace__trace_buffer")? + 8,
            trace_seq: pa("syscall_trace__trace_buffer")?,
        })
    }
}

/// Where a process sits in the scheduler at snapshot time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Running,
    Ready,
    Zombie,
    Waiting,
}

/// A single process control block, decoded for display.
pub struct ProcessInfo {
    pub role: Role,
    pub pcb_pa: u64,
    pub pid: u64,
    pub state: u64,
    pub parent: u64,
    pub exit_code: i64,
    pub stdout_fd: u64,
    pub stdin_fd: u64,
    pub page_root: u64,
    pub fb_mapped: bool,
    pub sp: u64,
    pub fp: u64,
}

/// One word of a process stack, with its virtual address and whether the page
/// backing it is mapped in the process's address space.
pub struct StackWord {
    pub va: u64,
    pub value: u64,
    pub mapped: bool,
}

/// Walk the scheduler from guest memory and decode every live PCB. Read-only:
/// uses `peek_bytes_raw`, so it never perturbs cache state or the run loop.
pub fn capture(vm: &VirtualMachine, sym: &OsSymbols) -> Vec<ProcessInfo> {
    let rd = |pa: u64| -> u64 {
        let bytes = vm.peek_bytes_raw(pa, 8);
        u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8]))
    };
    let read_pcb = |pa: u64, role: Role| ProcessInfo {
        role,
        pcb_pa: pa,
        pid: rd(pa + PCB_OFF_PID),
        state: rd(pa + PCB_OFF_STATE),
        parent: rd(pa + PCB_OFF_PARENT_PID),
        exit_code: i64::from_le_bytes(rd(pa + PCB_OFF_EXIT_CODE).to_le_bytes()),
        stdout_fd: rd(pa + PCB_OFF_STDOUT_FD),
        stdin_fd: rd(pa + PCB_OFF_STDIN_FD),
        page_root: rd(pa + PCB_OFF_PAGE_ROOT),
        fb_mapped: rd(pa + PCB_OFF_FB_MAPPED) != 0,
        sp: rd(pa + PCB_OFF_TF_SP),
        fp: rd(pa + PCB_OFF_TF_FP),
    };

    let mut procs = Vec::new();
    let current = rd(sym.current_process);
    if current != 0 {
        procs.push(read_pcb(current, Role::Running));
    }

    // The running PCB is popped off the ready queue, so it should not reappear;
    // the `!= current` guard is belt-and-suspenders against a transient overlap.
    let mut node = rd(sym.ready_queue_head);
    let mut walked = 0;
    while node != 0 && walked < MAX_WALK {
        if node != current {
            procs.push(read_pcb(node, Role::Ready));
        }
        node = rd(node + PCB_OFF_NEXT);
        walked += 1;
    }

    let mut zomb = rd(sym.zombie_head);
    walked = 0;
    while zomb != 0 && walked < MAX_WALK {
        procs.push(read_pcb(zomb, Role::Zombie));
        zomb = rd(zomb + PCB_OFF_NEXT);
        walked += 1;
    }

    let waiter = rd(sym.input_waiter);
    if waiter != 0 && waiter != current {
        procs.push(read_pcb(waiter, Role::Waiting));
    }

    // The saved trap frame only updates on traps, freezing the stack view between
    // syscalls; prefer live x2/x8, fall back to the frame during kernel traps.
    if let Some(running) = procs.first_mut()
        && running.role == Role::Running
    {
        let in_user_stack = |v: u64| v < USER_STACK_BASE && v >= USER_STACK_BASE - USER_STACK_SPAN;
        let live_sp = vm.peek_reg(2);
        if in_user_stack(live_sp) {
            running.sp = live_sp;
            let live_fp = vm.peek_reg(8);
            if in_user_stack(live_fp) {
                running.fp = live_fp;
            }
        }
    }

    procs
}

// Software Sv39 walk at `root_pa`: the MMU's own translate is TLB-bound to the
// current satp, so it cannot resolve a non-running process's address space.
fn translate_sv39(vm: &VirtualMachine, root_pa: u64, va: u64) -> Option<u64> {
    let rd = |pa: u64| -> u64 {
        let bytes = vm.peek_bytes_raw(pa, 8);
        u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8]))
    };
    const PPN_MASK: u64 = (1u64 << 44) - 1;
    let mut table = root_pa;
    // Three levels: 2, 1, 0. Each contributes 9 VPN bits; offset is bits 11:0.
    for level in (0..3).rev() {
        let shift = 12 + 9 * level;
        let vpn = (va >> shift) & 0x1ff;
        let pte = rd(table + vpn * 8);
        if pte & 1 == 0 {
            return None; // PTE invalid.
        }
        let readable = (pte >> 1) & 1 != 0;
        let executable = (pte >> 3) & 1 != 0;
        let ppn = (pte >> 10) & PPN_MASK;
        if readable || executable {
            // Leaf: combine the page PPN with the in-page (or in-superpage) offset.
            let page_off_mask = (1u64 << shift) - 1;
            return Some((ppn << 12) | (va & page_off_mask));
        }
        table = ppn << 12; // Descend to the next level.
    }
    None
}

/// Walk the selected process's stack from `sp` up to `USER_STACK_BASE`, decoding
/// one word per slot. Each VA is translated through the process's own page table.
pub fn capture_stack(vm: &VirtualMachine, p: &ProcessInfo) -> Vec<StackWord> {
    let mut out = Vec::new();
    if p.sp == 0 || p.sp >= USER_STACK_BASE || !p.sp.is_multiple_of(8) {
        return out; // Empty or unusable stack pointer.
    }
    let mut va = p.sp;
    while va < USER_STACK_BASE && out.len() < MAX_STACK_WORDS {
        let (value, mapped) = match translate_sv39(vm, p.page_root, va) {
            Some(pa) => {
                let bytes = vm.peek_bytes_raw(pa, 8);
                (
                    u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8])),
                    true,
                )
            }
            None => (0, false),
        };
        out.push(StackWord { va, value, mapped });
        va += 8;
    }
    out
}

/// One decoded entry from the syscall trace ring.
pub struct TraceEntry {
    pub seq: u64,
    pub pid: u64,
    pub num: u64,
    pub a0: u64,
    pub a1: u64,
    pub a2: u64,
    pub a3: u64,
    pub a4: u64,
    pub a5: u64,
    pub ret: u64,
}

/// Drain the syscall trace ring from guest memory, returning the most recent
/// entries newest-first (at most `MAX_TRACE_ROWS`). Read-only via `peek_bytes_raw`.
pub fn capture_trace(vm: &VirtualMachine, sym: &OsSymbols) -> Vec<TraceEntry> {
    let rd = |pa: u64| -> u64 {
        let bytes = vm.peek_bytes_raw(pa, 8);
        u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8]))
    };
    let field = |slot: usize, f: u64| rd(sym.trace_ring + ((slot * TRACE_FIELDS) as u64 + f) * 8);

    let total = rd(sym.trace_seq);
    if total == 0 {
        return Vec::new();
    }

    // Walk seq backwards from the newest; the slot for seq s is (s - 1) % ENTRIES.
    let mut out = Vec::new();
    let mut seq = total;
    while seq > 0 && out.len() < MAX_TRACE_ROWS {
        let slot = ((seq - 1) % TRACE_ENTRIES as u64) as usize;
        // Stop if the ring has wrapped past this slot (it now holds a newer seq).
        if field(slot, TRACE_F_SEQ) != seq {
            break;
        }
        out.push(TraceEntry {
            seq,
            pid: field(slot, TRACE_F_PID),
            num: field(slot, TRACE_F_NUM),
            a0: field(slot, TRACE_F_A0),
            a1: field(slot, TRACE_F_A0 + 1),
            a2: field(slot, TRACE_F_A0 + 2),
            a3: field(slot, TRACE_F_A0 + 3),
            a4: field(slot, TRACE_F_A0 + 4),
            a5: field(slot, TRACE_F_A0 + 5),
            ret: field(slot, TRACE_F_RET),
        });
        seq -= 1;
    }
    out
}
