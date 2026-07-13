use super::{FullStackApp, Tool, metrics};
use crate::ui::theme;
use crate::ui::theme::icons;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};

/// Rail glyph size; slightly larger than label text so icons read at a glance.
const RAIL_ICON_SIZE: f32 = 17.0;

impl FullStackApp {
    pub(super) fn activity_rail(&mut self, ui: &mut egui::Ui) {
        ui.add_space(metrics::PANEL_TOP_PADDING);
        for (tool, icon, hint) in [
            (Tool::Files, icons::FILES, "Explorer"),
            (Tool::Search, icons::SEARCH, "Search in workspace"),
            (Tool::Run, icons::RUN, "Run configurations"),
            (
                Tool::Inspect,
                icons::INSPECT,
                "Inspect runtime and artifacts",
            ),
        ] {
            if rail_button(ui, icon, hint, self.tool == tool).clicked() {
                self.tool = tool;
            }
        }
        // Pin the settings entry to the bottom of the rail, below the tools.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(metrics::PANEL_TOP_PADDING);
            if rail_button(ui, icons::SETTINGS, "Settings (Ctrl+,)", self.settings_open).clicked() {
                self.settings_open = !self.settings_open;
            }
        });
    }
}

/// A full-width tool button. Selected buttons fill with the well tone they reveal
/// and carry an accent bar on the edge facing the tool window.
fn rail_button(ui: &mut egui::Ui, icon: &str, hint: &str, active: bool) -> egui::Response {
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
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(RAIL_ICON_SIZE),
        color,
    );
    resp.on_hover_text(theme::mono(hint, metrics::STATUS_TEXT_SIZE, theme::TEXT))
}
