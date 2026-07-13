use super::metrics;
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};

pub(super) fn centered_hint(ui: &mut egui::Ui, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(theme::mono(
            text,
            metrics::LABEL_TEXT_SIZE,
            theme::TEXT_FAINT,
        ));
    });
}

/// A tool-window header band: eyebrow label over a full-width hairline. Shared by
/// the explorer and machine dock so every column reads the same way.
pub(super) fn panel_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(metrics::PANEL_HEADER_PAD_Y);
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.label(theme::label(title));
    });
    ui.add_space(metrics::PANEL_HEADER_PAD_Y);
    hairline(ui);
    ui.add_space(metrics::SECTION_GAP);
}

/// Run `add_contents` in a left-inset column so body text aligns with the header
/// label while row backgrounds can still span the full column width.
pub(super) fn indented(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.vertical(|ui| {
            // Mirror the left inset so right-aligned values keep off the edge.
            ui.set_max_width((ui.available_width() - metrics::PANEL_INSET_X).max(0.0));
            add_contents(ui);
        });
    });
}

pub(super) fn hairline(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, metrics::HAIRLINE_SIZE),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE);
}

/// An interior divider, one tier softer than `hairline`; separates sections
/// inside a single panel rather than panels from each other.
pub(super) fn hairline_soft(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, metrics::HAIRLINE_SIZE),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE_SOFT);
}

/// A section eyebrow inside a panel body: gap, tracked label, soft divider.
pub(super) fn section_eyebrow(ui: &mut egui::Ui, title: &str) {
    ui.add_space(metrics::DOCK_SECTION_GAP);
    ui.label(theme::label(title));
    ui.add_space(metrics::SECTION_GAP - 2.0);
    hairline_soft(ui);
    ui.add_space(metrics::SECTION_GAP);
}

/// A conjoined segmented control (the `va|pa` idiom generalized): every option
/// visible, the active one filled with ELECTRIC text. Returns a clicked index.
pub(super) fn segmented(ui: &mut egui::Ui, options: &[&str], selected: usize) -> Option<usize> {
    let font = FontId::monospace(metrics::STATUS_TEXT_SIZE);
    let seg_w = options
        .iter()
        .map(|label| {
            ui.fonts_mut(|f| f.layout_no_wrap((*label).to_owned(), font.clone(), theme::TEXT))
                .size()
                .x
        })
        .fold(0.0f32, f32::max)
        + 2.0 * metrics::SEGMENT_PAD_X;
    let size = vec2(seg_w * options.len() as f32, metrics::SEGMENT_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    for (i, label) in options.iter().enumerate() {
        let seg = egui::Rect::from_min_size(
            pos2(rect.left() + i as f32 * seg_w, rect.top()),
            vec2(seg_w, rect.height()),
        );
        if i == selected {
            painter.rect_filled(seg, 0.0, theme::SURFACE);
        }
        painter.text(
            seg.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            if i == selected {
                theme::ELECTRIC
            } else {
                theme::TEXT_FAINT
            },
        );
        if i > 0 {
            painter.line_segment(
                [
                    pos2(seg.left(), rect.top()),
                    pos2(seg.left(), rect.bottom()),
                ],
                egui::Stroke::new(1.0, theme::HAIRLINE),
            );
        }
    }
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, theme::HAIRLINE),
        egui::StrokeKind::Inside,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let clicked = resp
        .interact_pointer_pos()
        .filter(|_| resp.clicked())
        .map(|pos| (((pos.x - rect.left()) / seg_w) as usize).min(options.len() - 1))?;
    (clicked != selected).then_some(clicked)
}

/// The visual weight of an `action_row` button.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionTone {
    Primary,
    Normal,
    Danger,
}

/// A row of equal-width framed buttons spanning the available width. Returns
/// the clicked index. The shared transport/button idiom for narrow panels.
pub(super) fn action_row(ui: &mut egui::Ui, actions: &[(&str, ActionTone)]) -> Option<usize> {
    let mut clicked = None;
    let avail = ui.available_width();
    let n = actions.len() as f32;
    let width = (avail - (n - 1.0) * metrics::ACTION_GAP) / n;
    let (row, _) =
        ui.allocate_exact_size(vec2(avail, metrics::ACTION_BUTTON_HEIGHT), Sense::hover());
    for (i, (label, tone)) in actions.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            pos2(
                row.left() + i as f32 * (width + metrics::ACTION_GAP),
                row.top(),
            ),
            vec2(width, row.height()),
        );
        let id = ui.id().with(("action_row", i, *label));
        let resp = ui.interact(rect, id, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *label));
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            0.0,
            if resp.hovered() {
                theme::SURFACE_ALT
            } else {
                theme::SURFACE
            },
        );
        let (border, text) = match tone {
            ActionTone::Primary => (theme::ELECTRIC, theme::ELECTRIC),
            ActionTone::Normal => (theme::HAIRLINE, theme::TEXT),
            ActionTone::Danger => (theme::HAIRLINE, theme::DANGER),
        };
        painter.rect_stroke(
            rect,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0, border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            text,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// One tab in a view-switch strip (machine surfaces, terminal tabs): the active
/// tab gets a fill and an electric underline, so what is selectable reads as tabs.
pub(super) fn view_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let font = FontId::monospace(metrics::STATUS_TEXT_SIZE);
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT));
    let size = vec2(
        galley.size().x + 2.0 * metrics::TAB_PAD_X,
        metrics::VIEW_TAB_HEIGHT,
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    if active {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if active {
        theme::TEXT
    } else if resp.hovered() {
        theme::TEXT_DIM
    } else {
        theme::TEXT_FAINT
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    if active {
        let underline = egui::Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - metrics::TAB_ACCENT_HEIGHT),
            rect.right_bottom(),
        );
        painter.rect_filled(underline, 0.0, theme::ELECTRIC);
    }
    resp
}
