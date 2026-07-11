use super::{FullStackApp, Tool, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Pos2, Sense, Stroke, pos2, vec2};

impl FullStackApp {
    pub(super) fn activity_rail(&mut self, ui: &mut egui::Ui) {
        ui.add_space(metrics::PANEL_TOP_PADDING);
        for (tool, hint) in [
            (Tool::Files, "Explorer"),
            (Tool::Search, "Search in workspace"),
            (Tool::Run, "Run configurations"),
            (Tool::Inspect, "Inspect runtime and artifacts"),
        ] {
            if rail_button(ui, tool, hint, self.tool == tool).clicked() {
                self.tool = tool;
            }
        }
        // Pin the settings gear to the bottom of the rail, below the tools.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(metrics::PANEL_TOP_PADDING);
            if gear_button(ui, self.settings_open).clicked() {
                self.settings_open = !self.settings_open;
            }
        });
    }
}

/// The bottom-pinned settings gear, styled like a rail button.
fn gear_button(ui: &mut egui::Ui, active: bool) -> egui::Response {
    let (resp, rect, color) = rail_slot(ui, active);
    gear_icon(&ui.painter_at(rect), rect.center(), color);
    resp.on_hover_text(theme::mono(
        "Settings (Ctrl+,)",
        metrics::STATUS_TEXT_SIZE,
        theme::TEXT,
    ))
}

/// A hairline cog: a ring with eight short radial teeth and a hub.
fn gear_icon(painter: &egui::Painter, center: Pos2, color: egui::Color32) {
    let stroke = Stroke::new(1.2, color);
    let inner = 3.2;
    let outer = 5.6;
    painter.circle_stroke(center, inner + 1.4, stroke);
    for k in 0..8 {
        let angle = std::f32::consts::TAU * k as f32 / 8.0;
        let (s, c) = angle.sin_cos();
        painter.line_segment(
            [
                pos2(center.x + c * inner, center.y + s * inner),
                pos2(center.x + c * outer, center.y + s * outer),
            ],
            stroke,
        );
    }
}

/// Shared rail-button chrome (fill, accent bar, hover) for the tool buttons and the
/// settings gear. Returns the slot rect and icon color; the caller paints its glyph.
fn rail_slot(ui: &mut egui::Ui, active: bool) -> (egui::Response, egui::Rect, egui::Color32) {
    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(vec2(width, metrics::ACTIVITY_ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    let color = if active {
        theme::ELECTRIC
    } else if resp.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    if active {
        painter.rect_filled(rect, 0.0, theme::WELL);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, rect, color)
}

/// A full-width tool button. Selected buttons fill with the well tone they reveal
/// and carry an accent bar on the edge facing the tool window.
fn rail_button(ui: &mut egui::Ui, tool: Tool, hint: &str, active: bool) -> egui::Response {
    let (resp, rect, color) = rail_slot(ui, active);
    rail_icon(&ui.painter_at(rect), rect.center(), tool, color);
    resp.on_hover_text(theme::mono(hint, metrics::STATUS_TEXT_SIZE, theme::TEXT))
}

/// Hairline vector icons matching the instrument identity: a file tree for the
/// explorer, a play triangle for run, a chip outline for the inspector.
fn rail_icon(painter: &egui::Painter, center: Pos2, tool: Tool, color: egui::Color32) {
    let stroke = Stroke::new(1.2, color);
    match tool {
        Tool::Files => {
            // A folder: body with a small tab on the top-left.
            let (w, h) = (14.0, 10.0);
            let tl = pos2(center.x - w / 2.0, center.y - h / 2.0);
            let br = pos2(center.x + w / 2.0, center.y + h / 2.0);
            let tab_w = 5.0;
            let tab_h = 3.0;
            let points = [
                pos2(tl.x, br.y),
                pos2(tl.x, tl.y - tab_h + 1.0),
                pos2(tl.x + tab_w, tl.y - tab_h + 1.0),
                pos2(tl.x + tab_w + 2.0, tl.y),
                pos2(br.x, tl.y),
                pos2(br.x, br.y),
                pos2(tl.x, br.y),
            ];
            for segment in points.windows(2) {
                painter.line_segment([segment[0], segment[1]], stroke);
            }
        }
        Tool::Search => {
            // A magnifier: lens circle with a handle toward the corner.
            let r = 4.5;
            let lens = pos2(center.x - 1.5, center.y - 1.5);
            painter.circle_stroke(lens, r, stroke);
            let dir = std::f32::consts::FRAC_1_SQRT_2;
            painter.line_segment(
                [
                    pos2(lens.x + r * dir, lens.y + r * dir),
                    pos2(center.x + 5.5, center.y + 5.5),
                ],
                stroke,
            );
        }
        Tool::Run => {
            let r = 6.5;
            let points = [
                pos2(center.x - r * 0.6, center.y - r),
                pos2(center.x + r * 0.9, center.y),
                pos2(center.x - r * 0.6, center.y + r),
                pos2(center.x - r * 0.6, center.y - r),
            ];
            for segment in points.windows(2) {
                painter.line_segment([segment[0], segment[1]], stroke);
            }
        }
        Tool::Inspect => {
            // A chip: die outline plus two stub pins per side.
            let half = 5.0;
            let die = egui::Rect::from_center_size(center, vec2(half * 2.0, half * 2.0));
            painter.rect_stroke(die, 0.0, stroke, egui::StrokeKind::Middle);
            let pin = 3.0;
            for offset in [-2.5f32, 2.5] {
                painter.line_segment(
                    [
                        pos2(die.left() - pin, center.y + offset),
                        pos2(die.left(), center.y + offset),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(die.right(), center.y + offset),
                        pos2(die.right() + pin, center.y + offset),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(center.x + offset, die.top() - pin),
                        pos2(center.x + offset, die.top()),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(center.x + offset, die.bottom()),
                        pos2(center.x + offset, die.bottom() + pin),
                    ],
                    stroke,
                );
            }
        }
    }
}
