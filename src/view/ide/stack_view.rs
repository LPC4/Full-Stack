use crate::view::{CompilationState, CompilerView, ProgramCatalog, StackPalette, ui_theme};
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
enum StackElementKind {
    ReturnAddress,
    SavedRegister {
        reg: u8,
    },
    LocalVariable {
        name: String,
        type_name: String,
        size: usize,
    },
    Parameter {
        name: String,
        type_name: String,
        size: usize,
    },
}

#[derive(Debug, Clone)]
struct StackElement {
    kind: StackElementKind,
    offset: usize,
}

#[derive(Debug, Clone)]
struct FunctionStack {
    name: String,
    frame_size: usize,
    elements: Vec<StackElement>,
}

fn parse_assembly(asm: &str) -> Vec<FunctionStack> {
    let mut results = Vec::new();
    let mut current_func: Option<FunctionStack> = None;
    let mut pending_local_var: Option<(String, String)> = None;
    let mut pending_param: Option<(String, String)> = None;

    for line in asm.lines() {
        let line = line.trim();

        if line.starts_with("; Function:") {
            if let Some(func) = current_func.take() {
                results.push(func);
            }
            let name = line.trim_start_matches("; Function:").trim().to_owned();
            current_func = Some(FunctionStack {
                name,
                frame_size: 0,
                elements: vec![],
            });
            pending_local_var = None;
            pending_param = None;
        }

        if let Some(ref mut func) = current_func {
            if let Some(size) = line
                .strip_prefix("; Allocate stack frame:")
                .and_then(|s| s.trim().strip_suffix(" bytes"))
                .and_then(|s| s.parse().ok())
            {
                func.frame_size = size;
            } else if let Some(offset) = line
                .strip_prefix("; Save return address (ra) at offset")
                .and_then(|s| s.trim().parse().ok())
            {
                func.elements.push(StackElement {
                    kind: StackElementKind::ReturnAddress,
                    offset,
                });
            } else if let Some(rest) = line.strip_prefix("; Save callee-saved register s")
                && let Some((reg_str, tail)) = rest.split_once(" at offset")
                && let (Ok(reg), Ok(offset)) =
                    (reg_str.trim().parse::<u8>(), tail.trim().parse::<usize>())
            {
                func.elements.push(StackElement {
                    kind: StackElementKind::SavedRegister { reg },
                    offset,
                });
            }

            if let Some(stripped) = line.strip_prefix("; local var:") {
                let name = stripped.trim().to_owned();
                pending_local_var = Some((name, String::new()));
                pending_param = None;
            } else if let Some(stripped) = line.strip_prefix("; bind parameter:") {
                let name = stripped.trim().to_owned();
                pending_param = Some((name, String::new()));
                pending_local_var = None;
            }

            if (pending_local_var.is_some() || pending_param.is_some()) && line.starts_with("addi")
            {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4
                    && parts[2] == "sp,"
                    && let Ok(offset) = parts[3].parse::<usize>()
                {
                    if let Some((var_name, _)) = pending_local_var.take() {
                        func.elements.push(StackElement {
                            kind: StackElementKind::LocalVariable {
                                name: var_name,
                                type_name: String::new(),
                                size: 0,
                            },
                            offset,
                        });
                    } else if let Some((param_name, _)) = pending_param.take() {
                        func.elements.push(StackElement {
                            kind: StackElementKind::Parameter {
                                name: param_name,
                                type_name: String::new(),
                                size: 0,
                            },
                            offset,
                        });
                    }
                }
            }

            if let Some(ref mut elem) = func.elements.last_mut()
                && (line.starts_with("sw ")
                    || line.starts_with("sd ")
                    || line.starts_with("fsw ")
                    || line.starts_with("fsd ")
                    || line.starts_with("sh ")
                    || line.starts_with("sb "))
            {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let store_op = parts[0];
                    let (type_name, size) = match store_op {
                        "sw" => ("i32", 4),
                        "sd" => ("i64", 8),
                        "fsw" => ("f32", 4),
                        "fsd" => ("f64", 8),
                        "sh" => ("i16", 2),
                        "sb" => ("i8", 1),
                        _ => ("", 0),
                    };

                    if size > 0 {
                        match &mut elem.kind {
                            StackElementKind::LocalVariable {
                                type_name: tn,
                                size: sz,
                                ..
                            } => {
                                if *sz == 0 {
                                    *tn = type_name.to_owned();
                                    *sz = size;
                                }
                            }
                            StackElementKind::Parameter {
                                type_name: tn,
                                size: sz,
                                ..
                            } => {
                                if *sz == 0 {
                                    *tn = type_name.to_owned();
                                    *sz = size;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if let Some(func) = current_func {
        results.push(func);
    }
    results
}

#[derive(Default, Clone)]
pub struct StackView {
    selected_function_index: usize,
}

impl CompilerView for StackView {
    fn title(&self) -> &'static str {
        "Stack"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let functions = parse_assembly(state.asm());
        if functions.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No stack frames generated yet.").weak());
            });
            return;
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Inspect Function:").strong());

            if self.selected_function_index >= functions.len() {
                self.selected_function_index = 0;
            }

            egui::ComboBox::from_id_salt(ui.id().with("function_select"))
                .selected_text(&functions[self.selected_function_index].name)
                .show_ui(ui, |ui| {
                    for (i, func) in functions.iter().enumerate() {
                        if ui
                            .selectable_label(self.selected_function_index == i, &func.name)
                            .clicked()
                        {
                            self.selected_function_index = i;
                        }
                    }
                });
        });

        ui.separator();

        let func = &functions[self.selected_function_index];

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.add_space(8.0);
                draw_modern_function_stack(ui, func);
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

fn draw_modern_function_stack(ui: &mut egui::Ui, func: &FunctionStack) {
    let theme = ui_theme();
    let palette = theme.stack;
    if func.frame_size == 0 {
        ui.label(
            RichText::new("Leaf function or fully registered. No stack frame allocated.")
                .italics()
                .weak(),
        );
        return;
    }

    let bar_width = (ui.available_width() * 0.12).clamp(50.0, 100.0);
    let bar_height = (ui.available_height() - 80.0).clamp(100.0, 480.0);

    ui.horizontal(|ui| {
        let frame_size = func.frame_size.max(1);

        let (rect, _response) = ui.allocate_exact_size(
            Vec2::new(bar_width + 80.0, bar_height + 30.0),
            Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            let top_y = rect.top() + 15.0;
            let bottom_y = top_y + bar_height;
            let scale_y = bar_height / frame_size as f32;

            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left() + 10.0, top_y),
                Pos2::new(rect.left() + 10.0 + bar_width, bottom_y),
            );

            painter.rect_filled(bar_rect, CornerRadius::same(4), palette.background);
            painter.rect_stroke(
                bar_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, palette.frame),
                StrokeKind::Middle,
            );

            let rows = coalesce_rows(&func.elements);

            for row in &rows {
                let top_offset = row.offset + row.size;

                let y_bottom = bottom_y - (row.offset as f32 * scale_y);
                let y_top = bottom_y - (top_offset as f32 * scale_y);

                let seg_rect = Rect::from_min_max(
                    Pos2::new(bar_rect.left(), y_top),
                    Pos2::new(bar_rect.right(), y_bottom),
                );

                let stroke_color = row_color(&row.kind, &palette);
                let fill_color = stroke_color.gamma_multiply(0.25);

                painter.rect_filled(seg_rect, CornerRadius::same(2), fill_color);
                painter.rect_stroke(
                    seg_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0, stroke_color),
                    StrokeKind::Middle,
                );

                if ui.rect_contains_pointer(seg_rect) {
                    let response = ui.interact(seg_rect, ui.id().with(row.offset), Sense::hover());

                    response.on_hover_ui(|ui| {
                        match &row.kind {
                            RowKind::ReturnAddress => {
                                ui.label(
                                    RichText::new("Return Address (ra)")
                                        .strong()
                                        .color(stroke_color),
                                );
                            }
                            RowKind::SavedRegister { reg } => {
                                ui.label(
                                    RichText::new(format!("Saved Register (s{reg})"))
                                        .strong()
                                        .color(stroke_color),
                                );
                            }
                            RowKind::Variables {
                                names,
                                type_name,
                                is_param,
                            } => {
                                let title = if names.len() > 1 {
                                    "Shared slot"
                                } else if *is_param {
                                    "Parameter"
                                } else {
                                    "Local Variable"
                                };
                                ui.label(
                                    RichText::new(format!("{title}: {}", join_names(names)))
                                        .strong()
                                        .color(stroke_color),
                                );
                                ui.label(format!(
                                    "Type: {}",
                                    if type_name.is_empty() {
                                        "Unknown"
                                    } else {
                                        type_name
                                    }
                                ));
                                ui.label(format!("Size: {} bytes", row.size));
                                if names.len() > 1 {
                                    ui.label(format!("Slot coalesces {} variables", names.len()));
                                }
                            }
                        }
                        ui.separator();
                        ui.label(format!("Memory Offset: SP + 0x{:02X}", row.offset));
                    });
                }
            }

            let font_id = FontId::monospace(10.0);

            let high_galley = ui.painter().layout_no_wrap(
                format!("SP + 0x{frame_size:X}"),
                font_id.clone(),
                palette.label_text,
            );
            painter.galley(
                Pos2::new(bar_rect.right() + 8.0, top_y - 4.0),
                high_galley,
                palette.label_text,
            );

            let low_galley = ui.painter().layout_no_wrap(
                "SP + 0x0".to_owned(),
                font_id.clone(),
                palette.label_text,
            );
            painter.galley(
                Pos2::new(bar_rect.right() + 8.0, bottom_y - 4.0),
                low_galley,
                palette.label_text,
            );
        }

        ui.add_space(20.0);

        ui.vertical(|ui| {
            ui.heading("Frame Variables");
            ui.add_space(8.0);

            egui::Grid::new(ui.id().with("stack_elements_grid"))
                .striped(true)
                .min_col_width(70.0)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Offset").strong());
                    ui.label(RichText::new("Kind").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Type").strong());
                    ui.label(RichText::new("Size").strong());
                    ui.end_row();

                    let rows = coalesce_rows(&func.elements);

                    let mut current_offset = func.frame_size;
                    let mut shown_saved = false;
                    let mut shown_locals = false;

                    for row in &rows {
                        let top_of_row = row.offset + row.size;

                        // Header when crossing into the saved-register or locals zone.
                        if row.is_saved() && !shown_saved {
                            region_header(ui, "Saved Registers", palette.saved_register);
                            shown_saved = true;
                        } else if !row.is_saved() && !shown_locals {
                            region_header(ui, "Locals", palette.local_variable);
                            shown_locals = true;
                        }

                        if current_offset > top_of_row {
                            let gap = current_offset - top_of_row;
                            ui.label(
                                RichText::new(format!("+0x{top_of_row:02X}"))
                                    .monospace()
                                    .color(palette.dim_text),
                            );
                            ui.label(RichText::new("Padding").color(palette.dim_text));
                            ui.label(RichText::new("-").color(palette.dim_text));
                            ui.label(RichText::new("-").color(palette.dim_text));
                            ui.label(RichText::new(format!("{gap} bytes")).color(palette.dim_text));
                            ui.end_row();
                        }

                        ui.label(RichText::new(format!("+0x{:02X}", row.offset)).monospace());

                        match &row.kind {
                            RowKind::ReturnAddress => {
                                ui.label(
                                    RichText::new("Return Addr").color(palette.return_address),
                                );
                                ui.label("ra");
                                ui.label("-");
                                ui.label(format!("{} bytes", row.size));
                            }
                            RowKind::SavedRegister { reg } => {
                                ui.label(RichText::new("Saved Reg").color(palette.saved_register));
                                ui.label(format!("s{reg}"));
                                ui.label("-");
                                ui.label(format!("{} bytes", row.size));
                            }
                            RowKind::Variables {
                                names,
                                type_name,
                                is_param,
                            } => {
                                let (kind_label, color) = if names.len() > 1 {
                                    ("Shared", palette.local_variable)
                                } else if *is_param {
                                    ("Parameter", palette.parameter)
                                } else {
                                    ("Local Var", palette.local_variable)
                                };
                                ui.label(RichText::new(kind_label).color(color));
                                ui.label(join_names(names));
                                ui.label(if type_name.is_empty() { "?" } else { type_name });
                                ui.label(format!("{} bytes", row.size));
                            }
                        }
                        ui.end_row();

                        current_offset = row.offset;
                    }

                    if current_offset > 0 {
                        ui.label(RichText::new("+0x00").monospace().color(palette.dim_text));
                        ui.label(RichText::new("Padding / Locals").color(palette.dim_text));
                        ui.label(RichText::new("-").color(palette.dim_text));
                        ui.label(RichText::new("-").color(palette.dim_text));
                        ui.label(
                            RichText::new(format!("{current_offset} bytes"))
                                .color(palette.dim_text),
                        );
                        ui.end_row();
                    }
                });
        });
    });
}

// A full-width section label inside the frame-variables grid.
fn region_header(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).strong().color(color));
    ui.label("");
    ui.label("");
    ui.label("");
    ui.label("");
    ui.end_row();
}

// One rendered row per stack slot. Slot coloring maps several disjoint locals
// to the same offset, so variables sharing a slot are merged into one row.
enum RowKind {
    ReturnAddress,
    SavedRegister {
        reg: u8,
    },
    Variables {
        names: Vec<String>,
        type_name: String,
        is_param: bool,
    },
}

struct RenderRow {
    offset: usize,
    size: usize,
    kind: RowKind,
}

impl RenderRow {
    fn is_saved(&self) -> bool {
        matches!(
            self.kind,
            RowKind::ReturnAddress | RowKind::SavedRegister { .. }
        )
    }
}

// Merge raw elements into one row per offset, sorted top-of-frame first.
fn coalesce_rows(elements: &[StackElement]) -> Vec<RenderRow> {
    let mut by_offset: BTreeMap<usize, RenderRow> = BTreeMap::new();
    for elem in elements {
        match &elem.kind {
            StackElementKind::ReturnAddress => {
                by_offset.insert(
                    elem.offset,
                    RenderRow {
                        offset: elem.offset,
                        size: 8,
                        kind: RowKind::ReturnAddress,
                    },
                );
            }
            StackElementKind::SavedRegister { reg } => {
                by_offset.insert(
                    elem.offset,
                    RenderRow {
                        offset: elem.offset,
                        size: 8,
                        kind: RowKind::SavedRegister { reg: *reg },
                    },
                );
            }
            StackElementKind::LocalVariable {
                name,
                type_name,
                size,
            }
            | StackElementKind::Parameter {
                name,
                type_name,
                size,
            } => {
                let is_param = matches!(elem.kind, StackElementKind::Parameter { .. });
                let row = by_offset.entry(elem.offset).or_insert_with(|| RenderRow {
                    offset: elem.offset,
                    size: 0,
                    kind: RowKind::Variables {
                        names: Vec::new(),
                        type_name: String::new(),
                        is_param,
                    },
                });
                row.size = row.size.max(*size);
                if let RowKind::Variables {
                    names,
                    type_name: ty,
                    is_param: ip,
                } = &mut row.kind
                {
                    if !name.is_empty() && !names.iter().any(|n| n == name) {
                        names.push(name.clone());
                    }
                    if ty.is_empty() {
                        *ty = type_name.clone();
                    } else if !type_name.is_empty() && ty != type_name {
                        *ty = "mixed".to_owned();
                    }
                    *ip = *ip && is_param;
                }
            }
        }
    }
    let mut rows: Vec<RenderRow> = by_offset.into_values().collect();
    rows.sort_by(|a, b| b.offset.cmp(&a.offset));
    rows
}

// Accent color for a row, by kind. Coalesced multi-var slots read as locals.
fn row_color(kind: &RowKind, palette: &StackPalette) -> Color32 {
    match kind {
        RowKind::ReturnAddress => palette.return_address,
        RowKind::SavedRegister { .. } => palette.saved_register,
        RowKind::Variables {
            is_param, names, ..
        } if *is_param && names.len() == 1 => palette.parameter,
        RowKind::Variables { .. } => palette.local_variable,
    }
}

// Join shared-slot names, capping the list so the cell stays readable.
fn join_names(names: &[String]) -> String {
    if names.is_empty() {
        "?".to_owned()
    } else if names.len() <= 4 {
        names.join(", ")
    } else {
        format!("{}, +{} more", names[..3].join(", "), names.len() - 3)
    }
}
