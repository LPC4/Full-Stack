//! Machine window: secondary egui window for booting and observing kernel programs.
//!
//! The VM is ticked incrementally each UI frame (`maybe_tick`), keeping the interface
//! live. Key improvements over the original:
//!
//! - Repaint is rate-limited to ~60 fps via `request_repaint_after` (was unlimited).
//! - Layout is fixed-height so nothing jumps when booting starts.
//! - The stdin strip is always rendered (just disabled when idle).
//! - The log `LayoutJob` is rebuilt only when new UART output arrives.

use std::time::Duration;

use asm_to_binary::AssembledOutput;
use egui::{Color32, Frame, Margin, RichText, Stroke, Vec2};
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ── Palette ──────────────────────────────────────────────────────────────────

const TERM_BG: Color32 = Color32::from_rgb(7, 9, 12);
const TERM_TEXT: Color32 = Color32::from_rgb(185, 210, 185);
const TERM_OK: Color32 = Color32::from_rgb(72, 200, 100);
const TERM_WARN: Color32 = Color32::from_rgb(220, 178, 60);
const TERM_ERR: Color32 = Color32::from_rgb(230, 80, 80);
const TERM_DIM: Color32 = Color32::from_rgb(80, 100, 80);
const TERM_PANIC: Color32 = Color32::from_rgb(255, 60, 80);
const TERM_RUNNING: Color32 = Color32::from_rgb(80, 160, 220);
const TERM_BORDER: Color32 = Color32::from_rgb(25, 45, 25);
const TOOLBAR_BG: Color32 = Color32::from_rgb(10, 14, 18);

// ── Tuning ───────────────────────────────────────────────────────────────────

/// VM steps executed per UI frame while booting.
/// At 60 fps this gives ~3 M steps/sec — enough for a fast boot
/// while keeping each frame well under 16 ms.
const STEPS_PER_TICK: u64 = 50_000;
const MAX_STEPS: u64 = 10_000_000;
const FB_COLS: usize = 80;
const FB_ROWS: usize = 25;

/// Fixed height of the top toolbar row (boot / stop / clear + status).
const TOOLBAR_H: f32 = 34.0;
/// Fixed height of the stdin strip at the bottom.
/// Always rendered so the content area never changes height when booting starts.
const INPUT_H: f32 = 34.0;

// ── Phase ────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct BootResult {
    pub uart_output: String,
    pub exit_code: Option<i64>,
    pub steps: u64,
    pub max_steps_reached: bool,
    pub fb_bytes: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum FbTab {
    #[default]
    BootLog,
    Framebuffer,
}

enum BootPhase {
    Idle,
    Running {
        vm: Box<VirtualMachine>,
        steps: u64,
        uart_text: String,
        /// Incremented every time new UART arrives; used to skip LayoutJob rebuilds.
        log_generation: u64,
    },
    Done(BootResult),
}

impl Default for BootPhase {
    fn default() -> Self {
        Self::Idle
    }
}

// ── Main struct ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct MachineWindow {
    pub open: bool,
    pub boot_requested: bool,
    phase: BootPhase,
    pub active_tab: FbTab,
    uart_input: String,
    /// Cached LayoutJob for the boot log; avoids rebuilding every frame.
    log_cache: Option<egui::text::LayoutJob>,
    log_cache_generation: u64,
}

// ── Public API ────────────────────────────────────────────────────────────────

impl MachineWindow {
    /// Begin an incremental boot. The VM is ticked each frame via `ui()`.
    pub fn start_boot(&mut self, assembled: &AssembledOutput) {
        self.phase = BootPhase::Running {
            vm: Box::new(VirtualMachine::new_kernel(assembled)),
            steps: 0,
            uart_text: String::new(),
            log_generation: 0,
        };
        self.active_tab = FbTab::BootLog;
        self.log_cache = None;
        self.log_cache_generation = 0;
    }

    /// Render the machine window contents. Call once per frame while the window is open.
    pub fn ui(&mut self, ui: &mut egui::Ui, has_kernel: bool) {
        self.maybe_tick(ui.ctx());

        let is_running = matches!(self.phase, BootPhase::Running { .. });

        self.render_toolbar(ui, has_kernel, is_running);

        // Fixed content height: subtracts the stdin strip unconditionally so
        // the frame never resizes when booting starts.
        let content_h = {
            let sp = ui.spacing().item_spacing.y;
            (ui.available_height() - INPUT_H - sp).max(100.0)
        };

        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, TERM_BORDER))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), content_h));
                ui.set_max_height(content_h);
                match self.active_tab {
                    FbTab::BootLog => self.render_boot_log(ui),
                    FbTab::Framebuffer => self.render_framebuffer(ui),
                }
            });

        // Always rendered so height is stable; disabled when not running.
        self.render_input(ui, is_running);
    }
}

// ── VM tick ───────────────────────────────────────────────────────────────────

impl MachineWindow {
    fn maybe_tick(&mut self, ctx: &egui::Context) {
        let transition = match &mut self.phase {
            BootPhase::Running { vm, steps, uart_text, log_generation } => {
                let mut halted: Option<i64> = None;
                let mut timed_out = false;

                for _ in 0..STEPS_PER_TICK {
                    if *steps >= MAX_STEPS {
                        timed_out = true;
                        break;
                    }
                    match vm.step() {
                        Ok(StepOutcome::Continue) => *steps += 1,
                        Ok(StepOutcome::Halted(code)) => {
                            *steps += 1;
                            halted = Some(code);
                            break;
                        }
                        Err(_) => {
                            *steps += 1;
                            halted = Some(-1);
                            break;
                        }
                    }
                }

                let new_bytes = vm.drain_uart_output();
                if !new_bytes.is_empty() {
                    uart_text.push_str(&String::from_utf8_lossy(&new_bytes));
                    *log_generation = log_generation.wrapping_add(1);
                }

                if halted.is_some() || timed_out {
                    let fb_bytes = vm.peek_bytes_raw(RAM_BASE, FB_COLS * FB_ROWS);
                    Some(BootResult {
                        uart_output: std::mem::take(uart_text),
                        exit_code: halted,
                        steps: *steps,
                        max_steps_reached: timed_out,
                        fb_bytes,
                    })
                } else {
                    // Rate-limit repaints to ~60 fps to avoid saturating the CPU.
                    ctx.request_repaint_after(Duration::from_millis(16));
                    None
                }
            }
            _ => return,
        };

        if let Some(result) = transition {
            self.phase = BootPhase::Done(result);
        }
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

impl MachineWindow {
    fn render_toolbar(&mut self, ui: &mut egui::Ui, has_kernel: bool, is_running: bool) {
        Frame::NONE
            .fill(TOOLBAR_BG)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_min_height(TOOLBAR_H);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    let boot_label = if is_running { "Booting…" } else { "Boot" };
                    if ui
                        .add_enabled(
                            has_kernel && !is_running,
                            egui::Button::new(boot_label).min_size(egui::vec2(58.0, 26.0)),
                        )
                        .on_disabled_hover_text(if is_running {
                            "Kernel is currently booting"
                        } else {
                            "Compile a Kernel-mode program first"
                        })
                        .on_hover_text("Boot the current kernel program")
                        .clicked()
                    {
                        self.open = true;
                        self.boot_requested = true;
                    }

                    if is_running
                        && ui
                            .add(egui::Button::new("Stop").min_size(egui::vec2(44.0, 26.0)))
                            .on_hover_text("Abort the running kernel")
                            .clicked()
                    {
                        self.do_stop();
                    }

                    if ui
                        .add(egui::Button::new("Clear").min_size(egui::vec2(44.0, 26.0)))
                        .clicked()
                    {
                        self.phase = BootPhase::Idle;
                        self.log_cache = None;
                    }

                    // Status strip
                    if !matches!(self.phase, BootPhase::Idle) {
                        ui.separator();
                        let (label, col, steps, exit_code) = self.status_info();
                        ui.colored_label(col, RichText::new(label).strong().monospace().size(11.0));
                        if let Some(code) = exit_code {
                            ui.colored_label(
                                TERM_DIM,
                                RichText::new(format!("exit:{code}")).monospace().size(11.0),
                            );
                        }
                        ui.colored_label(
                            TERM_DIM,
                            RichText::new(format!("{steps} steps")).monospace().size(11.0),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.active_tab, FbTab::Framebuffer, "FB");
                        ui.selectable_value(&mut self.active_tab, FbTab::BootLog, "Log");
                    });
                });
            });
    }

    fn status_info(&self) -> (&'static str, Color32, u64, Option<i64>) {
        match &self.phase {
            BootPhase::Idle => ("IDLE", TERM_DIM, 0, None),
            BootPhase::Running { steps, .. } => ("RUNNING", TERM_RUNNING, *steps, None),
            BootPhase::Done(r) if r.max_steps_reached => ("TIMEOUT", TERM_WARN, r.steps, r.exit_code),
            BootPhase::Done(r) if r.exit_code == Some(0) => ("OK", TERM_OK, r.steps, r.exit_code),
            BootPhase::Done(r) if r.exit_code.is_some() => ("ERR", TERM_ERR, r.steps, r.exit_code),
            BootPhase::Done(r) => ("HALTED", TERM_DIM, r.steps, r.exit_code),
        }
    }

    fn do_stop(&mut self) {
        let result = match &mut self.phase {
            BootPhase::Running { vm, steps, uart_text, .. } => {
                let fb_bytes = vm.peek_bytes_raw(RAM_BASE, FB_COLS * FB_ROWS);
                Some(BootResult {
                    uart_output: std::mem::take(uart_text),
                    exit_code: None,
                    steps: *steps,
                    max_steps_reached: false,
                    fb_bytes,
                })
            }
            _ => None,
        };
        if let Some(r) = result {
            self.phase = BootPhase::Done(r);
        }
    }

    fn render_boot_log(&mut self, ui: &mut egui::Ui) {
        // Extract what we need to decide rendering without a long borrow.
        enum LogState<'a> {
            Idle,
            BootingNoOutput(u64),
            HasText { text: &'a str, generation: u64 },
            DoneEmpty,
        }

        let state = match &self.phase {
            BootPhase::Idle => LogState::Idle,
            BootPhase::Running { uart_text, steps, log_generation, .. } => {
                if uart_text.is_empty() {
                    LogState::BootingNoOutput(*steps)
                } else {
                    LogState::HasText { text: uart_text.as_str(), generation: *log_generation }
                }
            }
            BootPhase::Done(r) => {
                if r.uart_output.is_empty() {
                    LogState::DoneEmpty
                } else {
                    LogState::HasText { text: r.uart_output.as_str(), generation: u64::MAX }
                }
            }
        };

        egui::ScrollArea::vertical()
            .id_salt("mw_boot_log")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| match state {
                LogState::Idle => {
                    ui.colored_label(TERM_DIM, "Press Boot to compile and run the kernel.");
                }
                LogState::BootingNoOutput(steps) => {
                    ui.colored_label(TERM_DIM, format!("Booting… ({steps} steps)"));
                }
                LogState::DoneEmpty => {
                    ui.colored_label(TERM_DIM, "(no output)");
                }
                LogState::HasText { text, generation } => {
                    // Rebuild the LayoutJob only when new UART has arrived.
                    if self.log_cache.is_none() || self.log_cache_generation != generation {
                        self.log_cache = Some(build_log_job(text));
                        self.log_cache_generation = generation;
                    }
                    if let Some(job) = self.log_cache.clone() {
                        ui.label(job);
                    }
                }
            });
    }

    fn render_framebuffer(&self, ui: &mut egui::Ui) {
        let fb: Option<Vec<u8>> = match &self.phase {
            BootPhase::Idle => None,
            BootPhase::Running { vm, .. } => Some(vm.peek_bytes_raw(RAM_BASE, FB_COLS * FB_ROWS)),
            BootPhase::Done(r) => Some(r.fb_bytes.clone()),
        };

        match fb {
            None => {
                ui.colored_label(TERM_DIM, "Boot the kernel to see framebuffer contents.");
            }
            Some(ref bytes) if bytes.is_empty() || bytes.iter().all(|&b| b == 0) => {
                ui.colored_label(TERM_DIM, "(no framebuffer data)");
            }
            Some(bytes) => {
                let mut text = String::with_capacity(FB_COLS * FB_ROWS + FB_ROWS);
                for row in bytes.chunks(FB_COLS) {
                    for &b in row {
                        text.push(if b.is_ascii_graphic() || b == b' ' {
                            b as char
                        } else {
                            '.'
                        });
                    }
                    text.push('\n');
                }
                egui::ScrollArea::both()
                    .id_salt("mw_framebuffer")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).monospace().color(TERM_TEXT).size(12.0));
                    });
            }
        }
    }

    fn render_input(&mut self, ui: &mut egui::Ui, is_running: bool) {
        Frame::NONE
            .fill(TOOLBAR_BG)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_min_height(INPUT_H);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        if is_running { TERM_OK } else { TERM_DIM },
                        RichText::new("IN>").monospace().size(11.0),
                    );

                    let te = egui::TextEdit::singleline(&mut self.uart_input)
                        .id_salt("mw_stdin")
                        .desired_width(ui.available_width() - 58.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(if is_running {
                            "Enter to send"
                        } else {
                            "Start a boot first"
                        })
                        .interactive(is_running);
                    let resp = ui.add(te);

                    let send = ui
                        .add_enabled(
                            is_running,
                            egui::Button::new("Send").min_size(egui::vec2(50.0, 26.0)),
                        )
                        .clicked();
                    let enter = is_running
                        && resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if (send || enter) && is_running {
                        let mut line = std::mem::take(&mut self.uart_input);
                        if !line.ends_with('\n') {
                            line.push('\n');
                        }
                        if let BootPhase::Running { vm, .. } = &mut self.phase {
                            for b in line.bytes() {
                                vm.push_uart_rx(b);
                            }
                        }
                        resp.request_focus();
                    }
                });
            });
    }
}

// ── Log colorizer ─────────────────────────────────────────────────────────────

fn build_log_job(text: &str) -> egui::text::LayoutJob {
    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();

    for line in text.split('\n') {
        let (tag, tag_col, rest_col) = if line.starts_with("[  OK  ]") {
            (Some("[  OK  ]"), TERM_OK, TERM_TEXT)
        } else if line.starts_with("[ WARN ]") {
            (Some("[ WARN ]"), TERM_WARN, TERM_WARN)
        } else if line.starts_with("[ ERR  ]") {
            (Some("[ ERR  ]"), TERM_ERR, TERM_ERR)
        } else if line.starts_with("PANIC") || line.starts_with("panic") {
            (None, TERM_PANIC, TERM_PANIC)
        } else {
            (None, TERM_TEXT, TERM_TEXT)
        };

        let fmt = |col: Color32| egui::TextFormat {
            font_id: font.clone(),
            color: col,
            ..Default::default()
        };

        if let Some(t) = tag {
            job.append(t, 0.0, fmt(tag_col));
            job.append(&line[t.len()..], 0.0, fmt(rest_col));
        } else {
            job.append(line, 0.0, fmt(rest_col));
        }
        job.append("\n", 0.0, fmt(TERM_TEXT));
    }

    job
}
