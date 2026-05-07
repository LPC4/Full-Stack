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
            // Prepare text for measurement
            let output = if result.uart_output.is_empty() {
                "(no output)".to_owned()
            } else {
                result.uart_output.clone()
            };

            let monospace_font = egui::FontId::monospace(12.0); // match the style used for output

            // Measure text height (monospace, no wrap for simplicity)
            let galley = ui.fonts_mut(|f| {
                f.layout_no_wrap(
                    output.clone(),
                    monospace_font.clone(),
                    ui.visuals().text_color(),
                )
            });

            let frame_vpad = 20.0; // account for frame border + inner margin + label height
            let label_height =
                ui.fonts_mut(|f| f.row_height(&egui::TextStyle::Body.resolve(ui.style())));
            let desired_uart_height = 4.0 + label_height + 4.0 + galley.size().y + frame_vpad; // 4 extra space after label

            let summary_min_height = 82.0;
            let spacing = 12.0;
            let available_height = ui.available_height();
            let max_uart_height = (available_height - summary_min_height - spacing).max(40.0);
            let uart_height = desired_uart_height.min(max_uart_height);

            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.heading("Virtual Machine Execution");

                // Allocate exact height for UART area
                let _uart_height = desired_uart_height.min(max_uart_height);

                Frame::NONE
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border_soft))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.set_min_height(uart_height);
                        ui.label(RichText::new("UART Output:").strong());
                        ui.add_space(4.0);
                        if output == "(no output)"
                            || galley.size().y <= max_uart_height - frame_vpad
                        {
                            // is short enough to display fully
                            ui.label(RichText::new(&output).monospace());
                        } else {
                            ScrollArea::vertical()
                                .max_height(max_uart_height - frame_vpad - label_height - 8.0)
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&output).monospace());
                                });
                        }
                    });

                ui.add_space(spacing);

                // Execution summary
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
                            ui.label(format!("Exit code: {}", code));
                        }
                        ui.label(format!("Steps executed: {}", result.steps));
                    });
            });
        } else {
            // Empty state (unchanged)
            ui.vertical(|ui| {
                ui.add_space(20.0);
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("VM Execution Panel").heading());
                });
                ui.add_space(10.0);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Click \"Run in VM\" to execute the assembled program")
                            .weak(),
                    );
                });
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("on the custom RISC-V virtual machine.").weak());
                });
            });
        }
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
