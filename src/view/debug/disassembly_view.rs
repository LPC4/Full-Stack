//! Interactive disassembly view that highlights the current instruction during debugging.

use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, RichText, ScrollArea, Stroke, Ui};

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
        let mut label_addresses: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for (label, addr) in &session.symbols {
            label_addresses.insert(label.clone(), *addr);
        }
        
        // Parse assembly into lines and track current label context
        let lines: Vec<&str> = asm_text.lines().collect();
        let mut current_label: Option<String> = None;
        let mut current_block_start_addr: Option<u64> = None;
        let mut instruction_offset = 0u64; // Track offset within current block
        
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                for (line_idx, line) in lines.iter().enumerate() {
                    let trimmed = line.trim();
                    
                    // Check if this is a label line
                    if trimmed.ends_with(':') && !trimmed.starts_with('.') {
                        let label_name = trimmed.trim_end_matches(':');
                        current_label = Some(label_name.to_string());
                        current_block_start_addr = label_addresses.get(label_name).copied();
                        instruction_offset = 0;
                    }
                    
                    // Calculate the address for this instruction
                    let line_addr = if trimmed.starts_with('.') || trimmed.is_empty() || trimmed.starts_with(';') {
                        // Directives, empty lines, comments don't have addresses
                        None
                    } else if let Some(base_addr) = current_block_start_addr {
                        // Instructions are at base + offset (each RV64 instruction is 4 bytes)
                        Some(base_addr + instruction_offset)
                    } else {
                        None
                    };
                    
                    // Check if this line contains the current PC
                    let is_current_line = line_addr.map_or(false, |addr| addr == current_pc);
                    
                    // Increment instruction offset for actual instructions (not directives/comments)
                    if !trimmed.is_empty() && !trimmed.starts_with('.') && !trimmed.starts_with(';') && !trimmed.ends_with(':') {
                        instruction_offset += 4;
                    }

                    // Background highlighting for current instruction
                    if is_current_line {
                        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_height),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            Color32::from_rgba_premultiplied(255, 215, 0, 40), // Yellow highlight
                        );

                        // Draw a left border indicator
                        ui.painter().line_segment(
                            [rect.left_top(), rect.left_bottom()],
                            Stroke::new(3.0, Color32::from_rgb(255, 215, 0)),
                        );
                    }

                    // Format the line with syntax highlighting
                    let text_color = if is_current_line {
                        Color32::WHITE
                    } else if trimmed.starts_with(';') {
                        // Comments
                        Color32::from_gray(80)
                    } else if trimmed.starts_with('.') {
                        // Directives
                        Color32::from_gray(120)
                    } else if trimmed.ends_with(':') {
                        // Labels
                        Color32::from_rgb(200, 180, 100)
                    } else {
                        // Regular instructions
                        Color32::from_gray(200)
                    };

                    let font_size = if is_current_line { 13.0 } else { 12.0 };

                    // Add line number
                    let line_num_str = format!("{:>4}", line_idx + 1);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(line_num_str)
                                .monospace()
                                .size(11.0)
                                .color(Color32::from_gray(80)),
                        );
                        ui.add_space(8.0);

                        // Show address if present
                        if let Some(addr) = line_addr {
                            let addr_color = if is_current_line {
                                Color32::from_rgb(255, 215, 0)
                            } else {
                                Color32::from_gray(120)
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

