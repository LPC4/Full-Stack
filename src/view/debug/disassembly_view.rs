//! Interactive disassembly view that tracks the current PC during debugging.

use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Align, RichText, ScrollArea, Stroke, Ui};

#[derive(Clone, Default)]
pub struct DisassemblyView {
    symbol_search: String,
    show_symbols: bool,
}

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
        let follow_pc = state.disasm_follow_pc;

        let asm_text: &str = state.linked_asm();

        if asm_text.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No assembly available").weak());
            });
            return;
        }

        let lines: Vec<&str> = asm_text.lines().collect();
        let mut line_to_address: Vec<Option<u64>> = vec![None; lines.len()];
        let mut address_to_line: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();

        let mut block_base: Option<u64> = None;
        let mut insn_offset: u64 = 0;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.ends_with(':') && !trimmed.starts_with('.') {
                let label = trimmed.trim_end_matches(':');
                block_base = session.symbols.get(label).copied();
                insn_offset = 0;
                if let Some(addr) = block_base {
                    line_to_address[idx] = Some(addr);
                    address_to_line.entry(addr).or_insert(idx);
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('.') && !trimmed.starts_with(';')
            {
                if let Some(base) = block_base {
                    let addr = base + insn_offset;
                    line_to_address[idx] = Some(addr);
                    address_to_line.entry(addr).or_insert(idx);
                    insn_offset += 4;
                }
            }
        }

        let pc_in_code = address_to_line.contains_key(&current_pc);

        let fn_context: String = {
            let nearest = session
                .symbols
                .iter()
                .filter(|&(_, &a)| a <= current_pc)
                .max_by_key(|&(_, &a)| a);
            match nearest {
                Some((name, &addr)) => {
                    let offset = current_pc - addr;
                    if offset < 0x10_0000 {
                        if offset == 0 {
                            name.clone()
                        } else {
                            format!("{}  +{:#x}", name, offset)
                        }
                    } else {
                        format!("pc = {:#010x}  (firmware)", current_pc)
                    }
                }
                None => format!("pc = {:#010x}  (firmware)", current_pc),
            }
        };

        let scroll_to: Option<u64> = if follow_pc && pc_in_code {
            Some(current_pc)
        } else {
            None
        };

        ui.horizontal(|ui| {
            if pc_in_code {
                ui.label(
                    RichText::new(&fn_context)
                        .monospace()
                        .size(11.0)
                        .color(theme.info),
                );
            } else {
                ui.label(
                    RichText::new(format!("pc = {:#010x}  (firmware)", current_pc))
                        .monospace()
                        .size(11.0)
                        .color(theme.warning),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Symbols")
                    .on_hover_text("Jump to any symbol")
                    .clicked()
                {
                    self.show_symbols = !self.show_symbols;
                }
            });
        });

        ui.separator();

        if self.show_symbols {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter:").size(11.0).color(theme.text_dim));
                ui.add(
                    egui::TextEdit::singleline(&mut self.symbol_search)
                        .desired_width(140.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("symbol name"),
                );
                if ui.small_button("Clear").clicked() {
                    self.symbol_search.clear();
                }
                if ui.small_button("Close").clicked() {
                    self.show_symbols = false;
                }
            });

            let search_lower = self.symbol_search.to_lowercase();
            let mut sorted_syms: Vec<(&String, u64)> =
                session.symbols.iter().map(|(n, &a)| (n, a)).collect();
            sorted_syms.sort_by_key(|(_, a)| *a);

            let current_fn_addr = sorted_syms
                .iter()
                .filter(|(_, a)| *a <= current_pc)
                .max_by_key(|(_, a)| *a)
                .map(|(_, a)| *a);

            ScrollArea::vertical()
                .id_salt("disasm_sym_list")
                .max_height(130.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (name, addr) in &sorted_syms {
                        if !search_lower.is_empty() && !name.to_lowercase().contains(&search_lower)
                        {
                            continue;
                        }
                        let is_active = pc_in_code && Some(*addr) == current_fn_addr;
                        let resp = ui.selectable_label(
                            is_active,
                            RichText::new(format!("{:#010x}  {}", addr, name))
                                .monospace()
                                .size(11.0),
                        );
                        if resp.clicked() {
                            self.show_symbols = false;
                            // Handled below via address_to_line lookup - store for next frame.
                            // (We can't scroll from inside this closure easily, so close the
                            // panel; the user can use Follow PC to re-centre.)
                            let _ = addr;
                        }
                        if is_active {
                            resp.scroll_to_me(Some(Align::Center));
                        }
                    }
                });

            ui.separator();
        }

        ScrollArea::vertical()
            .id_salt("disasm_main")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                for (idx, line) in lines.iter().enumerate() {
                    let trimmed = line.trim();
                    let line_addr = line_to_address[idx];
                    let is_current = line_addr.map_or(false, |a| a == current_pc);

                    // Function-boundary divider before known symbol labels
                    if idx > 0 && trimmed.ends_with(':') && !trimmed.starts_with('.') {
                        let label = trimmed.trim_end_matches(':');
                        if session.symbols.contains_key(label) {
                            ui.add_space(6.0);
                            let (hr, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 1.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().hline(
                                hr.x_range(),
                                hr.center().y,
                                Stroke::new(1.0, theme.border_soft),
                            );
                            ui.add_space(2.0);
                        }
                    }

                    let bg_slot = ui.painter().add(egui::Shape::Noop);

                    let row_resp = ui
                        .horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:>5}", idx + 1))
                                    .monospace()
                                    .size(11.0)
                                    .color(theme.text_dim),
                            );
                            ui.add_space(4.0);

                            match line_addr {
                                Some(addr) => {
                                    let addr_color = if is_current {
                                        theme.highlight
                                    } else {
                                        theme.text_dim
                                    };
                                    ui.label(
                                        RichText::new(format!("{:#010x}", addr))
                                            .monospace()
                                            .size(11.0)
                                            .color(addr_color),
                                    );
                                }
                                None => {
                                    ui.label(RichText::new("            ").monospace().size(11.0));
                                }
                            }
                            ui.add_space(8.0);

                            let text_color = if is_current {
                                theme.text
                            } else if trimmed.starts_with(';') {
                                theme.text_dim
                            } else if trimmed.starts_with('.') {
                                theme.syntax.directive
                            } else if trimmed.ends_with(':') {
                                theme.syntax.label
                            } else {
                                theme.text_soft
                            };
                            let font_size = if is_current { 13.0 } else { 12.0 };
                            ui.label(
                                RichText::new(trimmed)
                                    .monospace()
                                    .size(font_size)
                                    .color(text_color),
                            );
                        })
                        .response;

                    if is_current {
                        let rect = row_resp.rect;
                        ui.painter().set(
                            bg_slot,
                            egui::Shape::rect_filled(
                                rect,
                                2.0,
                                theme.highlight.gamma_multiply(0.12),
                            ),
                        );
                        ui.painter().line_segment(
                            [rect.left_top(), rect.left_bottom()],
                            Stroke::new(3.0, theme.highlight),
                        );
                    }

                    if scroll_to.zip(line_addr).map_or(false, |(t, a)| t == a) {
                        row_resp.scroll_to_me(Some(Align::Center));
                    }
                }
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
