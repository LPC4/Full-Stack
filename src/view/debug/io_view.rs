use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Clone, Default)]
pub struct IoView {
    tx_input: String,
}

impl CompilerView for IoView {
    fn title(&self) -> &'static str {
        "IO"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        let Some(session) = state.debug_session.as_mut() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        // Output section
        ui.label(RichText::new("UART Output").strong());
        ui.add_space(4.0);

        let output = String::from_utf8_lossy(&session.uart_output).into_owned();
        // Reserve ~52 px for the input row + label + spacing below
        let output_height = (ui.available_height() - 52.0).max(40.0);

        Frame::NONE
            .fill(theme.surface)
            .stroke(Stroke::new(1.0, theme.border_soft))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ScrollArea::vertical()
                    .max_height(output_height)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if output.is_empty() {
                            ui.label(RichText::new("(no output yet)").weak().monospace());
                        } else {
                            ui.label(RichText::new(&output).monospace());
                        }
                    });
            });

        ui.add_space(8.0);

        // Input section
        ui.label(RichText::new("UART Input").strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.tx_input)
                    .desired_width(ui.available_width() - 60.0)
                    .hint_text("type and press Enter or Send"),
            );
            let resp = ui.button("Send").on_hover_text(
                "Bytes are queued and delivered to the VM's UART RX on the next step.",
            );
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (resp.clicked() || enter) && !self.tx_input.is_empty() {
                let bytes: Vec<u8> = self
                    .tx_input
                    .bytes()
                    .chain(std::iter::once(b'\n'))
                    .collect();
                session.send_uart(bytes);
                self.tx_input.clear();
            }
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
