//! The editor tab strip: one tab per open document, IntelliJ-style. The active
//! tab drops onto the editor canvas under an accent bar in its layer color.

use super::{FullStackApp, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::workspace::VPath;

enum TabAction {
    Activate(VPath),
    Close(VPath),
    FocusRuntime,
    CloseRuntime,
    FocusDebugger,
    CloseDebugger,
}

impl FullStackApp {
    /// Draw the strip and apply any tab switch/close it produced. Returns false
    /// when nothing is open (the editor shows its empty hint instead).
    pub(super) fn editor_tabs(&mut self, ui: &mut egui::Ui) -> bool {
        if self.workspace.open_paths().is_empty() && self.live.is_none() {
            return false;
        }

        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(vec2(width, metrics::TAB_STRIP_HEIGHT), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::CHROME);
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1.0, theme::HAIRLINE),
        );

        // Trailing zone: the current rung tag (and read-only badge) stay fixed on
        // the right even when the tabs overflow.
        let trailing_width = self.tab_trailing(ui, &painter, rect);
        let tabs_rect =
            egui::Rect::from_min_max(rect.min, pos2(rect.right() - trailing_width, rect.bottom()));

        let mut action = None;
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("tab_strip")
                .max_rect(tabs_rect),
            |ui| {
                ui.set_clip_rect(tabs_rect);
                egui::ScrollArea::horizontal()
                    .id_salt("tab_scroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.horizontal(|ui| {
                            let session_focused = self.main_surface != super::MainSurface::Document;
                            if let Some(live) = &self.live {
                                let label = self
                                    .session_tab_label
                                    .as_deref()
                                    .unwrap_or("machine")
                                    .to_owned();
                                let (_, state_color) =
                                    super::machine_dock::state_style(&live.state);
                                if self.runtime_open
                                    && let Some(act) = session_tab(
                                        ui,
                                        "run",
                                        &label,
                                        self.main_surface == super::MainSurface::Runtime,
                                        state_color,
                                        TabAction::FocusRuntime,
                                        TabAction::CloseRuntime,
                                    )
                                {
                                    action = Some(act);
                                }
                                if self.debugger_open
                                    && let Some(act) = session_tab(
                                        ui,
                                        "debug",
                                        &label,
                                        self.main_surface == super::MainSurface::Debugger,
                                        state_color,
                                        TabAction::FocusDebugger,
                                        TabAction::CloseDebugger,
                                    )
                                {
                                    action = Some(act);
                                }
                            }
                            let active = self.active_path();
                            let open: Vec<VPath> = self.workspace.open_paths().to_vec();
                            for path in open {
                                let is_active = !session_focused && active.as_ref() == Some(&path);
                                let dirty = self
                                    .workspace
                                    .document(&path)
                                    .is_some_and(|doc| doc.is_dirty());
                                if let Some(act) = tab(ui, &path, is_active, dirty) {
                                    action = Some(act);
                                }
                            }
                        });
                    });
            },
        );

        match action {
            Some(TabAction::Activate(path)) => self.activate_path(&path),
            Some(TabAction::Close(path)) => self.close_path(&path),
            Some(TabAction::FocusRuntime) => self.focus_runtime(),
            Some(TabAction::CloseRuntime) => self.close_runtime_surface(),
            Some(TabAction::FocusDebugger) => self.focus_debugger(),
            Some(TabAction::CloseDebugger) => self.close_debugger_surface(),
            None => {}
        }
        self.close_confirmation(ui);
        true
    }

    /// Right edge of the strip: rung tag in its spectrum color, plus a read-only
    /// badge when the focused document rejects edits. Returns the reserved width.
    fn tab_trailing(&self, ui: &egui::Ui, painter: &egui::Painter, rect: egui::Rect) -> f32 {
        let (tag, color) = match self.main_surface {
            super::MainSurface::Document => (
                format!("{} {}", self.layer.tag(), self.layer.name()),
                self.layer.color(),
            ),
            super::MainSurface::Runtime => ("RUN".to_owned(), theme::ELECTRIC),
            super::MainSurface::Debugger => ("DEBUG".to_owned(), theme::ELECTRIC),
        };
        let tag_galley = ui.fonts_mut(|f| {
            f.layout_no_wrap(tag, FontId::monospace(metrics::STATUS_TEXT_SIZE), color)
        });
        let mut used = tag_galley.size().x + metrics::PANEL_INSET_X * 2.0;
        painter.galley(
            pos2(
                rect.right() - metrics::PANEL_INSET_X - tag_galley.size().x,
                rect.center().y - tag_galley.size().y / 2.0,
            ),
            tag_galley,
            color,
        );

        let read_only = self.main_surface == super::MainSurface::Document
            && self
                .workspace
                .active_document()
                .is_some_and(|doc| doc.is_read_only());
        if read_only {
            let badge = ui.fonts_mut(|f| {
                f.layout_no_wrap(
                    "read only".to_owned(),
                    FontId::monospace(metrics::STATUS_TEXT_SIZE),
                    theme::TEXT_FAINT,
                )
            });
            used += badge.size().x + metrics::PANEL_INSET_X;
            painter.galley(
                pos2(
                    rect.right() - used + metrics::PANEL_INSET_X,
                    rect.center().y - badge.size().y / 2.0,
                ),
                badge,
                theme::TEXT_FAINT,
            );
        }
        used
    }

    fn close_confirmation(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.pending_close.clone() else {
            return;
        };
        if self.workspace.document(&path).is_none() {
            self.pending_close = None;
            return;
        }
        egui::Frame::new()
            .fill(theme::SURFACE)
            .inner_margin(egui::Margin::symmetric(
                metrics::PANEL_INSET_X as i8,
                metrics::SECTION_GAP as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::mono(
                        format!("{path} has unsaved edits"),
                        metrics::STATUS_TEXT_SIZE,
                        theme::WARNING,
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if chrome_button(ui, "cancel").clicked() {
                            self.pending_close = None;
                        }
                        if chrome_button(ui, "discard").clicked() {
                            self.close_path_confirmed(&path);
                        }
                    });
                });
            });
    }
}

/// A session surface tab. Closing hides the view; Stop owns session termination.
fn session_tab(
    ui: &mut egui::Ui,
    kind: &str,
    label: &str,
    active: bool,
    state_color: egui::Color32,
    focus: TabAction,
    close: TabAction,
) -> Option<TabAction> {
    let name = format!("[{kind}] {label}");
    let font = FontId::monospace(metrics::LABEL_TEXT_SIZE);
    let text_color = if active { theme::TEXT } else { theme::TEXT_DIM };
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), font, text_color));

    let glyph_slot = metrics::LABEL_TEXT_SIZE;
    let tab_width =
        metrics::TAB_PAD_X * 2.0 + galley.size().x + metrics::TAB_GLYPH_GAP + glyph_slot;
    let (rect, resp) =
        ui.allocate_exact_size(vec2(tab_width, metrics::TAB_STRIP_HEIGHT), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone()));
    let painter = ui.painter_at(rect);

    if active {
        painter.rect_filled(rect, 0.0, theme::CANVAS);
        let accent = egui::Rect::from_min_max(
            rect.left_top(),
            pos2(rect.right(), rect.top() + metrics::TAB_ACCENT_HEIGHT),
        );
        painter.rect_filled(accent, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.line_segment(
        [rect.right_top(), pos2(rect.right(), rect.bottom() - 1.0)],
        egui::Stroke::new(1.0, theme::HAIRLINE_SOFT),
    );
    painter.galley(
        pos2(
            rect.left() + metrics::TAB_PAD_X,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );

    // Glyph slot: close control when the pointer is near, else the state dot.
    let glyph_center = pos2(
        rect.right() - metrics::TAB_PAD_X - glyph_slot / 2.0 + 2.0,
        rect.center().y,
    );
    let glyph_rect = egui::Rect::from_center_size(glyph_center, vec2(glyph_slot, glyph_slot));
    let close_resp = ui.interact(glyph_rect, resp.id.with("close"), Sense::click());
    close_resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("close [{kind}] {label}"),
        )
    });
    if close_resp.hovered() {
        painter.rect_filled(glyph_rect, 0.0, theme::SURFACE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.hovered() || close_resp.hovered() {
        painter.text(
            glyph_center,
            Align2::CENTER_CENTER,
            "\u{00D7}",
            FontId::monospace(metrics::LABEL_TEXT_SIZE),
            if close_resp.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            },
        );
    } else {
        painter.circle_filled(glyph_center, 2.5, state_color);
    }

    if close_resp.clicked() || resp.middle_clicked() {
        Some(close)
    } else if resp.clicked() {
        Some(focus)
    } else {
        None
    }
}

/// One tab: basename, a dirty dot, and a close glyph when hovered or active.
/// Middle click closes; the accent bar carries the file's source-layer color.
fn tab(ui: &mut egui::Ui, path: &VPath, active: bool, dirty: bool) -> Option<TabAction> {
    let name = path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or(path.as_str())
        .to_owned();
    let font = FontId::monospace(metrics::LABEL_TEXT_SIZE);
    let text_color = if active { theme::TEXT } else { theme::TEXT_DIM };
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), font, text_color));

    // Reserve a fixed glyph slot so the tab does not resize when the close
    // control appears on hover.
    let glyph_slot = metrics::LABEL_TEXT_SIZE;
    let tab_width =
        metrics::TAB_PAD_X * 2.0 + galley.size().x + metrics::TAB_GLYPH_GAP + glyph_slot;
    let (rect, resp) =
        ui.allocate_exact_size(vec2(tab_width, metrics::TAB_STRIP_HEIGHT), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone()));
    let painter = ui.painter_at(rect);

    let layer =
        FullStackApp::source_layer_for_kind(FullStackApp::source_kind_for_path(path.as_str()));
    if active {
        // Drop onto the editor canvas: fill covers the strip's bottom hairline.
        painter.rect_filled(rect, 0.0, theme::CANVAS);
        let accent = egui::Rect::from_min_max(
            rect.left_top(),
            pos2(rect.right(), rect.top() + metrics::TAB_ACCENT_HEIGHT),
        );
        painter.rect_filled(accent, 0.0, layer.color());
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.line_segment(
        [rect.right_top(), pos2(rect.right(), rect.bottom() - 1.0)],
        egui::Stroke::new(1.0, theme::HAIRLINE_SOFT),
    );
    painter.galley(
        pos2(
            rect.left() + metrics::TAB_PAD_X,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );

    // Glyph slot: close control when the pointer is near, else the dirty dot.
    let glyph_center = pos2(
        rect.right() - metrics::TAB_PAD_X - glyph_slot / 2.0 + 2.0,
        rect.center().y,
    );
    let glyph_rect = egui::Rect::from_center_size(glyph_center, vec2(glyph_slot, glyph_slot));
    let close_resp = ui.interact(glyph_rect, resp.id.with("close"), Sense::click());
    close_resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("close {name}"))
    });
    let show_close = resp.hovered() || close_resp.hovered() || (active && !dirty);
    if close_resp.hovered() {
        painter.rect_filled(glyph_rect, 0.0, theme::SURFACE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if show_close {
        painter.text(
            glyph_center,
            Align2::CENTER_CENTER,
            "\u{00D7}",
            FontId::monospace(metrics::LABEL_TEXT_SIZE),
            if close_resp.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            },
        );
    } else if dirty {
        painter.circle_filled(glyph_center, 2.5, theme::WARNING);
    }

    if close_resp.clicked() || resp.middle_clicked() {
        Some(TabAction::Close(path.clone()))
    } else if resp.clicked() {
        Some(TabAction::Activate(path.clone()))
    } else {
        None
    }
}

fn chrome_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let resp = ui.add(egui::Button::new(theme::mono(
        label,
        metrics::STATUS_TEXT_SIZE,
        theme::TEXT,
    )));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
