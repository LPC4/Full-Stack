use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

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
            } else if let Some(rest) = line.strip_prefix("; Save callee-saved register s") {
                if let Some((reg_str, tail)) = rest.split_once(" at offset") {
                    if let (Ok(reg), Ok(offset)) =
                        (reg_str.trim().parse::<u8>(), tail.trim().parse::<usize>())
                    {
                        func.elements.push(StackElement {
                            kind: StackElementKind::SavedRegister { reg },
                            offset,
                        });
                    }
                }
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
                if parts.len() >= 4 && parts[2] == "sp," {
                    if let Ok(offset) = parts[3].parse::<usize>() {
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
            }

            if let Some(ref mut elem) = func.elements.last_mut() {
                if line.starts_with("sw ")
                    || line.starts_with("sd ")
                    || line.starts_with("fsw ")
                    || line.starts_with("fsd ")
                    || line.starts_with("sh ")
                    || line.starts_with("sb ")
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
    }

    if let Some(func) = current_func {
        results.push(func);
    }
    results
}

#[derive(Default)]
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
        let functions = parse_assembly(&state.asm);
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

            egui::ComboBox::from_id_salt("function_select")
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
}

fn draw_modern_function_stack(ui: &mut egui::Ui, func: &FunctionStack) {
    if func.frame_size == 0 {
        ui.label(
            RichText::new("Leaf function or fully registered. No stack frame allocated.")
                .italics()
                .weak(),
        );
        return;
    }

    ui.horizontal(|ui| {
        let frame_size = func.frame_size.max(1);
        let bar_width = 80.0;
        let bar_height = 350.0;

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

            painter.rect_filled(bar_rect, CornerRadius::same(4), Color32::from_gray(30));
            painter.rect_stroke(
                bar_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_gray(100)),
                StrokeKind::Middle,
            );

            let mut elements = func.elements.clone();
            elements.sort_by(|a, b| b.offset.cmp(&a.offset));

            for elem in &elements {
                let size = get_elem_size(elem);
                let top_offset = elem.offset + size;

                let y_bottom = bottom_y - (elem.offset as f32 * scale_y);
                let y_top = bottom_y - (top_offset as f32 * scale_y);

                let seg_rect = Rect::from_min_max(
                    Pos2::new(bar_rect.left(), y_top),
                    Pos2::new(bar_rect.right(), y_bottom),
                );

                let (fill_color, stroke_color) = match &elem.kind {
                    StackElementKind::ReturnAddress => (
                        Color32::from_rgba_premultiplied(200, 80, 80, 180),
                        Color32::from_rgb(255, 100, 100),
                    ),
                    StackElementKind::SavedRegister { .. } => (
                        Color32::from_rgba_premultiplied(180, 140, 60, 180),
                        Color32::from_rgb(255, 200, 100),
                    ),
                    StackElementKind::LocalVariable { .. } => (
                        Color32::from_rgba_premultiplied(60, 160, 100, 180),
                        Color32::from_rgb(100, 255, 150),
                    ),
                    StackElementKind::Parameter { .. } => (
                        Color32::from_rgba_premultiplied(120, 100, 200, 180),
                        Color32::from_rgb(180, 150, 255),
                    ),
                };

                painter.rect_filled(seg_rect, CornerRadius::same(2), fill_color);
                painter.rect_stroke(
                    seg_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0, stroke_color),
                    StrokeKind::Middle,
                );

                if ui.rect_contains_pointer(seg_rect) {
                    let response = ui.interact(seg_rect, ui.id().with(elem.offset), Sense::hover());

                    response.on_hover_ui(|ui| {
                        match &elem.kind {
                            StackElementKind::ReturnAddress => {
                                ui.label(
                                    RichText::new("Return Address (ra)")
                                        .strong()
                                        .color(stroke_color),
                                );
                            }
                            StackElementKind::SavedRegister { reg } => {
                                ui.label(
                                    RichText::new(format!("Saved Register (s{reg})"))
                                        .strong()
                                        .color(stroke_color),
                                );
                            }
                            StackElementKind::LocalVariable {
                                name,
                                type_name,
                                size,
                            } => {
                                ui.label(
                                    RichText::new(format!("Local Variable: {name}"))
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
                                ui.label(format!("Size: {size} bytes"));
                            }
                            StackElementKind::Parameter {
                                name,
                                type_name,
                                size,
                            } => {
                                ui.label(
                                    RichText::new(format!("Parameter: {name}"))
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
                                ui.label(format!("Size: {size} bytes"));
                            }
                        }
                        ui.separator();
                        ui.label(format!("Memory Offset: SP + 0x{:02X}", elem.offset));
                    });
                }
            }

            let font_id = FontId::monospace(10.0);

            let high_galley = ui.painter().layout_no_wrap(
                format!("SP + 0x{frame_size:X}"),
                font_id.clone(),
                Color32::LIGHT_GRAY,
            );
            painter.galley(
                Pos2::new(bar_rect.right() + 8.0, top_y - 4.0),
                high_galley,
                Color32::LIGHT_GRAY,
            );

            let low_galley = ui.painter().layout_no_wrap(
                "SP + 0x0".to_owned(),
                font_id.clone(),
                Color32::LIGHT_GRAY,
            );
            painter.galley(
                Pos2::new(bar_rect.right() + 8.0, bottom_y - 4.0),
                low_galley,
                Color32::LIGHT_GRAY,
            );
        }

        ui.add_space(20.0);

        ui.vertical(|ui| {
            ui.heading("Frame Variables");
            ui.add_space(8.0);

            egui::Grid::new("stack_elements_grid")
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

                    let mut elements = func.elements.clone();
                    elements.sort_by(|a, b| b.offset.cmp(&a.offset));

                    let mut current_offset = func.frame_size;

                    for elem in &elements {
                        let elem_size = get_elem_size(elem);
                        let top_of_elem = elem.offset + elem_size;

                        if current_offset > top_of_elem {
                            let gap = current_offset - top_of_elem;
                            ui.label(
                                RichText::new(format!("+0x{top_of_elem:02X}"))
                                    .monospace()
                                    .color(Color32::from_gray(120)),
                            );
                            ui.label(
                                RichText::new("Padding / Locals").color(Color32::from_gray(120)),
                            );
                            ui.label(RichText::new("-").color(Color32::from_gray(120)));
                            ui.label(RichText::new("-").color(Color32::from_gray(120)));
                            ui.label(
                                RichText::new(format!("{gap} bytes"))
                                    .color(Color32::from_gray(120)),
                            );
                            ui.end_row();
                        }

                        ui.label(RichText::new(format!("+0x{:02X}", elem.offset)).monospace());

                        match &elem.kind {
                            StackElementKind::ReturnAddress => {
                                ui.label(
                                    RichText::new("Return Addr")
                                        .color(Color32::from_rgb(255, 100, 100)),
                                );
                                ui.label("ra");
                                ui.label("-");
                                ui.label(format!("{elem_size} bytes"));
                            }
                            StackElementKind::SavedRegister { reg } => {
                                ui.label(
                                    RichText::new("Saved Reg")
                                        .color(Color32::from_rgb(255, 200, 100)),
                                );
                                ui.label(format!("s{reg}"));
                                ui.label("-");
                                ui.label(format!("{elem_size} bytes"));
                            }
                            StackElementKind::LocalVariable {
                                name, type_name, ..
                            } => {
                                ui.label(
                                    RichText::new("Local Var")
                                        .color(Color32::from_rgb(100, 255, 150)),
                                );
                                ui.label(name);
                                ui.label(if type_name.is_empty() { "?" } else { type_name });
                                ui.label(format!("{elem_size} bytes"));
                            }
                            StackElementKind::Parameter {
                                name, type_name, ..
                            } => {
                                ui.label(
                                    RichText::new("Parameter")
                                        .color(Color32::from_rgb(180, 150, 255)),
                                );
                                ui.label(name);
                                ui.label(if type_name.is_empty() { "?" } else { type_name });
                                ui.label(format!("{elem_size} bytes"));
                            }
                        }
                        ui.end_row();

                        current_offset = elem.offset;
                    }

                    if current_offset > 0 {
                        ui.label(
                            RichText::new("+0x00")
                                .monospace()
                                .color(Color32::from_gray(120)),
                        );
                        ui.label(RichText::new("Padding / Locals").color(Color32::from_gray(120)));
                        ui.label(RichText::new("-").color(Color32::from_gray(120)));
                        ui.label(RichText::new("-").color(Color32::from_gray(120)));
                        ui.label(
                            RichText::new(format!("{current_offset} bytes"))
                                .color(Color32::from_gray(120)),
                        );
                        ui.end_row();
                    }
                });
        });
    });
}

fn get_elem_size(elem: &StackElement) -> usize {
    match &elem.kind {
        StackElementKind::ReturnAddress => 8,
        StackElementKind::SavedRegister { .. } => 8,
        StackElementKind::LocalVariable { size, .. } => *size,
        StackElementKind::Parameter { size, .. } => *size,
    }
}
