//! Machine window: secondary egui window for booting and observing kernel programs.
//!
//! Opened via the "Machine" button in the IDE top bar. Holds its own VM instance
//! and is independent of the IDE/Debug state. Renders:
//!   - Boot log (UART output, colored by log level)
//!   - Framebuffer (text or pixel mode, read from RAM after boot)
//!   - Boot status strip

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

const MAX_STEPS: u64 = 10_000_000;
const FB_COLS: usize = 80;
const FB_ROWS: usize = 25;

#[derive(Clone, Default)]
pub struct BootResult {
    pub uart_output: String,
    pub exit_code: Option<i64>,
    pub steps: u64,
    pub max_steps_reached: bool,
    /// Raw bytes from `RAM_BASE` for the text framebuffer (80×25).
    pub fb_bytes: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum FbTab {
    #[default]
    BootLog,
    Framebuffer,
}

#[derive(Clone, Default)]
pub struct MachineWindow {
    pub open: bool,
    pub boot_requested: bool,
    pub result: Option<BootResult>,
    pub active_tab: FbTab,
}

impl MachineWindow {
    /// Boot the kernel from `assembled` and store the result.
    pub fn boot(&mut self, assembled: &asm_to_binary::AssembledOutput) {
        let mut vm = VirtualMachine::new_kernel(assembled);
        let run = vm.run(MAX_STEPS);

        let fb_bytes = vm.peek_bytes_raw(RAM_BASE, FB_COLS * FB_ROWS);

        self.result = Some(BootResult {
            uart_output: run.uart_output,
            exit_code: match run.outcome {
                StepOutcome::Halted(c) => Some(c),
                StepOutcome::Continue => None,
            },
            steps: run.steps,
            max_steps_reached: matches!(run.outcome, StepOutcome::Continue),
            fb_bytes,
        });
    }

    /// Render the machine window content into `ui`.
    /// Call this inside an `egui::Window::show()` closure.
    pub fn ui(&mut self, ui: &mut egui::Ui, has_kernel: bool) {
        ui.horizontal(|ui| {
            let boot_enabled = has_kernel;
            if ui
                .add_enabled(
                    boot_enabled,
                    egui::Button::new(RichText::new("Boot").strong())
                        .fill(Color32::from_rgb(20, 60, 100))
                        .min_size(egui::vec2(72.0, 28.0)),
                )
                .on_disabled_hover_text("Compile a Kernel-mode program in the IDE first")
                .on_hover_text("Compile and boot the current kernel program")
                .clicked()
            {
                self.open = true;
                self.boot_requested = true;
            }

            if ui.button("Clear").clicked() {
                self.result = None;
            }

            if let Some(r) = &self.result {
                ui.separator();
                let (txt, col) = if r.max_steps_reached {
                    ("TIMEOUT", TERM_WARN)
                } else if r.exit_code == Some(0) {
                    ("OK", TERM_OK)
                } else if r.exit_code.is_some() {
                    ("ERR", TERM_ERR)
                } else {
                    ("RUNNING", TERM_DIM)
                };
                ui.colored_label(col, txt);
                if let Some(code) = r.exit_code {
                    ui.colored_label(TERM_DIM, format!("exit:{code}"));
                }
                ui.colored_label(TERM_DIM, format!("{} steps", r.steps));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut self.active_tab, FbTab::Framebuffer, "Framebuffer");
                ui.selectable_value(&mut self.active_tab, FbTab::BootLog, "Boot Log");
            });
        });

        ui.add_space(4.0);

        match self.active_tab {
            FbTab::BootLog => self.render_boot_log(ui),
            FbTab::Framebuffer => self.render_framebuffer(ui),
        }
    }

    fn render_boot_log(&self, ui: &mut egui::Ui) {
        let avail = ui.available_height();
        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, Color32::from_rgb(30, 50, 30)))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), avail));
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| match &self.result {
                        None => {
                            ui.colored_label(TERM_DIM, "Press Boot to compile and run the kernel.");
                        }
                        Some(r) if r.uart_output.is_empty() => {
                            ui.colored_label(TERM_DIM, "(no output)");
                        }
                        Some(r) => {
                            let font = egui::FontId::monospace(12.0);
                            let mut job = egui::text::LayoutJob::default();
                            for line in r.uart_output.split('\n') {
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
                    });
            });
    }

    fn render_framebuffer(&self, ui: &mut egui::Ui) {
        let avail = ui.available_height();
        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, Color32::from_rgb(30, 50, 30)))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), avail));
                match &self.result {
                    None => {
                        ui.colored_label(TERM_DIM, "Boot the kernel to see framebuffer contents.");
                    }
                    Some(r) if r.fb_bytes.is_empty() => {
                        ui.colored_label(TERM_DIM, "(no framebuffer data)");
                    }
                    Some(r) => {
                        let mut text = String::with_capacity(FB_COLS * FB_ROWS + FB_ROWS);
                        for row in r.fb_bytes.chunks(FB_COLS) {
                            for &b in row {
                                text.push(if b.is_ascii_graphic() || b == b' ' {
                                    b as char
                                } else {
                                    '.'
                                });
                            }
                            text.push('\n');
                        }
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.label(RichText::new(text).monospace().color(TERM_TEXT));
                        });
                    }
                }
            });
    }
}
