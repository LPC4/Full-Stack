use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, Frame, RichText, ScrollArea, Stroke, Ui};

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
        let Some(session) = state.debug_session.as_mut() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        ui.heading("UART");
        ui.add_space(4.0);

        // ---- Output (RX from VM) ----
        ui.label(RichText::new("Output (VM -> host)").strong());
        let output = String::from_utf8_lossy(&session.uart_output).into_owned();
        let available = ui.available_height() - 80.0;

        Frame::NONE
            .stroke(Stroke::new(1.0, Color32::from_gray(70)))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ScrollArea::vertical()
                    .max_height(available.max(60.0))
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

        // ---- Input (TX to VM) ----
        ui.label(RichText::new("Input (host -> VM)").strong());
        ui.horizontal(|ui| {
            let resp = ui.text_edit_singleline(&mut self.tx_input);
            let send = ui.button("Send").clicked()
                || (resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if send && !self.tx_input.is_empty() {
                let bytes: Vec<u8> = self.tx_input.bytes().chain(std::iter::once(b'\n')).collect();
                session.send_uart(bytes);
                self.tx_input.clear();
            }
        });

        ui.add_space(2.0);
        ui.label(
            RichText::new("Bytes are queued and sent to the VM's UART RX on the next step.")
                
                .weak(),
        );
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
