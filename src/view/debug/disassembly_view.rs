//! Interactive disassembly view that highlights the current instruction during debugging.

use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{RichText, ScrollArea, Stroke, Ui};

#[derive(Clone, Default)]
pub struct DisassemblyView;

impl CompilerView for DisassemblyView {
    fn title(&self) -> &'static str {
        "Disassembly"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let current_pc = session.snapshot.cpu.pc;
        let asm_text = &state.asm;

        if asm_text.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No assembly available").weak());
            });
            return;
        }

        // Build a map of label -> address from the symbol table
        let mut label_addresses: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for (label, addr) in &session.symbols {
            label_addresses.insert(label.clone(), *addr);
        }

        // Parse assembly into lines and track current label context
        let lines: Vec<&str> = asm_text.lines().collect();
        let mut current_block_start_addr: Option<u64> = None;
        let mut instruction_offset = 0u64; // Track offset within current block

        // First pass: build a map of line indices to addresses
        let mut line_to_address: std::collections::HashMap<usize, u64> =
            std::collections::HashMap::new();
        let mut address_to_line: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check if this is a label line
            if trimmed.ends_with(':') && !trimmed.starts_with('.') {
                let label_name = trimmed.trim_end_matches(':');
                current_block_start_addr = label_addresses.get(label_name).copied();
                instruction_offset = 0;

                // Record the label's address
                if let Some(addr) = current_block_start_addr {
                    line_to_address.insert(line_idx, addr);
                    address_to_line.insert(addr, line_idx);
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('.') && !trimmed.starts_with(';')
            {
                // This is an instruction line
                if let Some(base_addr) = current_block_start_addr {
                    let instr_addr = base_addr + instruction_offset;
                    line_to_address.insert(line_idx, instr_addr);
                    address_to_line.insert(instr_addr, line_idx);
                    instruction_offset += 4; // Each RV64 instruction is 4 bytes
                }
            }
        }

        // Find the line number for the current PC
        let _current_line_idx = address_to_line.get(&current_pc).copied();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                for (line_idx, line) in lines.iter().enumerate() {
                    let trimmed = line.trim();

                    // Check if this line contains the current PC
                    let is_current_line = line_to_address
                        .get(&line_idx)
                        .map_or(false, |&addr| addr == current_pc);

                    // Background highlighting for current instruction
                    if is_current_line {
                        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_height),
                            egui::Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(rect, 2.0, theme.highlight.gamma_multiply(0.16));

                        // Draw a left border indicator
                        ui.painter().line_segment(
                            [rect.left_top(), rect.left_bottom()],
                            Stroke::new(3.0, theme.highlight),
                        );
                    }

                    // Format the line with syntax highlighting
                    let text_color = if is_current_line {
                        theme.text
                    } else if trimmed.starts_with(';') {
                        // Comments
                        theme.text_dim
                    } else if trimmed.starts_with('.') {
                        // Directives
                        theme.text_soft
                    } else if trimmed.ends_with(':') {
                        // Labels
                        theme.syntax.label
                    } else {
                        // Regular instructions
                        theme.text_soft
                    };

                    let font_size = if is_current_line { 13.0 } else { 12.0 };

                    // Add line number
                    let line_num_str = format!("{:>4}", line_idx + 1);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(line_num_str)
                                .monospace()
                                .size(11.0)
                                .color(theme.text_dim),
                        );
                        ui.add_space(8.0);

                        // Show address if present
                        if let Some(addr) = line_to_address.get(&line_idx) {
                            let addr_color = if is_current_line {
                                theme.highlight
                            } else {
                                theme.text_soft
                            };
                            ui.label(
                                RichText::new(format!("{:#018x}", addr))
                                    .monospace()
                                    .size(11.0)
                                    .color(addr_color),
                            );
                            ui.add_space(8.0);
                        } else {
                            ui.add_space(20.0); // Spacer for alignment
                        }

                        // Show the actual instruction/label
                        ui.label(
                            RichText::new(trimmed)
                                .monospace()
                                .size(font_size)
                                .color(text_color),
                        );
                    });
                }
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
