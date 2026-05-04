use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use eframe::epaint::PathStroke;
use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct CfgView;

struct BasicBlock {
    label: String,
    instructions: Vec<String>,
    targets: Vec<String>,
}

fn parse_blocks(asm: &str) -> Vec<BasicBlock> {
    let mut blocks = Vec::new();
    let mut current_block = BasicBlock {
        label: "entry".to_string(),
        instructions: vec![],
        targets: vec![],
    };

    let branch_mnemonics = [
        "j", "jal", "beq", "bne", "blt", "bge", "bltu", "bgeu", "beqz", "bnez",
    ];

    for line in asm.lines() {
        let trimmed = line.split(';').next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.ends_with(':') {
            if !current_block.instructions.is_empty() || current_block.label != "entry" {
                blocks.push(current_block);
            }
            current_block = BasicBlock {
                label: trimmed.trim_end_matches(':').to_string(),
                instructions: vec![],
                targets: vec![],
            };
        } else {
            current_block.instructions.push(trimmed.to_string());

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if !parts.is_empty() && branch_mnemonics.contains(&parts[0]) {
                if let Some(target) = parts.last() {
                    current_block.targets.push(target.to_string());
                }
            }
        }
    }
    if !current_block.instructions.is_empty() || current_block.label != "entry" {
        blocks.push(current_block);
    }
    blocks
}

impl CompilerView for CfgView {
    fn title(&self) -> &'static str {
        "CFG"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        if state.asm.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Compile code to generate CFG.").weak());
            });
            return;
        }

        let blocks = parse_blocks(&state.asm);

        egui::ScrollArea::both()
            .id_salt(ui.id())
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                draw_cfg(ui, &blocks);
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

/// Draw a simple arrowhead at `tip` pointing in direction `dir`.
/// `dir` should be a unit vector pointing **into** the target.
fn draw_arrowhead(painter: &egui::Painter, tip: Pos2, dir: Vec2, color: Color32) {
    let head_size = 8.0;
    let half_width = 5.0;

    // Compute left/right perpendicular vectors
    let perp = Vec2::new(-dir.y, dir.x) * half_width;
    let back = dir * (-head_size);

    let point_left = tip + back + perp;
    let point_right = tip + back - perp;

    painter.line_segment([tip, point_left], Stroke::new(1.5, color));
    painter.line_segment([tip, point_right], Stroke::new(1.5, color));
}

fn draw_cfg(ui: &mut egui::Ui, blocks: &[BasicBlock]) {
    let theme = ui_theme();
    // Define the fixed widths and margins of our content
    let margin_left = 120.0; // Space for the curved branch arrows
    let block_width = 250.0;
    let margin_right = 20.0; // Small buffer on the right
    let content_width = margin_left + block_width + margin_right;

    let line_height = 16.0;
    let block_spacing = 40.0;
    let top_margin = 20.0;
    let bottom_margin = 20.0;

    let displayed_lines = |block: &BasicBlock| -> usize {
        let len = block.instructions.len();
        if len > 5 {
            6 // 5 instructions + "... +X more"
        } else {
            len.max(1)
        }
    };

    // Calculate total height needed
    let mut total_height = top_margin;
    for block in blocks {
        let lines = displayed_lines(block);
        let block_height = 30.0 + lines as f32 * line_height;
        total_height += block_height + block_spacing;
    }
    total_height += bottom_margin;

    // Fetch available width, defaulting to content_width if ScrollArea passes infinity
    let available_width = if ui.available_width().is_finite() {
        ui.available_width()
    } else {
        content_width
    }
    .max(content_width);

    let (rect, _resp) = ui.allocate_exact_size(
        Vec2::new(available_width, total_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    // Centering Math: Calculate padding to push the graph into the middle
    let horizontal_offset = (available_width - content_width).max(0.0) / 2.0;
    let block_start_x = rect.left() + horizontal_offset + margin_left;

    // Draw all blocks and record their rectangles
    let mut block_rects = HashMap::new();
    let mut current_y = rect.top() + top_margin;

    for block in blocks {
        let lines = displayed_lines(block);
        let block_height = 30.0 + lines as f32 * line_height;

        let b_rect = Rect::from_min_size(
            Pos2::new(block_start_x, current_y), // Now uses centered X coordinate
            Vec2::new(block_width, block_height),
        );

        painter.rect_filled(b_rect, CornerRadius::same(6), theme.panel_alt);
        painter.rect_stroke(
            b_rect,
            CornerRadius::same(6),
            Stroke::new(1.0, theme.border_soft),
            StrokeKind::Middle,
        );

        painter.text(
            b_rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            &block.label,
            egui::FontId::monospace(14.0),
            theme.memory.link,
        );
        painter.line_segment(
            [
                b_rect.left_top() + Vec2::new(0.0, 25.0),
                b_rect.right_top() + Vec2::new(0.0, 25.0),
            ],
            Stroke::new(1.0, theme.border_soft),
        );

        let max_lines = block.instructions.len().min(5);
        for (i, inst) in block.instructions.iter().take(max_lines).enumerate() {
            painter.text(
                b_rect.left_top() + Vec2::new(8.0, 30.0 + (i as f32 * line_height)),
                egui::Align2::LEFT_TOP,
                inst,
                egui::FontId::monospace(12.0),
                theme.text_soft,
            );
        }
        if block.instructions.len() > 5 {
            painter.text(
                b_rect.left_top() + Vec2::new(8.0, 30.0 + (5.0 * line_height)),
                egui::Align2::LEFT_TOP,
                format!("... +{} more", block.instructions.len() - 5),
                egui::FontId::monospace(12.0),
                theme.text_dim,
            );
        }

        block_rects.insert(block.label.clone(), b_rect);
        current_y += block_height + block_spacing;
    }

    // Draw explicit branch edges (Bézier curves to the left)
    for block in blocks {
        if let Some(src_rect) = block_rects.get(&block.label) {
            let start_pos = src_rect.left_center();

            for target in &block.targets {
                if let Some(dst_rect) = block_rects.get(target) {
                    let end_pos = dst_rect.left_center();

                    let diff_y = end_pos.y - start_pos.y;
                    let control_dist = diff_y.abs().clamp(30.0, 100.0);

                    // Control points: outward to the left, then back in
                    let c1 = start_pos - Vec2::new(control_dist, 0.0);
                    let c2 = end_pos - Vec2::new(control_dist, 0.0);

                    // Draw the Bézier curve
                    let shape = egui::epaint::CubicBezierShape {
                        points: [start_pos, c1, c2, end_pos],
                        closed: false,
                        fill: Color32::TRANSPARENT,
                        stroke: PathStroke::new(1.5, theme.memory.link),
                    };
                    painter.add(shape);

                    // Tangential arrowhead at endpoint
                    // Derivative at t=1: 3*(P3 - P2)
                    let tangent = (end_pos - c2) * 3.0;
                    let dir = tangent.normalized(); // points into the block (rightwards)
                    draw_arrowhead(&painter, end_pos, dir, theme.memory.link);
                }
            }
        }
    }

    // Fall-through arrows (straight down) between consecutive blocks
    let unconditional_jumps: &[&str] = &["j", "jal", "jr", "ret", "tail", "jump"];

    for i in 0..blocks.len() - 1 {
        let this_block = &blocks[i];
        let next_block = &blocks[i + 1];

        let last_inst = this_block
            .instructions
            .last()
            .map(|s| s.split_whitespace().next().unwrap_or(""))
            .unwrap_or("");

        let falls_through = !unconditional_jumps.contains(&last_inst);

        if falls_through {
            let src_rect = block_rects[&this_block.label];
            let dst_rect = block_rects[&next_block.label];

            let start = src_rect.center_bottom();
            let end = dst_rect.center_top();

            let arrow_color = theme.success;
            painter.line_segment([start, end], Stroke::new(1.5, arrow_color));

            // Downward direction
            draw_arrowhead(&painter, end, Vec2::new(0.0, 1.0), arrow_color);
        }
    }
}
