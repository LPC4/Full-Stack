//! Machine window: secondary egui window for booting and observing kernel programs.

use std::time::{Duration, Instant};

use asm_to_binary::AssembledOutput;
use egui::{Color32, Frame, Margin, RichText, Stroke, Vec2};
use full_stack::view::ui_theme;
use virtual_machine::devices::framebuffer::{FB_HEIGHT, FB_WIDTH};
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// --- Palette ---

fn term_bg() -> Color32 {
    Color32::from_rgb(1, 1, 5)
}
fn term_cursor() -> Color32 {
    ui_theme().accent
}
fn term_text() -> Color32 {
    ui_theme().text
}
fn term_ok() -> Color32 {
    ui_theme().success
}
fn term_warn() -> Color32 {
    ui_theme().warning
}
fn term_err() -> Color32 {
    ui_theme().error
}
fn term_dim() -> Color32 {
    ui_theme().text_dim
}
fn term_panic() -> Color32 {
    Color32::from_rgb(255, 60, 80)
}
fn term_border() -> Color32 {
    ui_theme().border_soft
}
fn toolbar_bg() -> Color32 {
    ui_theme().panel
}

// --- Tuning ---

/// Wall-clock budget spent stepping the VM per UI frame. The VM runs as many
/// cycles as fit in this window rather than a fixed count, so throughput scales
/// with host speed instead of being pinned to ~`STEP_BATCH` x 60fps. Kept under a
/// 16ms frame so the UI stays responsive.
const FRAME_STEP_BUDGET: Duration = Duration::from_millis(8);

/// Cycles run between wall-clock checks. Amortizes the `Instant::now()` cost
/// while keeping the budget overshoot small.
const STEP_BATCH: u64 = 4096;

/// Fixed height of the top toolbar row (boot / stop / clear + status).
const TOOLBAR_H: f32 = 34.0;

// --- Phase ---

#[derive(Clone, Default)]
pub struct BootResult {
    pub uart_output: String,
    pub exit_code: Option<i64>,
    pub steps: u64,
    pub max_steps_reached: bool,
    /// Snapshot of the framebuffer device's RGBA8888 pixel buffer at stop time.
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
        /// Step budget before the boot is reported as timed out (from settings).
        max_steps: u64,
        uart_text: String,
        /// Incremented every time new UART arrives; used to skip `LayoutJob` rebuilds.
        log_generation: u64,
    },
    Done(BootResult),
}

impl Default for BootPhase {
    fn default() -> Self {
        Self::Idle
    }
}

// --- Main struct ---

#[derive(Default)]
pub struct MachineWindow {
    pub open: bool,
    pub boot_requested: bool,
    phase: BootPhase,
    pub active_tab: FbTab,
    pub selected_user_inject: bool,
    terminal_focused: bool,
    log_cache: Option<egui::text::LayoutJob>,
    log_cache_generation: u64,
    log_cache_cursor: bool,
    /// GPU texture the framebuffer pixels are uploaded into each frame.
    fb_texture: Option<egui::TextureHandle>,
}

// --- Public API ---

impl MachineWindow {
    /// Begin an incremental boot. The VM is ticked each frame via `ui()`.
    /// `max_steps` is the step budget (from settings) before the boot times out.
    pub fn start_boot(
        &mut self,
        assembled: &AssembledOutput,
        user_binary: Option<&AssembledOutput>,
        fs_image: Option<&[u8]>,
        max_steps: u64,
    ) {
        let mut vm = Box::new(VirtualMachine::new_kernel(assembled));

        // Inject a filesystem image if provided. The kernel reads the image base
        // and size from the metadata page at FS_META_PA during boot.
        if let Some(image) = fs_image {
            const FS_META_PA: u64 = 0x87BF_F000;
            const FS_IMAGE_PA: u64 = 0x87C0_0000;
            let _ = vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes());
            let _ = vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes());
            let _ = vm.write_ram(FS_IMAGE_PA, image);
        }

        // Inject user program into RAM if provided.
        if let Some(user_asm) = user_binary {
            // Include BSS (zero-filled globals like heap_buffer) so malloc works in user space.
            let mut flat = user_asm.to_flat_binary();
            let page_size = 4096usize;
            let padded = (flat.len() + page_size - 1) / page_size * page_size;
            flat.resize(padded, 0u8);

            const USER_CODE_VA: u64 = 0x4000_0000;
            if let Some(entry_off) = user_asm.symbol_address("_start") {
                let entry_va = USER_CODE_VA + entry_off;
                const USER_BINARY_PA: u64 = 0x87F0_0000;
                const USER_META_PA: u64 = 0x87EF_F000;
                let user_size = flat.len() as u64;

                let _ = vm.write_ram(USER_META_PA, &entry_va.to_le_bytes());
                let _ = vm.write_ram(USER_META_PA + 8, &user_size.to_le_bytes());
                let _ = vm.write_ram(USER_BINARY_PA, &flat);
            }
        }

        self.phase = BootPhase::Running {
            vm,
            steps: 0,
            max_steps: max_steps.max(1),
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

        let content_h = ui.available_height().max(100.0);

        let focused = self.terminal_focused && is_running;
        let ring = if focused {
            Stroke::new(1.5, term_cursor().gamma_multiply(0.7))
        } else {
            Stroke::new(1.0, term_border())
        };

        Frame::NONE
            .fill(term_bg())
            .stroke(ring)
            .corner_radius(4.0)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), content_h));
                ui.set_max_height(content_h);
                match self.active_tab {
                    FbTab::BootLog => self.render_console(ui, is_running),
                    FbTab::Framebuffer => self.render_framebuffer(ui, is_running),
                }
            });
    }
}

// --- VM tick ---

impl MachineWindow {
    fn maybe_tick(&mut self, ctx: &egui::Context) {
        let transition = match &mut self.phase {
            BootPhase::Running {
                vm,
                steps,
                max_steps,
                uart_text,
                log_generation,
            } => {
                let mut halted: Option<i64> = None;
                let mut timed_out = false;

                // Run cycles in batches until the wall-clock budget for this
                // frame is spent, the step cap is hit, or the VM halts.
                let frame_start = Instant::now();
                'budget: loop {
                    for _ in 0..STEP_BATCH {
                        if *steps >= *max_steps {
                            timed_out = true;
                            break 'budget;
                        }
                        match vm.step() {
                            Ok(StepOutcome::Continue) => *steps += 1,
                            Ok(StepOutcome::Halted(code)) => {
                                *steps += 1;
                                halted = Some(code);
                                break 'budget;
                            }
                            Err(_) => {
                                *steps += 1;
                                halted = Some(-1);
                                break 'budget;
                            }
                        }
                    }
                    if frame_start.elapsed() >= FRAME_STEP_BUDGET {
                        break;
                    }
                }

                let new_bytes = vm.drain_uart_output();
                if !new_bytes.is_empty() {
                    append_uart_bytes(uart_text, &new_bytes);
                    *log_generation = log_generation.wrapping_add(1);
                }

                if halted.is_some() || timed_out {
                    let fb_bytes = vm.peek_framebuffer().to_vec();
                    Some(BootResult {
                        uart_output: std::mem::take(uart_text),
                        exit_code: halted,
                        steps: *steps,
                        max_steps_reached: timed_out,
                        fb_bytes,
                    })
                } else {
                    // Rate-limit repaints to roughly 60 fps to avoid saturating the CPU.
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

// --- Rendering ---

impl MachineWindow {
    fn render_toolbar(&mut self, ui: &mut egui::Ui, has_kernel: bool, is_running: bool) {
        Frame::NONE
            .fill(toolbar_bg())
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_min_height(TOOLBAR_H);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    let boot_label = if is_running { "Booting..." } else { "Boot" };
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
                                term_dim(),
                                RichText::new(format!("exit:{code}")).monospace().size(11.0),
                            );
                        }
                        ui.colored_label(
                            term_dim(),
                            RichText::new(format!("{steps} steps"))
                                .monospace()
                                .size(11.0),
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
            BootPhase::Idle => ("IDLE", term_dim(), 0, None),
            BootPhase::Running { steps, .. } => ("RUNNING", ui_theme().accent, *steps, None),
            BootPhase::Done(r) if r.max_steps_reached => {
                ("TIMEOUT", term_warn(), r.steps, r.exit_code)
            }
            BootPhase::Done(r) if r.exit_code == Some(0) => ("OK", term_ok(), r.steps, r.exit_code),
            BootPhase::Done(r) if r.exit_code.is_some() => {
                ("ERR", term_err(), r.steps, r.exit_code)
            }
            BootPhase::Done(r) => ("HALTED", term_dim(), r.steps, r.exit_code),
        }
    }

    fn do_stop(&mut self) {
        let result = match &mut self.phase {
            BootPhase::Running {
                vm,
                steps,
                uart_text,
                ..
            } => {
                let fb_bytes = vm.peek_framebuffer().to_vec();
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

    fn render_console(&mut self, ui: &mut egui::Ui, is_running: bool) {
        // --- Focus handling ---
        // Click the console to focus, click away to blur.
        let rect = ui.max_rect();
        let id = ui.make_persistent_id("mw_console_surface");
        let resp = ui.interact(rect, id, egui::Sense::click());
        if resp.clicked() {
            self.terminal_focused = true;
        }
        if ui.input(|i| i.pointer.any_pressed()) {
            let clicked_outside = ui
                .input(|i| i.pointer.interact_pos())
                .map(|p| !rect.contains(p))
                .unwrap_or(false);
            if clicked_outside {
                self.terminal_focused = false;
            }
        }
        let focused = self.terminal_focused && is_running;

        // --- Console input ---
        // Send keystrokes straight to the VM's UART while focused.
        if focused {
            let bytes = collect_console_input(ui);
            if !bytes.is_empty() {
                if let BootPhase::Running { vm, .. } = &mut self.phase {
                    for b in bytes {
                        vm.push_uart_rx(b);
                    }
                }
            }
            // Keep the cursor blinking and input responsive.
            ui.ctx().request_repaint_after(Duration::from_millis(120));
        }

        // --- Decide what to show ---
        enum LogState<'a> {
            Idle,
            BootingNoOutput(u64),
            HasText { text: &'a str, generation: u64 },
            DoneEmpty,
        }

        let state = match &self.phase {
            BootPhase::Idle => LogState::Idle,
            BootPhase::Running {
                uart_text,
                steps,
                log_generation,
                ..
            } => {
                if uart_text.is_empty() {
                    LogState::BootingNoOutput(*steps)
                } else {
                    LogState::HasText {
                        text: uart_text.as_str(),
                        generation: *log_generation,
                    }
                }
            }
            BootPhase::Done(r) => {
                if r.uart_output.is_empty() {
                    LogState::DoneEmpty
                } else {
                    LogState::HasText {
                        text: r.uart_output.as_str(),
                        generation: u64::MAX,
                    }
                }
            }
        };

        // Blinking block cursor, shown only while the console is focused.
        let cursor_on = focused && (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;

        egui::ScrollArea::vertical()
            .id_salt("mw_console")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| match state {
                LogState::Idle => {
                    ui.colored_label(term_dim(), "Press Boot to compile and run the kernel.");
                }
                LogState::BootingNoOutput(steps) => {
                    ui.colored_label(term_dim(), format!("Booting... ({steps} steps)"));
                }
                LogState::DoneEmpty => {
                    ui.colored_label(term_dim(), "(no output)");
                }
                LogState::HasText { text, generation } => {
                    // Rebuild the layout only when output advanced or the cursor toggled.
                    if self.log_cache.is_none()
                        || self.log_cache_generation != generation
                        || self.log_cache_cursor != cursor_on
                    {
                        self.log_cache = Some(build_log_job(text, cursor_on));
                        self.log_cache_generation = generation;
                        self.log_cache_cursor = cursor_on;
                    }
                    if let Some(job) = self.log_cache.clone() {
                        ui.label(job);
                    }
                }
            });
    }

    /// Draw the framebuffer as a scaled image: the live device while running,
    /// the captured snapshot once the run stops.
    fn render_framebuffer(&mut self, ui: &mut egui::Ui, is_running: bool) {
        let bytes: Option<&[u8]> = match &self.phase {
            BootPhase::Idle => None,
            BootPhase::Running { vm, .. } => Some(vm.peek_framebuffer()),
            BootPhase::Done(r) => Some(r.fb_bytes.as_slice()),
        };

        let expected = FB_WIDTH * FB_HEIGHT * 4;
        match bytes {
            None => {
                ui.colored_label(term_dim(), "Boot the kernel to see framebuffer contents.");
                return;
            }
            Some(b) if b.len() < expected || b.iter().all(|&x| x == 0) => {
                ui.colored_label(term_dim(), "(framebuffer is blank)");
                return;
            }
            Some(_) => {}
        }

        // Upload the pixels into the texture, allocating it on first use.
        let pixels = bytes.unwrap();
        let image =
            egui::ColorImage::from_rgba_unmultiplied([FB_WIDTH, FB_HEIGHT], &pixels[..expected]);
        let opts = egui::TextureOptions::NEAREST;
        match &mut self.fb_texture {
            Some(tex) => tex.set(image, opts),
            none => {
                *none = Some(ui.ctx().load_texture("mw_framebuffer", image, opts));
            }
        }

        // Keep the live image refreshing as the device changes.
        if is_running {
            ui.ctx().request_repaint();
        }

        if let Some(tex) = &self.fb_texture {
            // Scale up to fit the available area, preserving aspect.
            let avail = ui.available_size();
            let scale = (avail.x / FB_WIDTH as f32)
                .min(avail.y / FB_HEIGHT as f32)
                .max(1.0);
            let size = egui::vec2(FB_WIDTH as f32 * scale, FB_HEIGHT as f32 * scale);
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new(tex).fit_to_exact_size(size));
            });
        }
    }
}

/// Append UART output bytes to the console buffer, interpreting the control
/// codes a terminal cares about. Backspace (0x08) erases the previous character
/// so the shell's `BS space BS` erase sequence visibly deletes a character;
/// carriage returns are dropped (the shell uses newlines).
fn append_uart_bytes(buf: &mut String, bytes: &[u8]) {
    for &b in bytes {
        match b {
            0x08 => {
                // Erase one character, but never merge lines across a newline.
                if !matches!(buf.chars().next_back(), None | Some('\n')) {
                    buf.pop();
                }
            }
            0x0d => {}
            _ => buf.push(b as char),
        }
    }
}

/// Translate this frame's keyboard events into bytes for the VM's UART receive
/// buffer. Printable ASCII comes through `Text` events; Enter/Backspace/Tab are
/// `Key` presses mapped to their control codes (the shell echoes them back).
fn collect_console_input(ui: &egui::Ui) -> Vec<u8> {
    let mut out = Vec::new();
    ui.input(|i| {
        for ev in &i.events {
            match ev {
                egui::Event::Text(t) => {
                    for ch in t.chars() {
                        let c = ch as u32;
                        if (0x20..0x7f).contains(&c) {
                            out.push(c as u8);
                        }
                    }
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    egui::Key::Enter => out.push(b'\r'),
                    egui::Key::Backspace => out.push(0x08),
                    egui::Key::Tab => out.push(b'\t'),
                    _ => {}
                },
                _ => {}
            }
        }
    });
    out
}

// --- Log colorizer ---

fn build_log_job(text: &str, cursor: bool) -> egui::text::LayoutJob {
    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();

    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let (tag, tag_col, rest_col) = if line.starts_with("[  OK  ]") {
            (Some("[  OK  ]"), term_ok(), term_text())
        } else if line.starts_with("[ WARN ]") {
            (Some("[ WARN ]"), term_warn(), term_warn())
        } else if line.starts_with("[ ERR  ]") {
            (Some("[ ERR  ]"), term_err(), term_err())
        } else if line.starts_with("PANIC") || line.starts_with("panic") {
            (None, term_panic(), term_panic())
        } else {
            (None, term_text(), term_text())
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
        // No trailing newline after the final line, so the cursor can sit right
        // after the prompt rather than on a blank line below it.
        if lines.peek().is_some() {
            job.append("\n", 0.0, fmt(term_text()));
        }
    }

    // Block cursor in the accent colour, drawn at the end of the output.
    if cursor {
        job.append(
            "\u{2588}",
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: term_cursor(),
                ..Default::default()
            },
        );
    }

    job
}
