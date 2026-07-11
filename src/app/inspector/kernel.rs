//! Kernel-only panes, driven by symbol probes into the guest: the scheduler's
//! process table, the guest filesystem tree, and the syscall trace ring.

use super::FullStackApp;
use super::widgets::{capture_due, human_size, kv_line, mono_row};
use crate::app::{chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::inspect::{fs_view, os_view};
use full_stack::session::MachineState;

/// Preview cap for guest FS files, in bytes.
const FS_PREVIEW_CAP: usize = 4096;

/// Throttled snapshot of the kernel probes, refreshed as the machine steps.
pub(crate) struct InspectCache {
    steps: u64,
    time: f64,
    processes: Vec<os_view::ProcessInfo>,
    fs_root: Option<fs_view::FsNode>,
    trace: Vec<os_view::TraceEntry>,
}

impl InspectCache {
    /// Live process count, shown in the machine dock for kernel runs.
    pub(crate) fn process_count(&self) -> usize {
        self.processes.len()
    }
}

impl FullStackApp {
    /// Re-run the kernel probes when the machine has stepped since the last
    /// capture: immediately while paused, throttled while running.
    pub(in crate::app) fn refresh_inspect_cache(&mut self, now: f64) {
        let Some(live) = &self.live else {
            self.inspector.cache = None;
            return;
        };
        let due = capture_due(
            self.inspector.cache.as_ref().map(|c| (c.steps, c.time)),
            live.steps,
            live.state == MachineState::Running,
            now,
        );
        if !due {
            return;
        }
        let vm = live.vm();
        self.inspector.cache = Some(InspectCache {
            steps: live.steps,
            time: now,
            processes: live
                .os_symbols()
                .map(|sym| os_view::capture(vm, sym))
                .unwrap_or_default(),
            fs_root: live.fs_symbols().and_then(|sym| fs_view::capture(vm, sym)),
            trace: live
                .os_symbols()
                .map(|sym| os_view::capture_trace(vm, sym))
                .unwrap_or_default(),
        });
    }

    /// Scheduler snapshot: one row per live PCB; selecting one unfolds its
    /// details and a stack walk through that process's own page table.
    pub(super) fn processes_pane(&mut self, ui: &mut egui::Ui) {
        let Some(cache) = &self.inspector.cache else {
            chrome::centered_hint(ui, "No capture yet.");
            return;
        };
        if cache.processes.is_empty() {
            chrome::centered_hint(ui, "No live processes.");
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("proc_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for process in &cache.processes {
                    let selected = self.inspector.pid == Some(process.pid);
                    if process_row(ui, process, selected).clicked() {
                        clicked = Some(process.pid);
                    }
                    if selected {
                        self.process_detail(ui, process);
                    }
                }
            });
        if let Some(pid) = clicked {
            self.inspector.pid = (self.inspector.pid != Some(pid)).then_some(pid);
        }
    }

    fn process_detail(&self, ui: &mut egui::Ui, process: &os_view::ProcessInfo) {
        chrome::indented(ui, |ui| {
            ui.add_space(metrics::SECTION_GAP);
            kv_line(ui, "parent", &process.parent.to_string());
            kv_line(ui, "pcb", &format!("{:#x}", process.pcb_pa));
            kv_line(ui, "page root", &format!("{:#x}", process.page_root));
            kv_line(ui, "sp", &format!("{:#x}", process.sp));
            if process.role == os_view::Role::Zombie {
                kv_line(ui, "exit", &process.exit_code.to_string());
            }
            if process.fb_mapped {
                kv_line(ui, "framebuffer", "mapped");
            }
            ui.add_space(metrics::SECTION_GAP);
        });
        let Some(live) = &self.live else {
            return;
        };
        let stack = os_view::capture_stack(live.vm(), process);
        if stack.is_empty() {
            return;
        }
        chrome::indented(ui, |ui| {
            ui.label(theme::label("stack"));
        });
        for word in stack.iter().take(16) {
            let text = if word.mapped {
                format!("{:#012x}  {:016x}", word.va, word.value)
            } else {
                format!("{:#012x}  (unmapped)", word.va)
            };
            let color = if word.mapped {
                theme::TEXT_DIM
            } else {
                theme::TEXT_FAINT
            };
            mono_row(ui, &text, color);
        }
        ui.add_space(metrics::SECTION_GAP);
    }

    /// The guest filesystem tree; selecting a file shows its head as text.
    pub(super) fn guest_fs_pane(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self
            .inspector
            .cache
            .as_ref()
            .and_then(|c| c.fs_root.as_ref())
        else {
            chrome::centered_hint(ui, "Guest FS is not readable yet.");
            return;
        };
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("guest_fs_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                fs_node_rows(ui, root, 0, self.inspector.inode, &mut clicked);
                if let Some(inode) = self.inspector.inode {
                    self.fs_preview(ui, inode);
                }
            });
        if let Some(inode) = clicked {
            self.inspector.inode = (self.inspector.inode != Some(inode)).then_some(inode);
        }
    }

    fn fs_preview(&self, ui: &mut egui::Ui, inode: u64) {
        let Some(live) = &self.live else {
            return;
        };
        let (Some(sym), Some(root)) = (
            live.fs_symbols(),
            self.inspector
                .cache
                .as_ref()
                .and_then(|c| c.fs_root.as_ref()),
        ) else {
            return;
        };
        let Some(node) = find_fs_node(root, inode) else {
            return;
        };
        let bytes = fs_view::file_preview(live.vm(), sym, node);
        ui.add_space(metrics::SECTION_GAP);
        chrome::hairline(ui);
        chrome::indented(ui, |ui| {
            ui.add_space(metrics::SECTION_GAP);
            ui.label(theme::label(&node.name));
            let shown = &bytes[..bytes.len().min(FS_PREVIEW_CAP)];
            let text = match std::str::from_utf8(shown) {
                Ok(text) => text.to_owned(),
                Err(_) => format!("(binary, {} bytes)", node.size),
            };
            ui.label(theme::mono(
                text,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
            if bytes.len() > FS_PREVIEW_CAP {
                ui.label(theme::mono(
                    format!(
                        "... {} more",
                        crate::app::diagnostics::tally(
                            (bytes.len() - FS_PREVIEW_CAP) as u64,
                            "byte"
                        )
                    ),
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            }
        });
    }

    /// The syscall trace ring, newest first.
    pub(super) fn trace_pane(&self, ui: &mut egui::Ui) {
        let Some(cache) = &self.inspector.cache else {
            chrome::centered_hint(ui, "No capture yet.");
            return;
        };
        if cache.trace.is_empty() {
            chrome::centered_hint(ui, "No syscalls traced yet.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("trace_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in &cache.trace {
                    trace_row(ui, entry);
                }
            });
    }
}

fn process_row(
    ui: &mut egui::Ui,
    process: &os_view::ProcessInfo,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        format!("pid {}", process.pid),
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        if selected {
            theme::ELECTRIC
        } else {
            theme::TEXT
        },
    );
    let (role, color) = role_style(process.role);
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        role,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        color,
    );
    resp
}

fn role_style(role: os_view::Role) -> (&'static str, egui::Color32) {
    match role {
        os_view::Role::Running => ("running", theme::ELECTRIC),
        os_view::Role::Ready => ("ready", theme::TEXT_DIM),
        os_view::Role::Waiting => ("waiting", theme::WARNING),
        os_view::Role::Zombie => ("zombie", theme::TEXT_FAINT),
    }
}

fn fs_node_rows(
    ui: &mut egui::Ui,
    node: &fs_view::FsNode,
    depth: usize,
    selected: Option<u64>,
    clicked: &mut Option<u64>,
) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    let is_selected = !node.is_dir && selected == Some(node.inode);
    if is_selected {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
    } else if resp.hovered() && !node.is_dir {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let indent = metrics::PANEL_INSET_X + depth as f32 * metrics::TREE_INDENT;
    let (label, color) = if node.is_dir {
        (format!("{}/", node.name), theme::TEXT_DIM)
    } else {
        (
            node.name.clone(),
            if is_selected {
                theme::ELECTRIC
            } else {
                theme::TEXT
            },
        )
    };
    painter.text(
        pos2(rect.left() + indent, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    if !node.is_dir {
        let mut trailer = human_size(node.size);
        if node.open_fds > 0 {
            trailer = format!("{} fd:{}", trailer, node.open_fds);
        }
        painter.text(
            pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
            Align2::RIGHT_CENTER,
            trailer,
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            theme::TEXT_FAINT,
        );
        if resp.clicked() {
            *clicked = Some(node.inode);
        }
    }
    for child in &node.children {
        fs_node_rows(ui, child, depth + 1, selected, clicked);
    }
}

fn find_fs_node(node: &fs_view::FsNode, inode: u64) -> Option<&fs_view::FsNode> {
    if node.inode == inode && !node.is_dir {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_fs_node(child, inode))
}

fn trace_row(ui: &mut egui::Ui, entry: &os_view::TraceEntry) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{} p{} {}", entry.seq, entry.pid, syscall_name(entry.num)),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT,
    );
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{:x} -> {:x}", entry.a0, entry.ret),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
    resp.on_hover_text(format!(
        "seq {}  pid {}  {} ({})\na0  {:#x}\nret {:#x}",
        entry.seq,
        entry.pid,
        syscall_name(entry.num),
        entry.num,
        entry.a0,
        entry.ret
    ));
}

/// Names for the kernel's syscall numbers (core/syscall.hll); raw number otherwise.
fn syscall_name(num: u64) -> String {
    let name = match num {
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
        _ => return format!("sys{num}"),
    };
    name.to_owned()
}
