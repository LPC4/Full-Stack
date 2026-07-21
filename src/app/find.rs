//! Find-in-file: a bar under the tab strip with live match highlighting and
//! Enter / Shift+Enter navigation through the matches.

use super::{FullStackApp, metrics};
use crate::ui::theme;
use eframe::egui;

#[derive(Default)]
pub(crate) struct FindState {
    pub(super) open: bool,
    pub(super) query: String,
    pub(super) current: usize,
    focus: bool,
}

pub(super) struct FindHighlight {
    pub(super) ranges: Vec<(usize, usize)>,
    pub(super) current: usize,
    pub(super) jump_to: Option<usize>,
}

impl FindState {
    pub(super) fn request_open(&mut self) {
        self.open = true;
        self.focus = true;
    }
}

// ASCII-case-insensitive; folding never crosses UTF-8 char boundaries.
pub(super) fn match_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let haystack = text.as_bytes();
    let needle = query.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

impl FullStackApp {
    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui, text: &str) -> Option<FindHighlight> {
        if !self.find.open {
            return None;
        }

        let ranges = match_ranges(text, &self.find.query);
        if !ranges.is_empty() {
            self.find.current = self.find.current.min(ranges.len() - 1);
        }
        let mut jump = false;
        let mut close = false;

        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            ui.label(theme::label("find"));

            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.find.query)
                    .id_salt("find_query")
                    .font(egui::TextStyle::Monospace)
                    .hint_text(theme::mono(
                        "find in file",
                        metrics::LABEL_TEXT_SIZE,
                        theme::TEXT_FAINT,
                    ))
                    .desired_width(220.0),
            );
            if self.find.focus {
                edit.request_focus();
                self.find.focus = false;
            }
            if edit.changed() {
                self.find.current = 0;
                jump = true;
            }
            if edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let back = ui.input(|i| i.modifiers.shift);
                self.step_match(ranges.len(), back);
                jump = true;
                edit.request_focus();
            }

            let count = if self.find.query.is_empty() {
                String::new()
            } else if ranges.is_empty() {
                "no matches".to_owned()
            } else {
                format!("{} of {}", self.find.current + 1, ranges.len())
            };
            let count_color = if ranges.is_empty() && !self.find.query.is_empty() {
                theme::WARNING
            } else {
                theme::TEXT_DIM
            };
            ui.label(theme::mono(count, metrics::STATUS_TEXT_SIZE, count_color));

            if nav_glyph(ui, "\u{2039}").clicked() {
                self.step_match(ranges.len(), true);
                jump = true;
            }
            if nav_glyph(ui, "\u{203A}").clicked() {
                self.step_match(ranges.len(), false);
                jump = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(metrics::PANEL_INSET_X);
                if nav_glyph(ui, "\u{00D7}").clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(metrics::PANEL_HEADER_PAD_Y - 2.0);

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if close {
            self.find.open = false;
            return None;
        }
        if ranges.is_empty() {
            return None;
        }
        let jump_to = jump.then(|| ranges[self.find.current].0);
        Some(FindHighlight {
            ranges,
            current: self.find.current,
            jump_to,
        })
    }

    fn step_match(&mut self, len: usize, back: bool) {
        if len == 0 {
            return;
        }
        self.find.current = if back {
            (self.find.current + len - 1) % len
        } else {
            (self.find.current + 1) % len
        };
    }
}

fn nav_glyph(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    let resp = ui.add(
        egui::Button::new(theme::mono(
            glyph,
            metrics::LABEL_TEXT_SIZE,
            theme::TEXT_DIM,
        ))
        .frame(false),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::match_ranges;

    #[test]
    fn finds_case_insensitive_ascii_matches() {
        assert_eq!(
            match_ranges("Main main MAIN", "main"),
            vec![(0, 4), (5, 9), (10, 14)]
        );
        assert!(match_ranges("abc", "").is_empty());
        assert!(match_ranges("abc", "zzz").is_empty());
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(match_ranges("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn multibyte_text_keeps_valid_ranges() {
        let text = "\u{00E9}x\u{00E9}x";
        let ranges = match_ranges(text, "x");
        for (start, end) in ranges {
            assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        }
    }
}
