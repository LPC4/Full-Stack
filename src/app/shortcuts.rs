use super::{FullStackApp, MachineState, MainSurface, TerminalKind, Tool, egui, quick_open};

impl FullStackApp {
    pub(super) fn handle_shortcuts(&mut self, ui: &egui::Ui, now: f64) {
        if self.guest_input_active() && ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            return;
        }
        if ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_all_documents();
        } else if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_active_document();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::B)) {
            self.build_selected_config(now);
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::R)) {
            self.run_selected_config(now);
        }
        // Debug transport: F5 resume-or-run (shift: debug when idle, stop when
        // live; ctrl: restart), F6 pause, F10 steps per the dock unit.
        if ui.input(|i| i.key_pressed(egui::Key::F5)) {
            let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
            let alive = self.live.as_ref().is_some_and(|live| live.state.is_alive());
            if alive && shift {
                self.stop_live_machine();
            } else if alive && ctrl {
                self.restart_selected_config(now);
            } else if alive {
                if let Some(live) = &mut self.live
                    && live.state == MachineState::Paused
                {
                    live.resume();
                }
            } else if shift {
                self.debug_selected_config(now);
            } else {
                self.run_selected_config(now);
            }
        }
        // F9: toggle a breakpoint on the editor's caret line.
        if ui.input(|i| i.key_pressed(egui::Key::F9))
            && let (Some((line, _)), Some(path)) = (self.cursor_pos, self.active_path())
        {
            self.toggle_source_breakpoint(path.as_str(), line);
        }
        if ui.input(|i| i.key_pressed(egui::Key::F6))
            && let Some(live) = &mut self.live
        {
            live.pause();
        }
        if ui.input(|i| i.key_pressed(egui::Key::F10)) && self.live.is_some() {
            self.step_selected_live_machine();
        }
        if ui.input(|i| i.key_pressed(egui::Key::F11)) && self.live.is_some() {
            if ui.input(|i| i.modifiers.shift) {
                if let Some(live) = &mut self.live {
                    live.step_out();
                }
            } else {
                self.step_into_selected_live_machine();
            }
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::J)) {
            self.terminal_open = !self.terminal_open;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::W)) {
            match self.main_surface {
                MainSurface::Debugger => self.close_debugger_surface(),
                MainSurface::Document => self.close_active_path(),
            }
        }
        if ui.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::F)) {
            self.tool = Tool::Search;
            self.search.request_focus();
        } else if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            self.find.request_open();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::P)) {
            self.quick_open = Some(quick_open::QuickOpenState::default());
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.settings_open = !self.settings_open;
        }
        if self.settings_open && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }
    }

    pub(super) fn guest_input_active(&self) -> bool {
        let alive = self.live.as_ref().is_some_and(|live| live.state.is_alive());
        alive && self.terminal_open && self.terminal.active == TerminalKind::Machine
    }
}
