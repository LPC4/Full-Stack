use crate::view::highlight_code;
use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Align, Frame, RichText, TextEdit, TextStyle};
use hll_to_ir::{Diagnostic, DiagnosticLevel};

#[derive(Default, Clone)]
pub struct SourceView;

/// Char index of the first character on `line` (1-based) within `text`, clamped to
/// the text length. Used to place the editor cursor when a diagnostic is clicked.
fn char_index_of_line_start(text: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut remaining = line - 1;
    let mut index = 0usize;
    for ch in text.chars() {
        if remaining == 0 {
            break;
        }
        index += 1;
        if ch == '\n' {
            remaining -= 1;
        }
    }
    index
}

impl CompilerView for SourceView {
    fn title(&self) -> &'static str {
        "Source"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        let mut source_code = catalog.get_selected_source();
        let is_stdlib = catalog
            .current_program()
            .map(|p| p.is_stdlib())
            .unwrap_or(false);
        let is_os = catalog
            .current_program()
            .map(|p| p.is_os())
            .unwrap_or(false);
        let is_readonly = is_stdlib || is_os;

        // Compact info chip for read-only programs
        if is_readonly {
            let (chip_label, hint) = if is_os {
                (
                    "os",
                    "read-only: select Kernel mode and open the Machine window to boot",
                )
            } else {
                (
                    "stdlib",
                    "read-only: compile to inspect tokens, AST, IR, and assembly",
                )
            };
            Frame::NONE
                .fill(theme.surface_alt)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(chip_label)
                                .small()
                                .strong()
                                .color(theme.text_dim),
                        );
                        ui.label(RichText::new("|").small().color(theme.border));
                        ui.label(RichText::new(hint).small().color(theme.text_dim));
                    });
                });
            ui.add_space(2.0);
        }

        let mut layouter = |ui: &egui::Ui, string: &dyn egui::TextBuffer, _wrap: f32| {
            let mut job = highlight_code(ui.style(), string.as_str());
            job.wrap.max_width = f32::INFINITY;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        // The diagnostics panel shows structured diagnostics when we have them, and
        // otherwise the plain error string (assembler/linker/setup failures).
        let has_structured = !state.diagnostics().is_empty();
        let has_panel = has_structured || state.error.is_some();
        let available = ui.available_size();
        let panel_height = if has_panel {
            (available.y * 0.32).clamp(80.0, 260.0)
        } else {
            0.0
        };
        let editor_height = (available.y - panel_height).max(50.0);

        let frame = Frame::NONE.fill(theme.panel).inner_margin(4.0);

        let panel_id = ui.id();
        let goto = state.goto_line.take();
        frame.show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt(panel_id.with("source_editor_scroll"))
                .auto_shrink([false; 2])
                .max_height(editor_height)
                .show(ui, |ui| {
                    let output = TextEdit::multiline(&mut source_code)
                        .font(TextStyle::Monospace)
                        .frame(Frame::NONE)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        .min_size(egui::vec2(available.x, editor_height))
                        .layouter(&mut layouter)
                        .interactive(!is_readonly)
                        .show(ui);

                    if !is_readonly && output.response.changed() {
                        catalog.replace_selected_source_with_history(source_code.clone());
                    }

                    // A clicked diagnostic moves the cursor to its line and scrolls
                    // it into view. Place the caret, then scroll to the galley row.
                    if let Some(line) = goto {
                        let idx = char_index_of_line_start(&source_code, line);
                        let ccursor = egui::text::CCursor::new(idx);
                        let mut edit_state = output.state.clone();
                        edit_state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                        edit_state.store(ui.ctx(), output.response.id);
                        let row_rect = output
                            .galley
                            .pos_from_cursor(ccursor)
                            .translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(row_rect, Some(Align::Center));
                        output.response.request_focus();
                    }
                });
        });

        if has_structured {
            ui.add_space(2.0);
            let diags: Vec<Diagnostic> = state.diagnostics().to_vec();
            render_structured_diagnostics(ui, panel_id, panel_height, &diags, &mut state.goto_line);
        } else if let Some(error_text) = &state.error {
            let error_text = error_text.clone();
            ui.add_space(2.0);
            render_plain_error(ui, panel_id, panel_height, &error_text);
        }
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

/// Render diagnostics with a clickable header, source line + caret, and note.
/// A click on a span-bearing diagnostic sets `goto_line` to jump the editor.
fn render_structured_diagnostics(
    ui: &mut egui::Ui,
    panel_id: egui::Id,
    panel_height: f32,
    diags: &[Diagnostic],
    goto_line: &mut Option<u32>,
) {
    let theme = ui_theme();
    let any_error = diags.iter().any(|d| d.level == DiagnosticLevel::Error);
    let accent = if any_error {
        theme.error
    } else {
        theme.warning
    };
    let panel_frame = theme.alert_frame(accent.gamma_multiply(0.12), accent);

    panel_frame.show(ui, |ui| {
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt(panel_id.with("diag_scroll"))
            .max_height((panel_height - 30.0).max(20.0))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for diag in diags {
                    let level_color = match diag.level {
                        DiagnosticLevel::Error => theme.error,
                        DiagnosticLevel::Warning => theme.warning,
                    };
                    let level_text = match diag.level {
                        DiagnosticLevel::Error => "error",
                        DiagnosticLevel::Warning => "warning",
                    };
                    let location = diag
                        .span
                        .as_ref()
                        .map(|s| format!(" {}:{}", s.line, s.col))
                        .unwrap_or_default();
                    let header = format!("{level_text}{location}  {}", diag.message);

                    // Clickable header (only navigable when a span is present).
                    let label = egui::Label::new(
                        RichText::new(header)
                            .monospace()
                            .strong()
                            .color(level_color),
                    );
                    let resp = if diag.span.is_some() {
                        ui.add(label.sense(egui::Sense::click()))
                            .on_hover_text("Click to jump to this line")
                    } else {
                        ui.add(label)
                    };
                    if resp.clicked()
                        && let Some(span) = &diag.span
                    {
                        *goto_line = Some(span.line);
                    }

                    // Source line + caret under the offending column.
                    if let Some(span) = &diag.span
                        && !span.source_line.is_empty()
                    {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.colored_label(
                                theme.info,
                                RichText::new(format!("{:>4} | {}", span.line, span.source_line))
                                    .monospace(),
                            );
                        });
                        let caret_pad = "       ".len() + span.col.saturating_sub(1) as usize;
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.colored_label(
                                level_color,
                                RichText::new(format!("{}^", " ".repeat(caret_pad))).monospace(),
                            );
                        });
                    }

                    if let Some(note) = &diag.note {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.colored_label(
                                theme.text_dim,
                                RichText::new(format!("note: {note}")).monospace(),
                            );
                        });
                    }
                    ui.add_space(4.0);
                }
            });
    });
}

/// Render a plain error string (assembler/linker/setup failure with no spans).
fn render_plain_error(ui: &mut egui::Ui, panel_id: egui::Id, panel_height: f32, error_text: &str) {
    let theme = ui_theme();
    let error_frame = theme.alert_frame(theme.error.gamma_multiply(0.12), theme.error);
    error_frame.show(ui, |ui| {
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt(panel_id.with("error_scroll"))
            .max_height((panel_height - 30.0).max(20.0))
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for line in error_text.lines() {
                    if line.trim().is_empty() {
                        ui.add_space(2.0);
                    } else if line.trim_start().starts_with("- ")
                        || line.trim_start().starts_with("  -")
                    {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.colored_label(theme.error, RichText::new(line.trim()).monospace());
                        });
                    } else if line.trim_start().starts_with("  |") {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.colored_label(theme.info, RichText::new(line).monospace());
                        });
                    } else {
                        ui.colored_label(theme.error, RichText::new(line).monospace().strong());
                    }
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::char_index_of_line_start;

    #[test]
    fn line_start_indices() {
        let text = "alpha\nbeta\ngamma";
        assert_eq!(char_index_of_line_start(text, 1), 0);
        assert_eq!(char_index_of_line_start(text, 2), 6); // after "alpha\n"
        assert_eq!(char_index_of_line_start(text, 3), 11); // after "beta\n"
    }

    #[test]
    fn line_start_clamps_and_handles_edges() {
        let text = "one\ntwo";
        assert_eq!(char_index_of_line_start(text, 0), 0);
        // A line past the end clamps to the text length.
        assert_eq!(char_index_of_line_start(text, 99), text.chars().count());
        assert_eq!(char_index_of_line_start("", 5), 0);
    }
}
