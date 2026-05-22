//! Machine window: secondary egui window for booting and observing kernel programs.
//!
//! Opened via the "Machine" button in the IDE top bar. Holds its own VM instance
//! and is independent of the IDE/Debug state. Renders live:
//!   - Boot log (UART output, colored by log level, streamed per-frame)
//!   - Framebuffer (text or pixel mode, polled each frame while running)
//!   - Boot status strip + stdin input widget

use asm_to_binary::AssembledOutput;
use egui::{Color32, Frame, RichText, Stroke};
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

const TERM_BG: Color32 = Color32::from_rgb(7, 9, 12);
const TERM_TEXT: Color32 = Color32::from_rgb(185, 210, 185);
const TERM_OK: Color32 = Color32::from_rgb(72, 200, 100);
const TERM_WARN: Color32 = Color32::from_rgb(220, 178, 60);
const TERM_ERR: Color32 = Color32::from_rgb(230, 80, 80);
const TERM_DIM: Color32 = Color32::from_rgb(100, 120, 100);
const TERM_PANIC: Color32 = Color32::from_rgb(255, 60, 80);
const TERM_RUNNING: Color32 = Color32::from_rgb(80, 160, 220);

// Steps executed per egui frame while the kernel is running.
// ~50 K steps/frame × 60 fps ≈ 3 M steps/s on the UI thread without stuttering.
const STEPS_PER_TICK: u64 = 50_000;
const MAX_STEPS: u64 = 10_000_000;
const FB_COLS: usize = 80;
const FB_ROWS: usize = 25;

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
    },
    Done(BootResult),
}

impl Default for BootPhase {
    fn default() -> Self {
        BootPhase::Idle
    }
}

#[derive(Default)]
pub struct MachineWindow {
    pub open: bool,
    pub boot_requested: bool,
    phase: BootPhase,
    pub active_tab: FbTab,
    uart_input: String,
}

impl MachineWindow {
    /// Begin an incremental boot of `assembled`. The VM runs in small
    /// batches each UI frame via `maybe_tick`, so the interface stays live.
    pub fn start_boot(&mut self, assembled: &AssembledOutput) {
        let vm = Box::new(VirtualMachine::new_kernel(assembled));
        self.phase = BootPhase::Running {
            vm,
            steps: 0,
            uart_text: String::new(),
        };
        self.active_tab = FbTab::BootLog;
    }

    /// Advance the running VM by one batch and drain UART output.
    /// Requests a repaint if the VM is still running so egui keeps calling us.
    fn maybe_tick(&mut self, ctx: &egui::Context) {
        // Extract a result only if the phase transitions to Done.
        let transition: Option<BootResult> = match &mut self.phase {
            BootPhase::Running { vm, steps, uart_text } => {
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

                // Drain UART output accumulated during this batch.
                let new_bytes = vm.drain_uart_output();
                if !new_bytes.is_empty() {
                    uart_text.push_str(&String::from_utf8_lossy(&new_bytes));
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
                    None
                }
            }
            _ => return,
        };

        if let Some(result) = transition {
            self.phase = BootPhase::Done(result);
        } else {
            // Still running, ask egui to call us again next frame.
            ctx.request_repaint();
        }
    }

    /// Render the machine window content into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui, has_kernel: bool) {
        // Advance the VM before rendering so this frame shows fresh output.
        self.maybe_tick(ui.ctx());

        let is_running = matches!(self.phase, BootPhase::Running { .. });

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            let boot_label = if is_running { "Booting…" } else { "Boot" };
            if ui
                .add_enabled(
                    has_kernel && !is_running,
                    egui::Button::new(boot_label).min_size(egui::vec2(60.0, 24.0)),
                )
                .on_disabled_hover_text(if is_running {
                    "Kernel is currently booting"
                } else {
                    "Compile a Kernel-mode program in the IDE first"
                })
                .on_hover_text("Boot the current kernel program")
                .clicked()
            {
                self.open = true;
                self.boot_requested = true;
            }

            // Stop button — only visible while running.
            if is_running {
                if ui
                    .add(
                        egui::Button::new("Stop").min_size(egui::vec2(60.0, 24.0)),
                    )
                    .on_hover_text("Abort the running kernel")
                    .clicked()
                {
                    // Snapshot current state then transition to Done.
                    let result = match &mut self.phase {
                        BootPhase::Running { vm, steps, uart_text } => {
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
            }

            if ui
                .add(egui::Button::new("Clear").min_size(egui::vec2(60.0, 24.0)))
                .clicked()
            {
                self.phase = BootPhase::Idle;
            }

            let status: Option<(&str, Color32, u64, Option<i64>)> = match &self.phase {
                BootPhase::Idle => None,
                BootPhase::Running { steps, .. } => {
                    Some(("RUNNING", TERM_RUNNING, *steps, None))
                }
                BootPhase::Done(r) if r.max_steps_reached => {
                    Some(("TIMEOUT", TERM_WARN, r.steps, r.exit_code))
                }
                BootPhase::Done(r) if r.exit_code == Some(0) => {
                    Some(("OK", TERM_OK, r.steps, r.exit_code))
                }
                BootPhase::Done(r) if r.exit_code.is_some() => {
                    Some(("ERR", TERM_ERR, r.steps, r.exit_code))
                }
                BootPhase::Done(r) => Some(("HALTED", TERM_DIM, r.steps, r.exit_code)),
            };

            if let Some((txt, col, steps, exit_code)) = status {
                ui.separator();
                ui.colored_label(col, RichText::new(txt).strong());
                if let Some(code) = exit_code {
                    ui.colored_label(TERM_DIM, format!("exit:{code}"));
                }
                ui.colored_label(TERM_DIM, format!("{steps} steps"));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut self.active_tab, FbTab::Framebuffer, "Framebuffer");
                ui.selectable_value(&mut self.active_tab, FbTab::BootLog, "Boot Log");
            });
        });

        ui.add_space(6.0);

        let available_height = ui.available_height();
        let content_height = if is_running {
            // Leave space for stdin input when running
            (available_height - 50.0).max(200.0)
        } else {
            available_height.max(200.0)
        };
        
        egui::Frame::NONE.show(ui, |ui| {
            ui.set_min_height(content_height);
            match self.active_tab {
                FbTab::BootLog => self.render_boot_log(ui),
                FbTab::Framebuffer => self.render_framebuffer(ui),
            }
        });

        if is_running {
            ui.add_space(6.0);
            self.render_input(ui);
        }
    }


    fn render_boot_log(&self, ui: &mut egui::Ui) {
        let margin = 8.0;
        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, Color32::from_rgb(30, 50, 30)))
            .inner_margin(margin)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("machine_boot_log")
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .show(ui, |ui| match &self.phase {
                        BootPhase::Idle => {
                            ui.colored_label(
                                TERM_DIM,
                                "Press Boot to compile and run the kernel.",
                            );
                        }
                        BootPhase::Running { uart_text, steps, .. } => {
                            if uart_text.is_empty() {
                                ui.colored_label(
                                    TERM_DIM,
                                    format!("Booting… ({steps} steps)"),
                                );
                            } else {
                                self.render_colored_log(ui, uart_text);
                            }
                        }
                        BootPhase::Done(r) if r.uart_output.is_empty() => {
                            ui.colored_label(TERM_DIM, "(no output)");
                        }
                        BootPhase::Done(r) => {
                            self.render_colored_log(ui, &r.uart_output);
                        }
                    });
            });
    }

    fn render_colored_log(&self, ui: &mut egui::Ui, text: &str) {
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
        ui.label(job);
    }

    fn render_framebuffer(&self, ui: &mut egui::Ui) {
        // Poll the live framebuffer while running so the Framebuffer tab updates too.
        let fb_bytes: Option<Vec<u8>> = match &self.phase {
            BootPhase::Idle => None,
            BootPhase::Running { vm, .. } => {
                Some(vm.peek_bytes_raw(RAM_BASE, FB_COLS * FB_ROWS))
            }
            BootPhase::Done(r) => Some(r.fb_bytes.clone()),
        };

        let margin = 8.0;
        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, Color32::from_rgb(30, 50, 30)))
            .inner_margin(margin)
            .show(ui, |ui| {
                match fb_bytes {
                    None => {
                        ui.colored_label(
                            TERM_DIM,
                            "Boot the kernel to see framebuffer contents.",
                        );
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
                            .id_salt("machine_framebuffer")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height())
                            .show(ui, |ui| {
                                ui.label(RichText::new(text).monospace().color(TERM_TEXT));
                            });
                    }
                }
            });
    }

    fn render_input(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("IN>").monospace().color(TERM_OK));
            let text_edit = egui::TextEdit::singleline(&mut self.uart_input)
                .id_salt("machine_stdin")
                .desired_width(ui.available_width() - 100.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("stdin — press Enter to send");
            let resp = ui.add(text_edit);
            
            let send = ui.button("Send").clicked();
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if send || enter {
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
    }
}
