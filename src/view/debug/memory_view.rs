use crate::view::debug::ADDRESS_PRESETS;
use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use virtual_machine::bus::RAM_BASE;
use egui::{Grid, RichText, ScrollArea, Ui};

#[derive(Clone)]
pub struct MemoryView {
    addr_input: String,
    current_addr: u64,
}

impl Default for MemoryView {
    fn default() -> Self {
        Self {
            addr_input: format!("{RAM_BASE:#010x}"),
            current_addr: RAM_BASE,
        }
    }
}

impl CompilerView for MemoryView {
    fn title(&self) -> &'static str {
        "Memory"
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

        // Layout calculation based on font width
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let char_w = ui
            .painter()
            .layout_no_wrap("0".to_owned(), font_id.clone(), ui.visuals().text_color())
            .size()
            .x
            .max(6.0);
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;

        let available_w = ui.available_width();
        let available_h = ui.available_height();

        // Address column = 10 chars. Grid spacing + scrollbar ~ 60px.
        let reserved_w = (10.0 * char_w) + 60.0;
        let usable_w = (available_w - reserved_w).max(char_w * 34.0);

        let px_per_8_bytes = 34.0 * char_w;
        let half_chunks = (usable_w / px_per_8_bytes).floor() as usize;
        let bytes_per_row = (half_chunks.max(1)) * 8;

        // Dynamic page size based on available height (leave ~120px for the new toolbar)
        let num_rows = ((available_h - 120.0) / row_height).floor().max(8.0) as usize;
        let page_size = num_rows * bytes_per_row;

        // Snap current address to an 8-byte boundary to keep rows perfectly aligned
        self.current_addr &= !7;

        // Top toolbar
        egui::Frame::NONE
            .fill(theme.panel_alt)
            .corner_radius(6.0)
            .inner_margin(10.0)
            .show(ui, |ui| {
                // Row 1: Presets
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new("Jump to:").weak().small());

                    for preset in ADDRESS_PRESETS {
                        if ui.small_button(preset.label).clicked() {
                            self.current_addr = preset.addr & !7;
                            self.addr_input = format!("{:#010x}", preset.addr);
                        }
                    }
                    for (label, addr) in &session.snapshot.section_presets {
                        if ui.small_button(*label).clicked() {
                            self.current_addr = addr & !7;
                            self.addr_input = format!("{:#010x}", addr);
                        }
                    }

                    // Show available presets info
                    if session.snapshot.section_presets.is_empty() {
                        ui.label(
                            RichText::new("(No section symbols found)")
                                .weak()
                                .small()
                                .color(theme.warning),
                        );
                    }
                });

                ui.add_space(8.0);

                // Row 2: Search, Navigation, and Range Info
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Address:").strong());

                    // Fixed-width text edit so it doesn't awkwardly stretch
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.addr_input)
                            .desired_width(120.0)
                            .font(egui::TextStyle::Monospace),
                    );

                    if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Go").clicked()
                    {
                        let cleaned = self.addr_input.trim().trim_start_matches("0x");
                        if let Ok(addr) = u64::from_str_radix(cleaned, 16) {
                            self.current_addr = addr & !7;
                        }
                    }

                    ui.add_space(8.0);

                    // Button to jump to current PC
                    if ui
                        .small_button("@PC")
                        .on_hover_text(format!(
                            "Jump to current PC: {:#010x}",
                            session.snapshot.cpu.pc
                        ))
                        .clicked()
                    {
                        self.current_addr = session.snapshot.cpu.pc & !7;
                        self.addr_input = format!("{:#010x}", self.current_addr);
                    }

                    ui.separator();
                    ui.add_space(8.0);

                    if ui.button("< Prev").clicked() {
                        self.current_addr = self.current_addr.saturating_sub(page_size as u64) & !7;
                        self.addr_input = format!("{:#010x}", self.current_addr);
                    }
                    if ui.button("Next >").clicked() {
                        self.current_addr = self.current_addr.saturating_add(page_size as u64) & !7;
                        self.addr_input = format!("{:#010x}", self.current_addr);
                    }

                    // Push the range display to the far right of the toolbar
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "[ {:#010x} - {:#010x} ]",
                                self.current_addr,
                                self.current_addr + page_size as u64 - 1
                            ))
                            .monospace()
                            .weak()
                            .color(theme.text_dim),
                        );
                    });
                });
            });

        ui.add_space(8.0);

        // Fetch bytes from VM
        let bytes = session.peek_bytes(self.current_addr, page_size);

        // Debug info: show how many bytes were actually read
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Read {} bytes from {:#010x}",
                    bytes.len(),
                    self.current_addr
                ))
                .small()
                .weak(),
            );
        });
        ui.add_space(4.0);

        // Hex dump
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Grid::new("memdump_grid")
                    .num_columns(3)
                    .striped(true)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        // Header (Left aligned perfectly with the data)
                        ui.label(
                            RichText::new("Address")
                                .monospace()
                                .strong()
                                .color(theme.text_dim),
                        );
                        ui.label(
                            RichText::new("Hex")
                                .monospace()
                                .strong()
                                .color(theme.text_dim),
                        );
                        ui.label(
                            RichText::new("ASCII")
                                .monospace()
                                .strong()
                                .color(theme.text_dim),
                        );
                        ui.end_row();

                        for (row_idx, chunk) in bytes.chunks(bytes_per_row).enumerate() {
                            let row_addr = self.current_addr + (row_idx * bytes_per_row) as u64;

                            // Address column
                            ui.label(
                                RichText::new(format!("{row_addr:#010x}"))
                                    .monospace()
                                    .color(theme.text_soft),
                            );

                            // Hex column
                            let mut hex_str =
                                String::with_capacity(bytes_per_row * 3 + half_chunks * 2);
                            for (i, &b) in chunk.iter().enumerate() {
                                if i > 0 && i % 8 == 0 {
                                    hex_str.push(' ');
                                }
                                hex_str.push_str(&format!("{b:02x} "));
                            }
                            ui.label(RichText::new(hex_str).monospace().color(theme.text));

                            // ASCII column
                            let mut ascii_str = String::with_capacity(bytes_per_row + half_chunks);
                            for (i, &b) in chunk.iter().enumerate() {
                                if i > 0 && i % 8 == 0 {
                                    ascii_str.push(' ');
                                }
                                if b.is_ascii_graphic() || b == b' ' {
                                    ascii_str.push(b as char);
                                } else {
                                    ascii_str.push('.');
                                }
                            }
                            ui.label(RichText::new(ascii_str).monospace().color(theme.text_soft));

                            ui.end_row();
                        }
                    });
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
