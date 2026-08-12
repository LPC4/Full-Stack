use super::{
    AssembledOutput, CachedLaunch, ConsoleStream, FullStackApp, LaunchCache, LaunchMode, Layer,
    LiveMachine, MachineState, MainSurface, RunConfig, RunKind, SessionEngine, Tool, debug_view,
    diagnostics, egui, inspector, machine_dock,
};

impl FullStackApp {
    pub(super) fn save_dirty_before_run(&mut self) {
        if self.settings.save_all_before_run && self.workspace.dirty_documents().next().is_some() {
            self.workspace.save_all().ok();
        }
    }

    pub(super) fn run_selected_config(&mut self, now: f64) {
        self.launch_selected_config(now, false);
    }

    // Starts paused at entry regardless of the config's debug flag.
    pub(super) fn debug_selected_config(&mut self, now: f64) {
        self.launch_selected_config(now, true);
    }

    pub(super) fn restart_selected_config(&mut self, now: f64) {
        let debug = if self.live.is_some() {
            self.launch_mode == LaunchMode::Debug
        } else {
            false
        };
        self.launch_selected_config_inner(now, debug, true);
    }

    pub(super) fn launch_selected_config(&mut self, now: f64, debug: bool) {
        self.launch_selected_config_inner(now, debug, false);
    }

    pub(super) fn launch_selected_config_inner(
        &mut self,
        now: f64,
        debug: bool,
        allow_reuse: bool,
    ) {
        self.save_dirty_before_run();
        let Some(config) = self.selected_config().cloned() else {
            self.status_message = Some("no run config selected".to_owned());
            self.terminal.start("Run", "fsc run".to_owned());
            self.terminal
                .push(ConsoleStream::Error, "No run configuration is selected.");
            self.terminal_open = self.settings.open_terminal_on_run;
            return;
        };
        let run_target = self
            .config_root(&config)
            .map(|path| path.to_string())
            .unwrap_or_else(|| config.name.clone());
        let command = if debug {
            format!("fsc run {run_target} # debug session")
        } else {
            format!("fsc run {run_target}")
        };
        self.terminal.start("Run", command);
        self.terminal_open = self.settings.open_terminal_on_run;

        if let Some(root) = self.config_root(&config)
            && self.workspace.open_document(root).is_ok()
        {
            self.reset_layer_for_active_file();
        }
        self.refresh_build_target();
        self.refresh_stage_cache();

        self.live = None;
        self.vm_result = None;
        self.machine_texture = None;
        self.machine_texture_fill_count = None;
        self.reset_machine_views();
        if allow_reuse
            && self.settings.restart_reuses_build
            && self.try_relaunch_cached(&config, now, debug)
        {
            return;
        }
        self.terminal
            .push(ConsoleStream::Info, "Building run target.");
        let engine = SessionEngine::new(self.workspace.fs());
        match config.kind {
            // Hosted and freestanding share the launch path; the session
            // constructor picks the bare-metal VM from the config kind.
            RunKind::Hosted | RunKind::Freestanding => match engine.build_config(&config) {
                Ok(build) => {
                    self.report_linked_build(&build);
                    self.start_hosted_live_session(&config, &build, now, debug);
                    self.store_launch_cache(&config, CachedLaunch::Hosted(Box::new(build)));
                }
                Err(err) => {
                    self.status_message = Some("run failed".to_owned());
                    self.terminal.push(ConsoleStream::Error, err.to_string());
                }
            },
            RunKind::KernelBoot => match engine.boot_kernel_config(&config) {
                Ok(boot) => {
                    self.report_linked_build(&boot.build);
                    self.terminal.push(
                        ConsoleStream::Success,
                        format!(
                            "Built boot FS: {} entries, {} byte image.",
                            boot.fs_entries.len(),
                            boot.fs_image.len()
                        ),
                    );
                    self.start_kernel_live_session(
                        &config,
                        &boot.build,
                        &boot.init,
                        &boot.fs_image,
                        now,
                        debug,
                    );
                    self.store_launch_cache(&config, CachedLaunch::Kernel(Box::new(boot)));
                }
                Err(err) => {
                    self.status_message = Some("run failed".to_owned());
                    self.terminal.push(ConsoleStream::Error, err.to_string());
                }
            },
        }
    }

    pub(super) fn try_relaunch_cached(
        &mut self,
        config: &RunConfig,
        now: f64,
        debug: bool,
    ) -> bool {
        let Some(cache) = self.launch_cache.take() else {
            return false;
        };
        let fingerprint =
            full_stack::session::config_source_fingerprint(self.workspace.fs(), config);
        if cache.config != *config || cache.fingerprint != fingerprint {
            return false;
        }
        self.terminal.push(
            ConsoleStream::Info,
            "Sources unchanged; restarting from the last build.",
        );
        match &cache.launch {
            CachedLaunch::Hosted(build) => {
                self.start_hosted_live_session(config, build, now, debug);
            }
            CachedLaunch::Kernel(boot) => {
                self.start_kernel_live_session(
                    config,
                    &boot.build,
                    &boot.init,
                    &boot.fs_image,
                    now,
                    debug,
                );
            }
        }
        self.launch_cache = Some(cache);
        true
    }

    pub(super) fn store_launch_cache(&mut self, config: &RunConfig, launch: CachedLaunch) {
        let fingerprint =
            full_stack::session::config_source_fingerprint(self.workspace.fs(), config);
        self.launch_cache = Some(LaunchCache {
            config: config.clone(),
            fingerprint,
            launch,
        });
    }

    pub(super) fn report_linked_build(&mut self, build: &full_stack::session::BuildOutcome) {
        // Retain the outcome for the inspector's Artifacts/Symbols panes.
        self.last_build = Some(build.clone());
        self.terminal.push(
            ConsoleStream::Success,
            format!(
                "Linked {}, target {:?}.",
                diagnostics::tally(build.artifacts.units.len() as u64, "unit"),
                build.target
            ),
        );
        if let Some(dir) = &build.artifact_dir {
            self.terminal.push(
                ConsoleStream::Info,
                format!("Wrote .ir/.s/.elf artifacts to {}", dir.display()),
            );
        }
    }

    pub(super) fn start_hosted_live_session(
        &mut self,
        config: &RunConfig,
        build: &full_stack::session::BuildOutcome,
        now: f64,
        debug: bool,
    ) {
        let tab_label = self.session_tab_label_for(config);
        let live = if config.kind == RunKind::Freestanding {
            LiveMachine::new_freestanding(&build.artifacts)
        } else {
            LiveMachine::new_hosted(&build.artifacts)
        };
        match live {
            Ok(mut live) => {
                if let Some(config) = build.latency {
                    live.set_memory_latency_config(config);
                }
                if !config.run.autorun.is_empty() {
                    live.push_uart_line(&config.run.autorun);
                }
                self.live = Some(live);
                self.session_tab_label = Some(tab_label);
                if debug {
                    self.apply_persisted_breakpoints(config);
                }
                self.activate_machine_terminal_for_run();
                self.open_session_surfaces(debug);
                self.layer = Self::source_layer_for_kind(self.active_source_kind());
                self.build_glow_start = Some(now);
                self.record_build_lit(Some(Layer::Elf));
                self.launch_mode = if debug {
                    LaunchMode::Debug
                } else {
                    LaunchMode::Run
                };
                if debug {
                    self.enter_debug_pause();
                }
            }
            Err(err) => {
                self.status_message = Some("run failed".to_owned());
                self.terminal.push(ConsoleStream::Error, err);
            }
        }
    }

    pub(super) fn start_kernel_live_session(
        &mut self,
        config: &RunConfig,
        build: &full_stack::session::BuildOutcome,
        init: &AssembledOutput,
        fs_image: &[u8],
        now: f64,
        debug: bool,
    ) {
        let tab_label = self.session_tab_label_for(config);
        match LiveMachine::new_kernel(
            &build.artifacts,
            Some(init),
            Some(fs_image),
            &config.run.autorun,
        ) {
            Ok(mut live) => {
                if let Some(config) = build.latency {
                    live.set_memory_latency_config(config);
                }
                self.live = Some(live);
                self.session_tab_label = Some(tab_label);
                if debug {
                    self.apply_persisted_breakpoints(config);
                }
                self.activate_machine_terminal_for_run();
                self.open_session_surfaces(debug);
                self.build_glow_start = Some(now);
                self.record_build_lit(Some(Layer::Elf));
                self.launch_mode = if debug {
                    LaunchMode::Debug
                } else {
                    LaunchMode::Run
                };
                if debug {
                    self.enter_debug_pause();
                }
            }
            Err(err) => {
                self.status_message = Some("kernel boot failed".to_owned());
                self.terminal.push(ConsoleStream::Error, err);
            }
        }
    }

    pub(super) fn enter_debug_pause(&mut self) {
        if let Some(live) = &mut self.live {
            live.pause();
        }
        // Source-mapped sessions land on the pc-following Code view; images
        // without a line table fall back to the disassembly.
        self.debug_view = if self.live.as_ref().is_some_and(LiveMachine::has_line_table) {
            debug_view::DebugView::Code
        } else {
            debug_view::DebugView::Disasm
        };
        self.focus_debugger();
        self.terminal.push(
            ConsoleStream::Info,
            "Paused at entry. Step over (F10), step in (F11), or resume (F5).",
        );
    }

    pub(super) fn activate_machine_terminal_for_run(&mut self) {
        if self.settings.focus_machine_console_on_run {
            self.terminal.set_active_machine();
        }
    }

    // On exit/fault/stop this produces the post-mortem `vm_result` the panels read.
    pub(super) fn drive_live_machine(&mut self, ctx: &egui::Context) {
        let budget_steps = self
            .selected_config()
            .map(|config| config.run.max_steps_per_frame)
            .unwrap_or(self.settings.default_steps_per_frame)
            .max(1);
        let wall_budget =
            std::time::Duration::from_millis(self.settings.frame_budget_ms.clamp(1, 32));
        let Some(live) = &mut self.live else {
            return;
        };
        if live.state == MachineState::Running {
            live.drive(budget_steps, wall_budget);
            ctx.request_repaint();
        }
        self.latch_live_result();
    }

    pub(super) fn latch_live_result(&mut self) {
        let Some(live) = &self.live else {
            return;
        };
        if live.state.is_alive() || self.vm_result.is_some() {
            return;
        }
        self.vm_result = Some(live.result());
        let (stream, summary) = match &live.state {
            MachineState::Exited(0) => (
                ConsoleStream::Success,
                format!("exit 0; {}.", diagnostics::tally(live.steps, "VM step")),
            ),
            MachineState::Exited(code) => (
                ConsoleStream::Warning,
                format!(
                    "exit {code}; {}.",
                    diagnostics::tally(live.steps, "VM step")
                ),
            ),
            MachineState::Stopped => (
                ConsoleStream::Info,
                format!("stopped; {}.", diagnostics::tally(live.steps, "VM step")),
            ),
            MachineState::Faulted(msg) => {
                let at = live
                    .source_for_pc(live.registers().pc)
                    .map(|(file, line)| format!(" ({file}:{line})"))
                    .unwrap_or_default();
                (ConsoleStream::Error, format!("fault: {msg}{at}"))
            }
            MachineState::Running | MachineState::Paused => unreachable!("checked above"),
        };
        // The run summary belongs in the terminal; only faults warrant a
        // persistent status-bar note, so a clean exit does not clutter it.
        if matches!(live.state, MachineState::Faulted(_)) {
            self.status_message = Some(summary.clone());
        }
        self.terminal.push(stream, summary);
    }

    // Keeps the final machine state available to the Inspector after hiding the debugger.
    pub(super) fn stop_live_machine(&mut self) {
        let Some(live) = &mut self.live else {
            return;
        };
        live.stop();
        self.dismiss_session_surfaces();
    }

    pub(super) fn debugger_focused(&self) -> bool {
        self.live.is_some() && self.main_surface == MainSurface::Debugger
    }

    pub(super) fn debugger_dock_visible(&self) -> bool {
        self.launch_mode == LaunchMode::Debug
            && self.live.as_ref().is_some_and(|live| live.state.is_alive())
    }

    pub(super) fn focus_debugger(&mut self) {
        if self.launch_mode != LaunchMode::Debug {
            return;
        }
        self.debugger_open = true;
        self.main_surface = MainSurface::Debugger;
        self.settings_open = false;
    }

    pub(super) fn close_debugger_surface(&mut self) {
        self.debugger_open = false;
        self.main_surface = MainSurface::Document;
        self.reset_layer_for_active_file();
    }

    pub(super) fn open_session_surfaces(&mut self, debug: bool) {
        // A fresh session starts pinned to "now" so the scrubber tracks the end.
        self.time_scrub = 0;
        self.time_follow_now = true;
        self.debugger_open = debug;
        self.main_surface = if debug {
            MainSurface::Debugger
        } else {
            MainSurface::Document
        };
        // Debugging reads on the left: bring the inspector's session data up.
        if debug {
            self.tool = Tool::Inspect;
            self.inspector.pane = inspector::InspectorPane::CallStack;
        }
    }

    pub(super) fn dismiss_session_surfaces(&mut self) {
        self.debugger_open = false;
        self.main_surface = MainSurface::Document;
        self.reset_layer_for_active_file();
    }

    // The selected unit controls F10; instruction mode retires exactly one instruction.
    pub(super) fn step_selected_live_machine(&mut self) {
        let Some(live) = &mut self.live else {
            return;
        };
        match self.step_unit {
            machine_dock::StepUnit::Line => live.step_line(),
            machine_dock::StepUnit::Instruction => live.step_in(),
            machine_dock::StepUnit::Cycle => live.step_cycle(),
        }
    }

    pub(super) fn step_into_selected_live_machine(&mut self) {
        let Some(live) = &mut self.live else {
            return;
        };
        if self.step_unit == machine_dock::StepUnit::Line {
            live.step_line_in();
        } else {
            live.step_in();
        }
    }

    pub(super) fn persist_live_breakpoints(&mut self) {
        if !self.settings.persist_breakpoints {
            return;
        }
        let Some(live) = &self.live else {
            return;
        };
        let mut specs = live
            .breakpoints
            .iter()
            .filter_map(|addr| live.breakpoint_spec_for_addr(*addr))
            .collect::<Vec<_>>();
        specs.sort();
        specs.dedup();
        let Some(config) = self
            .workspace
            .run_configs_mut()
            .configs
            .get_mut(self.selected_config)
        else {
            return;
        };
        if config.debug.breakpoints != specs {
            config.debug.breakpoints = specs;
            self.persist_run_configs();
        }
    }

    pub(super) fn apply_persisted_breakpoints(&mut self, config: &RunConfig) {
        let Some(live) = &mut self.live else {
            return;
        };
        for spec in &config.debug.breakpoints {
            match live.resolve_breakpoint(spec) {
                Ok(addr) => {
                    live.breakpoints.insert(addr);
                }
                Err(err) => {
                    self.terminal.push(
                        ConsoleStream::Warning,
                        format!("Breakpoint `{spec}` was not restored: {err}."),
                    );
                }
            }
        }
    }

    // Valid only while the buffer matches the compiled source (edits clear the stage cache).
    pub(super) fn reset_machine_views(&mut self) {
        self.debug_view = debug_view::DebugView::Disasm;
        self.dismiss_session_surfaces();
        self.inspector.clear_session();
        self.code_follow = None;
        self.code_follow_scrolled = None;
    }

    pub(super) fn config_has_screen(&self) -> bool {
        use full_stack::runconfig::PresentMode;
        self.selected_config().is_some_and(|config| {
            matches!(
                config.run.present,
                PresentMode::Framebuffer | PresentMode::Both
            )
        })
    }
}
