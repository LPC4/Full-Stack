//! OS-level inspector.
//!
//! Reads the kernel scheduler/PCB state from guest memory and renders a read-only
//! process table. Knowledge of the PCB layout lives here, not in the VM crate,
//! which stays a generic RISC-V machine.

use asm_to_binary::AssembledOutput;
use eframe::egui::{self, Color32, RichText};
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

// Syscall trace ring layout (kernel/core/syscall.hll). Must match the HLL constants.
const TRACE_ENTRIES: usize = 256;
const TRACE_FIELDS: usize = 8;
const TRACE_F_SEQ: u64 = 0;
const TRACE_F_PID: u64 = 1;
const TRACE_F_NUM: u64 = 2;
const TRACE_F_A0: u64 = 3;
const TRACE_F_RET: u64 = 7;

// How many of the most recent ecalls the panel shows (newest first).
const MAX_TRACE_ROWS: usize = 64;

/// Guest-physical addresses of the kernel scheduler globals.
///
/// Resolved once at boot from the linked kernel symbol table. HLL globals keep
/// their names through the pipeline, and kernel data is identity-mapped, so
/// PA = `RAM_BASE + offset`.
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
    /// (e.g. a non-kernel binary, so the panel simply hides itself).
    pub fn from_kernel(assembled: &AssembledOutput) -> Option<Self> {
        let pa = |n: &str| assembled.symbol_address(n).map(|off| RAM_BASE + off);
        Some(Self {
            current_process: pa("current_process")?,
            ready_queue_head: pa("ready_queue_head")?,
            zombie_head: pa("zombie_head")?,
            input_waiter: pa("input_waiter")?,
            trace_ring: pa("trace_ring")?,
            trace_seq: pa("trace_seq")?,
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

impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "RUN ",
            Self::Ready => "RDY ",
            Self::Zombie => "ZOMB",
            Self::Waiting => "WAIT",
        }
    }

    fn color(self) -> Color32 {
        match self {
            Self::Running => Color32::from_rgb(120, 220, 120),
            Self::Ready => Color32::from_rgb(150, 190, 255),
            Self::Zombie => Color32::from_rgb(200, 120, 120),
            Self::Waiting => Color32::from_rgb(220, 200, 120),
        }
    }
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

fn state_name(state: u64) -> &'static str {
    match state {
        0 => "READY",
        1 => "RUNNING",
        2 => "BLOCKED",
        3 => "EXITED",
        _ => "?",
    }
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

    // The running process's saved trap frame only updates when it traps, so the
    // stack view would look frozen between syscalls. Prefer the live register
    // file (x2/x8) for it, falling back to the saved frame when the CPU is in a
    // kernel trap (live sp on the kernel stack, outside the user window).
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

/// Translate a user VA to a physical address by walking the Sv39 page table at
/// `root_pa` in software (read-only, via `peek_bytes_raw`). The MMU's own
/// translate is TLB-bound to the *current* satp, so it cannot resolve a
/// non-running process's address space; this walk works for any root. Returns
/// `None` for an unmapped or invalid VA. Layout knowledge stays out of the VM.
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
    pub ret: u64,
}

/// Human name for a syscall number (kernel/core/syscall.hll SYSCALL_* constants).
fn syscall_name(num: u64) -> &'static str {
    match num {
        2 => "yield",
        46 => "ftruncate",
        56 => "open",
        57 => "close",
        62 => "lseek",
        63 => "read",
        64 => "write",
        82 => "rename",
        83 => "mkdir",
        93 => "exit",
        100 => "readchar",
        101 => "readdir",
        102 => "stat",
        103 => "exec",
        104 => "pidalive",
        105 => "unlink",
        106 => "rmdir",
        107 => "mapfb",
        108 => "pollkey",
        110 => "exec_redir",
        129 => "kill",
        214 => "brk",
        220 => "fork",
        260 => "wait",
        261 => "waitpid",
        _ => "?",
    }
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
            ret: field(slot, TRACE_F_RET),
        });
        seq -= 1;
    }
    out
}

/// Render the syscall trace panel: the most recent ecalls newest-first, with an
/// optional per-pid filter and error returns highlighted. Read-only.
pub fn render_trace(ui: &mut egui::Ui, vm: &VirtualMachine, sym: &OsSymbols) {
    let entries = capture_trace(vm, sym);
    if entries.is_empty() {
        ui.label(mono("no syscalls traced yet", Color32::GRAY));
        return;
    }

    // Pid filter: 0 (default) shows every process; otherwise only that pid.
    let filter_id = ui.make_persistent_id("mw_trace_pid_filter");
    let mut filter = ui.data(|d| d.get_temp::<u64>(filter_id)).unwrap_or(0);
    ui.horizontal_wrapped(|ui| {
        ui.label(mono("pid filter:", Color32::from_rgb(160, 160, 160)));
        if ui
            .selectable_label(filter == 0, mono("all", Color32::LIGHT_GRAY))
            .clicked()
        {
            filter = 0;
        }
        let mut pids: Vec<u64> = entries.iter().map(|e| e.pid).collect();
        pids.sort_unstable();
        pids.dedup();
        for pid in pids {
            let txt = mono(format!("{pid}"), Color32::LIGHT_GRAY);
            if ui.selectable_label(filter == pid, txt).clicked() {
                filter = pid;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(filter_id, filter));
    ui.add_space(4.0);

    let head = Color32::from_rgb(160, 160, 160);
    egui::Grid::new("mw_syscall_trace")
        .num_columns(5)
        .spacing([12.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            for h in ["seq", "pid", "syscall", "a0", "ret"] {
                ui.label(mono(h, head));
            }
            ui.end_row();

            for e in entries.iter().filter(|e| filter == 0 || e.pid == filter) {
                let ret = e.ret as i64;
                let ret_col = if ret < 0 {
                    Color32::from_rgb(220, 120, 120)
                } else {
                    Color32::LIGHT_GRAY
                };
                ui.label(mono(format!("{}", e.seq), Color32::from_rgb(120, 120, 120)));
                ui.label(mono(format!("{}", e.pid), Color32::LIGHT_GRAY));
                ui.label(mono(
                    format!("{} ({})", syscall_name(e.num), e.num),
                    Color32::from_rgb(150, 190, 255),
                ));
                ui.label(mono(format!("{:#x}", e.a0), Color32::LIGHT_GRAY));
                if ret < 0 {
                    ui.label(mono(format!("{ret}"), ret_col));
                } else {
                    ui.label(mono(format!("{:#x}", e.ret), ret_col));
                }
                ui.end_row();
            }
        });
}

fn mono(text: impl Into<String>, col: Color32) -> RichText {
    RichText::new(text.into()).monospace().size(11.0).color(col)
}

/// Render the stack column for the selected process: one row per word from sp up
/// to the stack base, with sp and fp marked. Read-only.
fn render_stack(ui: &mut egui::Ui, vm: &VirtualMachine, p: &ProcessInfo) {
    let head = Color32::from_rgb(160, 160, 160);
    ui.label(mono(
        format!(
            "stack pid {}  sp {:#010x}  fp {:#010x}  base {:#010x}",
            p.pid, p.sp, p.fp, USER_STACK_BASE
        ),
        head,
    ));

    if p.page_root == 0 {
        ui.label(mono(
            "no address space (kernel-only process)",
            Color32::GRAY,
        ));
        return;
    }

    let words = capture_stack(vm, p);
    if words.is_empty() {
        ui.label(mono("stack empty (sp at base)", Color32::GRAY));
        return;
    }

    let depth_bytes = USER_STACK_BASE.saturating_sub(p.sp);
    ui.label(mono(
        format!("{} words, {} bytes deep", words.len(), depth_bytes),
        Color32::from_rgb(120, 120, 120),
    ));

    // Render as a plain grid; the Debug tab's outer scroll area owns scrolling, so
    // we must not nest a second vertical ScrollArea here (it jitters while running).
    egui::Grid::new("mw_stack_words")
        .num_columns(3)
        .spacing([12.0, 1.0])
        .striped(true)
        .show(ui, |ui| {
            for w in &words {
                let marker = if w.va == p.sp {
                    "<- sp"
                } else if w.va == p.fp {
                    "<- fp"
                } else {
                    ""
                };
                let marker_col = if w.va == p.sp {
                    Role::Running.color()
                } else {
                    Color32::from_rgb(220, 200, 120)
                };
                ui.label(mono(format!("{:#010x}", w.va), Color32::LIGHT_GRAY));
                if w.mapped {
                    ui.label(mono(format!("{:#018x}", w.value), Color32::LIGHT_GRAY));
                } else {
                    ui.label(mono("<unmapped>", Color32::DARK_GRAY));
                }
                ui.label(mono(marker, marker_col));
                ui.end_row();
            }
            if words.len() == MAX_STACK_WORDS {
                ui.label(mono("...", head));
                ui.label(mono("(truncated)", head));
                ui.label(mono("", head));
                ui.end_row();
            }
        });
}

/// Render the scheduler chip strip and the process table into the Debug tab.
pub fn render(ui: &mut egui::Ui, vm: &VirtualMachine, sym: &OsSymbols) {
    let procs = capture(vm, sym);

    if procs.is_empty() {
        ui.label(mono(
            "no live processes (kernel not yet scheduling)",
            Color32::GRAY,
        ));
        return;
    }

    // Chip strip: one colored pid chip per process, ordered as captured.
    ui.horizontal_wrapped(|ui| {
        for p in &procs {
            ui.label(mono(
                format!("[{} pid {}]", p.role.label(), p.pid),
                p.role.color(),
            ));
        }
    });
    ui.add_space(4.0);

    // Selected pid persists across frames; default to the running process so the
    // stack view shows something useful on first open. Clamp to a live pid.
    let sel_id = ui.make_persistent_id("mw_os_selected_pid");
    let mut selected = ui.data(|d| d.get_temp::<u64>(sel_id)).unwrap_or(0);
    if !procs.iter().any(|p| p.pid == selected) {
        selected = procs.first().map(|p| p.pid).unwrap_or(0);
    }

    // Process table: one row per PCB with the fields the kernel already holds.
    // The pid cell is clickable and selects the process for the stack view.
    egui::Grid::new("mw_proc_table")
        .num_columns(8)
        .spacing([12.0, 2.0])
        .striped(true)
        .show(ui, |ui| {
            let head = Color32::from_rgb(160, 160, 160);
            for h in [
                "pid", "role", "state", "parent", "exit", "out/in", "root", "fb",
            ] {
                ui.label(mono(h, head));
            }
            ui.end_row();

            for p in &procs {
                let pid_text = mono(format!("{}", p.pid), p.role.color());
                if ui.selectable_label(p.pid == selected, pid_text).clicked() {
                    selected = p.pid;
                }
                ui.label(mono(p.role.label(), p.role.color()));
                ui.label(mono(state_name(p.state), Color32::LIGHT_GRAY));
                ui.label(mono(format!("{}", p.parent), Color32::LIGHT_GRAY));
                ui.label(mono(format!("{}", p.exit_code), Color32::LIGHT_GRAY));
                ui.label(mono(
                    format!("{}/{}", p.stdout_fd, p.stdin_fd),
                    Color32::LIGHT_GRAY,
                ));
                ui.label(mono(format!("{:#x}", p.page_root), Color32::LIGHT_GRAY));
                ui.label(mono(
                    if p.fb_mapped { "yes" } else { "-" },
                    Color32::LIGHT_GRAY,
                ));
                ui.end_row();
            }
        });

    ui.data_mut(|d| d.insert_temp(sel_id, selected));

    // Stack view for the selected process.
    if let Some(p) = procs.iter().find(|p| p.pid == selected) {
        ui.add_space(6.0);
        render_stack(ui, vm, p);
    }
}
