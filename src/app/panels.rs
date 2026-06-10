use super::{FullStackApp, ViewWrapper};
use egui::{Frame, Layout, Margin, RichText, Stroke};
use full_stack::compilation_pipeline::TargetMode;
use full_stack::view::debug::SessionStatus;
use full_stack::view::{
    AssemblyView, AstView, CacheView, CfgView, CompilerView, CpuStateView, ExecutionView,
    FramebufferView, IoView, IrView, MemoryView, PipelineView, SourceView, StackView, TokensView,
    VmExecutionView, ui_theme,
};

const IDE_VIEWS: &[(&str, fn() -> Box<dyn CompilerView>)] = &[
    ("Source", || Box::new(SourceView::default())),
    ("Tokens", || Box::new(TokensView::default())),
    ("AST", || Box::new(AstView::default())),
    ("IR", || Box::new(IrView::default())),
    ("Assembly", || Box::new(AssemblyView::default())),
    ("CFG", || Box::new(CfgView::default())),
    ("Stack", || Box::new(StackView::default())),
    ("Execution (QEMU)", || Box::new(ExecutionView::default())),
    ("VM Output", || Box::new(VmExecutionView::default())),
];

const DEBUG_VIEWS: &[(&str, fn() -> Box<dyn CompilerView>)] = &[
    ("CPU State", || Box::new(CpuStateView::default())),
    ("Pipeline", || Box::new(PipelineView::default())),
    ("Memory", || Box::new(MemoryView::default())),
    ("IO", || Box::new(IoView::default())),
    ("Cache", || Box::new(CacheView::default())),
    ("Framebuffer", || Box::new(FramebufferView::default())),
];

// --- Helper: render an "Add View" submenu from a list of descriptors ---

fn add_view_menu(
    ui: &mut egui::Ui,
    label: &str,
    entries: &[(&str, fn() -> Box<dyn CompilerView>)],
    pending: &mut Option<ViewWrapper>,
    counter: &mut u64,
) {
    for (name, make) in entries {
        if ui.button(*name).clicked() {
            *pending = Some(ViewWrapper::new(make(), counter));
            ui.close();
        }
    }
    let _ = label; // suppress unused warning; label kept for clarity at call sites
}

// --- Top bars ---

impl FullStackApp {
    pub(super) fn ide_top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = ui_theme();
        let is_stdlib = self
            .catalog
            .current_program()
            .map(|p| p.is_stdlib() || p.standalone)
            .unwrap_or(false);
        let is_os = self
            .catalog
            .current_program()
            .map(|p| p.is_os())
            .unwrap_or(false);
        let is_kernel = self.target_mode == TargetMode::Kernel;
        let is_user = self
            .catalog
            .current_program()
            .map(|p| p.is_user())
            .unwrap_or(false);

        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Compile").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                self.compile();
            }

            // "Run in VM": shown for non-stdlib, non-kernel, non-user programs.
            if !is_stdlib && !is_kernel && !is_user {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Run in VM").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(100.0, 35.0)),
                    )
                    .on_hover_text("Run the assembled program in the internal RISC-V VM")
                    .clicked()
                {
                    if let Some(assembled) = self.compilation_state.assembled() {
                        let assembled = assembled.clone();
                        let entry = self.compilation_state.entry_symbol.clone();
                        let base = self.compilation_state.load_base;
                        let max_steps = self.settings.max_vm_steps;
                        self.compilation_state.vm_result =
                            Some(super::run_in_vm(&assembled, &entry, base, max_steps));
                        self.focus_vm_output_tab();
                    }
                }
            }

            // "Run": shown for userspace programs. Boots the kernel + shell and
            // auto-runs this program. The kernel is compiled once and cached, with
            // no effect on the catalog selection or the current target mode.
            if is_user {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Run").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(100.0, 35.0)),
                    )
                    .on_hover_text("Boot the kernel and auto-run this program in the shell")
                    .clicked()
                {
                    let program_id = self.catalog.selected_program_id.clone();
                    let prepared = self
                        .ensure_kernel_binary()
                        .and_then(|()| self.compile_and_store_hosted(&program_id));
                    match prepared {
                        Ok(()) => {
                            self.selected_inject_program_id = program_id;
                            self.machine_window.selected_user_inject = true;
                            self.machine_window.open = true;
                            self.machine_window.boot_requested = true;
                            self.machine_window.autorun_requested = true;
                        }
                        Err(e) => {
                            self.compilation_state
                                .set_error(format!("run setup failed: {e}"));
                        }
                    }
                }
            }

            if is_kernel {
                let has_assembled = self.compilation_state.assembled().is_some();
                if ui
                    .add_enabled(
                        has_assembled,
                        egui::Button::new(RichText::new("Machine").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(90.0, 35.0)),
                    )
                    .on_disabled_hover_text("Compile a Kernel program first")
                    .on_hover_text("Open the Machine window to boot and observe the kernel")
                    .clicked()
                {
                    self.machine_window.open = true;
                }
            }

            // Target selector: hidden for stdlib, OS, and userspace programs.
            if !is_stdlib && !is_os && !is_user {
                ui.separator();
                ui.label("Target:");
                let prev_mode = self.target_mode;
                egui::ComboBox::from_id_salt("target_mode_combo")
                    .selected_text(self.target_mode.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.target_mode,
                            TargetMode::Hosted,
                            TargetMode::Hosted.label(),
                        );
                        ui.selectable_value(
                            &mut self.target_mode,
                            TargetMode::Freestanding,
                            TargetMode::Freestanding.label(),
                        );
                        ui.selectable_value(
                            &mut self.target_mode,
                            TargetMode::Kernel,
                            TargetMode::Kernel.label(),
                        );
                    });
                if self.target_mode != prev_mode {
                    let new_mode = self.target_mode;
                    self.target_mode = prev_mode;
                    self.user_set_target_mode = true;
                    self.set_target_mode(new_mode);
                }

                if self.target_mode == TargetMode::Freestanding {
                    ui.label("Base:");
                    let lb_response = ui.add(
                        egui::TextEdit::singleline(&mut self.load_base_input)
                            .desired_width(90.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("0x80200000"),
                    );
                    if lb_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.compile();
                    }
                }
            }

            if ui
                .add(egui::Button::new("⚙").min_size(egui::vec2(35.0, 35.0)))
                .on_hover_text("Settings")
                .clicked()
            {
                self.show_settings = !self.show_settings;
            }

            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("More", |ui| {
                    if ui.button("Reset File").clicked() {
                        if let Some(program) = self.catalog.current_program() {
                            if program.is_custom() {
                                self.catalog.set_selected_source(
                                    full_stack::view::blank_custom_program_source(),
                                );
                            } else {
                                self.catalog.ensure_consistency();
                            }
                        }
                        self.compile();
                        ui.close();
                    }
                    if ui.button("Reset UI Layout").clicked() {
                        self.reset_layout();
                        ui.close();
                    }
                    ui.separator();
                    ui.label(RichText::new("Add View").small().weak());
                    add_view_menu(
                        ui,
                        "IDE",
                        IDE_VIEWS,
                        &mut self.pending_new_view,
                        &mut self.next_view_id,
                    );
                });
            });

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                if !is_stdlib && !is_kernel && !is_user {
                    let can_debug = self.compilation_state.assembled().is_some();
                    if ui
                        .add_enabled(
                            can_debug,
                            egui::Button::new(RichText::new("To Debugger").strong())
                                .fill(theme.accent)
                                .min_size(egui::vec2(100.0, 35.0)),
                        )
                        .on_disabled_hover_text("Compile successfully first")
                        .clicked()
                    {
                        self.enter_debug_mode();
                    }
                }

                ui.separator();

                if let Some(program) = self.catalog.current_program() {
                    let full_name = program.name.clone();
                    let short_name: String = full_name.chars().take(24).collect();
                    let (kind_label, kind_color) = match program.kind {
                        full_stack::view::ProgramKind::Example => ("example", theme.text_dim),
                        full_stack::view::ProgramKind::Custom => ("custom", theme.text_dim),
                        full_stack::view::ProgramKind::Stdlib => ("stdlib", theme.text_dim),
                        full_stack::view::ProgramKind::Os => ("os", theme.text_dim),
                        full_stack::view::ProgramKind::User => ("user", theme.text_dim),
                    };
                    ui.label(RichText::new(kind_label).weak().small().color(kind_color));
                    let name_resp = ui.label(RichText::new(&short_name).strong());
                    if full_name.len() > 24 {
                        name_resp.on_hover_text(&full_name);
                    }
                }

                ui.add_space(20.0);

                let pill_margin = Margin {
                    left: 8,
                    right: 8,
                    top: 3,
                    bottom: 3,
                };
                match &self.compilation_state.error_summary.clone() {
                    Some(summary) => {
                        let short: String = summary.chars().take(40).collect();
                        Frame::NONE
                            .fill(theme.error.gamma_multiply(0.15))
                            .stroke(Stroke::new(1.0, theme.error))
                            .inner_margin(pill_margin)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                ui.colored_label(theme.error, format!("ERR: {short}"));
                            });
                    }
                    None => {
                        Frame::NONE
                            .fill(theme.accent.gamma_multiply(0.18))
                            .stroke(Stroke::new(1.0, theme.accent))
                            .inner_margin(pill_margin)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                ui.colored_label(theme.accent, "OK");
                            });
                    }
                }
            });
        });
    }

    pub(super) fn debug_top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = ui_theme();
        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Reset").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.reset();
                }
            }

            ui.separator();

            ui.label("N:")
                .on_hover_text("Number of instructions or cycles to advance per step button click");
            ui.add(
                egui::TextEdit::singleline(&mut self.step_n_input)
                    .desired_width(36.0)
                    .font(egui::TextStyle::Monospace),
            );
            let n: u64 = self.step_n_input.trim().parse().unwrap_or(1).max(1);

            let session = self.compilation_state.debug_session.as_ref();
            let is_running = session
                .map(|s| s.status == SessionStatus::Running)
                .unwrap_or(false);

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Insn").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text(format!("Retire {n} instruction(s) through the pipeline"))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n_instructions(n);
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Cycle").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text(format!("Advance {n} pipeline cycle(s)"))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n(n);
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Fn").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text("Run until the PC enters a different function")
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_to_next_function();
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new(RichText::new("Run").strong())
                        .fill(theme.accent)
                        .min_size(egui::vec2(100.0, 35.0)),
                )
                .on_hover_text("Run until halt or error")
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n(u64::MAX);
                }
            }

            ui.separator();

            let follow = &mut self.compilation_state.disasm_follow_pc;
            if ui
                .add(
                    egui::Button::new("Follow PC")
                        .selected(*follow)
                        .min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text("Keep the disassembly view scrolled to the current PC")
                .clicked()
            {
                *follow = !*follow;
            }

            if ui
                .add(egui::Button::new("⚙").min_size(egui::vec2(35.0, 35.0)))
                .on_hover_text("Settings")
                .clicked()
            {
                self.show_settings = !self.show_settings;
            }

            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("More", |ui| {
                    if ui.button("Reset layout").clicked() {
                        self.reset_debug_layout();
                        ui.close();
                    }
                    ui.separator();
                    ui.label(RichText::new("Add View").small().weak());
                    add_view_menu(
                        ui,
                        "Debug",
                        DEBUG_VIEWS,
                        &mut self.pending_new_view,
                        &mut self.next_view_id,
                    );
                });
            });

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                if ui
                    .add(
                        egui::Button::new(RichText::new("To IDE").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(100.0, 35.0)),
                    )
                    .clicked()
                {
                    self.exit_debug_mode();
                    return;
                }

                ui.separator();

                let Some(session) = &self.compilation_state.debug_session else {
                    return;
                };

                let (status_text, status_color) = match &session.status {
                    SessionStatus::Running => ("Running", theme.success),
                    SessionStatus::Halted(0) => ("Halted OK", theme.success),
                    SessionStatus::Halted(_) => ("Halted (err)", theme.error),
                    SessionStatus::Error(_) => ("Error", theme.error),
                };
                ui.colored_label(status_color, status_text);
                ui.separator();
                ui.label(
                    RichText::new(format!("{} steps", session.step_count))
                        .monospace()
                        .weak(),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("PC {:#010x}", session.snapshot.cpu.pc))
                        .monospace()
                        .strong(),
                );
            });
        });
    }
}
