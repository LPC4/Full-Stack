use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Frame, RichText, ScrollArea, Stroke};

#[derive(Clone)]
pub struct VmExecutionResult {
    pub uart_output: String,
    pub exit_code: Option<i32>,
    pub steps: u64,
    pub max_steps_reached: bool,
}

#[derive(Default, Clone)]
pub struct VmExecutionView;

impl CompilerView for VmExecutionView {
    fn title(&self) -> &'static str {
        "VM Output"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        if let Some(result) = &state.vm_result {
            let output = if result.uart_output.is_empty() {
                "(no output)"
            } else {
                &result.uart_output
            };

            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.heading("Virtual Machine Execution");

                // UART output: auto-shrinks for short content, scrolls for long.
                // Reserve ~100px for the summary section below.
                let max_uart = (ui.available_height() - 112.0).max(60.0);

                Frame::NONE
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border_soft))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("UART Output:").strong());
                        ui.add_space(4.0);
                        ScrollArea::vertical()
                            .max_height(max_uart)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.label(RichText::new(output).monospace());
                            });
                    });

                ui.add_space(12.0);

                Frame::NONE
                    .fill(theme.panel_alt)
                    .stroke(Stroke::new(1.0, theme.border_soft))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("Execution Summary").strong());
                        ui.add_space(4.0);

                        let (status_text, status_color) = if result.max_steps_reached {
                            ("[WARNING] Step limit reached", theme.warning)
                        } else if let Some(code) = result.exit_code {
                            if code == 0 {
                                ("[OK] Program halted successfully", theme.success)
                            } else {
                                ("[ERROR] Program exited with non-zero code", theme.error)
                            }
                        } else {
                            ("[UNKNOWN] Execution finished", theme.text_dim)
                        };

                        ui.colored_label(status_color, status_text);
                        if let Some(code) = result.exit_code {
                            ui.label(format!("Exit code: {code}"));
                        }
                        ui.label(format!("Steps executed: {}", result.steps));
                    });
            });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Click \"Run in VM\" to execute the assembled program.").weak(),
                );
            });
        }
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
