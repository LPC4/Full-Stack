//! OS-level inspector.
//!
//! Reads the kernel scheduler/PCB state from guest memory and renders a read-only
//! process table. Knowledge of the PCB layout lives here, not in the VM crate,
//! which stays a generic RISC-V machine.

use asm_to_binary::AssembledOutput;
use eframe::egui::{self, Color32, RichText};
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::VirtualMachine;

// PCB field byte offsets (kernel/process.hll; OS spec 5.1). The PCB is 384 bytes;
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

// Bound list walks so a corrupted `next` pointer can never spin the render thread.
const MAX_WALK: usize = 64;

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
}

impl OsSymbols {
    /// Resolve the four scheduler globals, or `None` if the image lacks them
    /// (e.g. a non-kernel binary, so the panel simply hides itself).
    pub fn from_kernel(assembled: &AssembledOutput) -> Option<Self> {
        let pa = |n: &str| assembled.symbol_address(n).map(|off| RAM_BASE + off);
        Some(Self {
            current_process: pa("current_process")?,
            ready_queue_head: pa("ready_queue_head")?,
            zombie_head: pa("zombie_head")?,
            input_waiter: pa("input_waiter")?,
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

    procs
}

fn mono(text: impl Into<String>, col: Color32) -> RichText {
    RichText::new(text.into()).monospace().size(11.0).color(col)
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

    // Process table: one row per PCB with the fields the kernel already holds.
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
                ui.label(mono(format!("{}", p.pid), p.role.color()));
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
}
