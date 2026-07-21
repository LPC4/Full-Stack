//! Workspace-wide find (Ctrl+Shift+F): a Search tool scanning every text file,
//! results grouped by file with click-to-jump line previews.

use super::{FullStackApp, chrome, explorer, find, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::workspace::{EntryKind, VPath};

// Larger files are skipped; the scan is a plain pass, not an index.
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_TOTAL_HITS: usize = 500;
const PREVIEW_MAX: usize = 128;
const CONTEXT_BEFORE: usize = 32;

#[derive(Default)]
pub(crate) struct SearchState {
    query: String,
    focus: bool,
    results: Option<SearchResults>,
    computed_for: Option<String>,
}

impl SearchState {
    pub(super) fn request_focus(&mut self) {
        self.focus = true;
    }
}

struct SearchResults {
    files: Vec<FileHits>,
    total: usize,
    truncated: bool,
}

struct FileHits {
    path: VPath,
    hits: Vec<LineHit>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct LineHit {
    pub(super) line: usize,
    pub(super) preview: String,
    pub(super) range: (usize, usize),
}

pub(super) fn line_hits(text: &str, query: &str, budget: usize) -> Vec<LineHit> {
    let mut hits = Vec::new();
    let mut ranges = find::match_ranges(text, query).into_iter().peekable();
    let mut line_start = 0;
    for (idx, line) in text.split_inclusive('\n').enumerate() {
        let line_end = line_start + line.len();
        let content = line.trim_end_matches('\n').trim_end_matches('\r');
        while let Some(&(start, end)) = ranges.peek() {
            if start >= line_end {
                break;
            }
            ranges.next();
            if hits.len() >= budget {
                return hits;
            }
            let s = (start - line_start).min(content.len());
            let e = (end - line_start).clamp(s, content.len());
            let (preview, range) = preview_window(content, s, e);
            hits.push(LineHit {
                line: idx + 1,
                preview,
                range,
            });
        }
        line_start = line_end;
    }
    hits
}

fn preview_window(line: &str, start: usize, end: usize) -> (String, (usize, usize)) {
    if line.len() <= PREVIEW_MAX {
        return (line.to_owned(), (start, end));
    }
    let mut from = start.saturating_sub(CONTEXT_BEFORE);
    while from > 0 && !line.is_char_boundary(from) {
        from -= 1;
    }
    let mut to = (from + PREVIEW_MAX).max(end).min(line.len());
    while to < line.len() && !line.is_char_boundary(to) {
        to += 1;
    }
    (line[from..to].to_owned(), (start - from, end - from))
}

fn is_binary_name(name: &str) -> bool {
    name.ends_with(".elf") || name.ends_with(".o")
}

impl FullStackApp {
    pub(super) fn search_panel(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Search");
        self.search_query_row(ui);
        if self
            .search
            .computed_for
            .as_deref()
            .is_none_or(|computed| computed != self.search.query)
        {
            self.recompute_search();
        }

        ui.add_space(metrics::SECTION_GAP);
        self.search_summary(ui);
        ui.add_space(metrics::SECTION_GAP);

        let mut open_file: Option<VPath> = None;
        let mut jump: Option<(VPath, usize)> = None;
        egui::ScrollArea::vertical()
            .id_salt("search_results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let Some(results) = &self.search.results else {
                    return;
                };
                for file in &results.files {
                    if file_row(ui, &file.path, file.hits.len()).clicked() {
                        open_file = Some(file.path.clone());
                    }
                    for hit in &file.hits {
                        if hit_row(ui, hit).clicked() {
                            jump = Some((file.path.clone(), hit.line));
                        }
                    }
                    ui.add_space(metrics::SECTION_GAP);
                }
            });
        if let Some(path) = open_file {
            self.open_path(path);
        }
        if let Some((path, line)) = jump {
            self.jump_to_source(path.as_str(), line);
        }
    }

    fn search_query_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(metrics::PANEL_INSET_X);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.search.query)
                    .id_salt("workspace_search_query")
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme::TEXT)
                    .hint_text(theme::mono(
                        "find in workspace",
                        metrics::LABEL_TEXT_SIZE,
                        theme::TEXT_FAINT,
                    ))
                    .desired_width(ui.available_width() - metrics::PANEL_INSET_X - 24.0),
            );
            if self.search.focus {
                edit.request_focus();
                self.search.focus = false;
            }
            // Rescan on demand: edits elsewhere do not auto-refresh results.
            if ui
                .add(
                    egui::Button::new(theme::mono(
                        "\u{21BB}",
                        metrics::LABEL_TEXT_SIZE,
                        theme::TEXT_DIM,
                    ))
                    .frame(false),
                )
                .on_hover_text(theme::mono(
                    "rescan the workspace",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT,
                ))
                .clicked()
            {
                self.search.computed_for = None;
            }
        });
    }

    fn search_summary(&self, ui: &mut egui::Ui) {
        let (text, color) = match &self.search.results {
            None => (
                "type to search every workspace file".to_owned(),
                theme::TEXT_FAINT,
            ),
            Some(results) if results.total == 0 => ("no matches".to_owned(), theme::WARNING),
            Some(results) => {
                let cap = if results.truncated {
                    format!(" (first {MAX_TOTAL_HITS})")
                } else {
                    String::new()
                };
                (
                    format!(
                        "{} in {}{cap}",
                        super::diagnostics::tally(results.total as u64, "match"),
                        super::diagnostics::tally(results.files.len() as u64, "file")
                    ),
                    theme::TEXT_DIM,
                )
            }
        };
        chrome::indented(ui, |ui| {
            ui.label(theme::mono(text, metrics::STATUS_TEXT_SIZE, color));
        });
    }

    // Open documents are searched through their (possibly unsaved) buffers.
    fn recompute_search(&mut self) {
        let query = self.search.query.clone();
        if query.is_empty() {
            self.search.results = None;
            self.search.computed_for = Some(query);
            return;
        }
        let mut files = Vec::new();
        let mut total = 0;
        let mut truncated = false;
        for entry in self.workspace.tree().flatten() {
            if entry.kind != EntryKind::File
                || entry.len > MAX_FILE_BYTES
                || is_binary_name(&entry.name)
            {
                continue;
            }
            if total >= MAX_TOTAL_HITS {
                truncated = true;
                break;
            }
            let text = match self.workspace.document(&entry.path) {
                Some(doc) if doc.is_binary() => continue,
                Some(doc) => doc.text().to_owned(),
                None => {
                    let Ok(bytes) = self.workspace.fs().read_file(&entry.path) else {
                        continue;
                    };
                    let Ok(text) = String::from_utf8(bytes) else {
                        continue;
                    };
                    text
                }
            };
            let hits = line_hits(&text, &query, MAX_TOTAL_HITS - total);
            if !hits.is_empty() {
                total += hits.len();
                files.push(FileHits {
                    path: entry.path.clone(),
                    hits,
                });
            }
        }
        self.search.results = Some(SearchResults {
            files,
            total,
            truncated,
        });
        self.search.computed_for = Some(query);
    }
}

fn file_row(ui: &mut egui::Ui, path: &VPath, count: usize) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), metrics::ROW_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mid_y = rect.center().y;
    let base = path.file_name().unwrap_or_default();
    let chip = egui::Rect::from_center_size(
        pos2(rect.left() + metrics::PANEL_INSET_X + 4.5, mid_y),
        vec2(5.0, 5.0),
    );
    painter.rect_filled(chip, 0.0, explorer::file_chip_color(base));
    let text_x = rect.left() + metrics::PANEL_INSET_X + 13.0;
    let base_rect = painter.text(
        pos2(text_x, mid_y),
        Align2::LEFT_CENTER,
        base,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        theme::TEXT,
    );
    let dir = &path.as_str()[..path.as_str().len() - base.len()];
    if !dir.is_empty() {
        painter.text(
            pos2(base_rect.right() + metrics::TAB_GLYPH_GAP, mid_y),
            Align2::LEFT_CENTER,
            dir.trim_end_matches('/'),
            FontId::monospace(metrics::STATUS_TEXT_SIZE),
            theme::TEXT_FAINT,
        );
    }
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, mid_y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
    resp
}

fn hit_row(ui: &mut egui::Ui, hit: &LineHit) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), metrics::ROW_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mid_y = rect.center().y;
    let font = FontId::monospace(metrics::STATUS_TEXT_SIZE);
    let gutter_right = rect.left() + metrics::PANEL_INSET_X + 34.0;
    painter.text(
        pos2(gutter_right, mid_y),
        Align2::RIGHT_CENTER,
        hit.line.to_string(),
        font.clone(),
        theme::TEXT_FAINT,
    );
    let mut x = gutter_right + metrics::TAB_GLYPH_GAP;
    let (start, end) = hit.range;
    let segments = [
        (&hit.preview[..start], theme::TEXT_DIM),
        (&hit.preview[start..end], theme::ELECTRIC),
        (&hit.preview[end..], theme::TEXT_DIM),
    ];
    for (text, color) in segments {
        if text.is_empty() || x >= rect.right() {
            continue;
        }
        let segment_rect = painter.text(
            pos2(x, mid_y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            color,
        );
        x = segment_rect.right();
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::{LineHit, line_hits, preview_window};

    #[test]
    fn hits_carry_line_numbers_and_in_line_ranges() {
        let text = "alpha\nbeta alpha\ngamma\n";
        let hits = line_hits(text, "alpha", 10);
        assert_eq!(
            hits,
            vec![
                LineHit {
                    line: 1,
                    preview: "alpha".to_owned(),
                    range: (0, 5),
                },
                LineHit {
                    line: 2,
                    preview: "beta alpha".to_owned(),
                    range: (5, 10),
                },
            ]
        );
    }

    #[test]
    fn budget_caps_the_hit_count() {
        let text = "x\nx\nx\nx\n";
        assert_eq!(line_hits(text, "x", 2).len(), 2);
        assert!(line_hits(text, "", 10).is_empty());
    }

    #[test]
    fn two_matches_on_one_line_produce_two_hits() {
        let hits = line_hits("ab ab", "ab", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].range, (0, 2));
        assert_eq!(hits[1].range, (3, 5));
        assert_eq!(hits[0].line, 1);
    }

    #[test]
    fn long_lines_window_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(200), "b".repeat(200));
        let start = 200;
        let (preview, (s, e)) = preview_window(&line, start, start + 6);
        assert!(preview.len() <= 140, "preview stays bounded");
        assert_eq!(&preview[s..e], "needle");
    }

    #[test]
    fn crlf_previews_do_not_include_the_line_break() {
        let hits = line_hits("one\r\ntwo\r\n", "two", 10);
        assert_eq!(hits[0].preview, "two");
        assert_eq!(hits[0].line, 2);
    }
}
