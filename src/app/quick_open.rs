//! Quick open (Cmd+P): a floating file switcher over the whole shell. Substring
//! filter on workspace paths; arrows + Enter to open, Escape to dismiss.

use super::{FullStackApp, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::workspace::{EntryKind, TreeEntry, VPath};

const PANEL_WIDTH: f32 = 460.0;
const MAX_RESULTS: usize = 12;

pub(crate) struct QuickOpenState {
    query: String,
    selected: usize,
    focus: bool,
}

impl Default for QuickOpenState {
    fn default() -> Self {
        Self {
            query: String::new(),
            selected: 0,
            focus: true,
        }
    }
}

impl FullStackApp {
    /// Render the switcher when open; draws above every panel.
    pub(super) fn quick_open_overlay(&mut self, ctx: &egui::Context) {
        let Some(state) = self.quick_open.as_ref() else {
            return;
        };
        let matches = self.quick_open_matches(&state.query);

        let mut chosen: Option<VPath> = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("quick_open"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 90.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::CHROME)
                    .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
                    .inner_margin(egui::Margin::same(0))
                    .show(ui, |ui| {
                        ui.set_width(PANEL_WIDTH);
                        let state = self.quick_open.as_mut().expect("checked above");
                        let (picked, dismissed) = palette_body(ui, state, &matches);
                        chosen = picked;
                        close = dismissed;
                    });
            });

        // Click anywhere outside the panel dismisses, like every palette UI.
        let outside_click = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| !area.response.rect.contains(pos))
        });
        if outside_click {
            close = true;
        }

        if let Some(path) = chosen {
            self.open_path(path);
            self.quick_open = None;
        } else if close {
            self.quick_open = None;
        }
    }

    /// Workspace files matching the query, basename hits first, capped for display.
    fn quick_open_matches(&self, query: &str) -> Vec<VPath> {
        let mut matches: Vec<VPath> = Vec::new();
        for entry in &self.workspace.tree().root {
            collect_matches(entry, query, &mut matches);
        }
        matches.sort_by_key(|path| {
            let base = basename(path.as_str()).to_ascii_lowercase();
            (!base.contains(&query.to_ascii_lowercase()), path.clone())
        });
        matches.truncate(MAX_RESULTS);
        matches
    }
}

/// Query field, keyboard navigation, and result rows. Returns the chosen path
/// and whether the palette asked to close.
fn palette_body(
    ui: &mut egui::Ui,
    state: &mut QuickOpenState,
    matches: &[VPath],
) -> (Option<VPath>, bool) {
    let mut chosen = None;

    ui.add_space(metrics::PANEL_HEADER_PAD_Y);
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        ui.label(theme::label("go to file"));
    });
    ui.horizontal(|ui| {
        ui.add_space(metrics::PANEL_INSET_X);
        let edit = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .id_salt("quick_open_query")
                .font(egui::TextStyle::Monospace)
                .hint_text(theme::mono(
                    "type a file name",
                    metrics::LABEL_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ))
                .desired_width(PANEL_WIDTH - metrics::PANEL_INSET_X * 2.0),
        );
        if state.focus {
            edit.request_focus();
            state.focus = false;
        }
        if edit.changed() {
            state.selected = 0;
        }
    });
    ui.add_space(metrics::PANEL_HEADER_PAD_Y);

    if !matches.is_empty() {
        state.selected = state.selected.min(matches.len() - 1);
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                state.selected = (state.selected + 1) % matches.len();
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                state.selected = (state.selected + matches.len() - 1) % matches.len();
            }
        });
    }
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        chosen = matches.get(state.selected).cloned();
    }
    let close = ui.input(|i| i.key_pressed(egui::Key::Escape));

    chrome_hairline(ui);
    if matches.is_empty() {
        ui.add_space(metrics::SECTION_GAP);
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::mono(
                "no matching files",
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ));
        });
        ui.add_space(metrics::SECTION_GAP);
    }
    for (idx, path) in matches.iter().enumerate() {
        if result_row(ui, path, idx == state.selected).clicked() {
            chosen = Some(path.clone());
        }
    }
    (chosen, close)
}

fn collect_matches(entry: &TreeEntry, query: &str, out: &mut Vec<VPath>) {
    match entry.kind {
        EntryKind::Dir => {
            for child in &entry.children {
                collect_matches(child, query, out);
            }
        }
        EntryKind::File => {
            if query.is_empty()
                || entry
                    .path
                    .as_str()
                    .to_ascii_lowercase()
                    .contains(&query.to_ascii_lowercase())
            {
                out.push(entry.path.clone());
            }
        }
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// One result row: basename bright, the containing path dim after it.
fn result_row(ui: &mut egui::Ui, path: &VPath, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), metrics::ROW_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let base = basename(path.as_str());
    let base_color = if selected {
        theme::ELECTRIC
    } else {
        theme::TEXT
    };
    let base_galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            base.to_owned(),
            FontId::monospace(metrics::LABEL_TEXT_SIZE),
            base_color,
        )
    });
    let base_width = base_galley.size().x;
    painter.galley(
        pos2(
            rect.left() + metrics::PANEL_INSET_X,
            rect.center().y - base_galley.size().y / 2.0,
        ),
        base_galley,
        base_color,
    );
    let dir = &path.as_str()[..path.as_str().len() - base.len()];
    if !dir.is_empty() {
        painter.text(
            pos2(
                rect.left() + metrics::PANEL_INSET_X + base_width + metrics::TAB_GLYPH_GAP,
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            dir.trim_end_matches('/'),
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            theme::TEXT_FAINT,
        );
    }
    resp
}

/// A full-width hairline inside the palette frame.
fn chrome_hairline(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE);
}
