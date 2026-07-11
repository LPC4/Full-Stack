//! Row and formatting helpers every inspector pane shares.

use crate::app::{chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};

/// Minimum seconds between probe captures while the machine is running.
pub(super) const RUNNING_CAPTURE_INTERVAL: f64 = 0.25;

/// Shared throttle for the step-driven probes: recapture when there is no prior
/// snapshot, or the step count advanced and we are paused or past the interval.
pub(super) fn capture_due(last: Option<(u64, f64)>, steps: u64, running: bool, now: f64) -> bool {
    match last {
        None => true,
        Some((last_steps, last_time)) => {
            last_steps != steps && (!running || now - last_time > RUNNING_CAPTURE_INTERVAL)
        }
    }
}

/// A left-inset section eyebrow, matching the panel header style.
pub(super) fn section_label(ui: &mut egui::Ui, text: &str) {
    chrome::indented(ui, |ui| {
        ui.label(theme::label(text));
    });
}

/// A two-column mono row: dim key on the left, value on the right. Values too
/// wide for the row are left-elided (keeping the tail, which ends paths well).
pub(super) fn stat_row(ui: &mut egui::Ui, key: &str, value: &str) {
    let width = ui.available_width();
    let font = FontId::monospace(metrics::LABEL_TEXT_SIZE);
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
    }
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        key,
        font.clone(),
        theme::TEXT_DIM,
    );
    let avail = width - 2.0 * metrics::PANEL_INSET_X - key.chars().count() as f32 * char_w - 12.0;
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        elide_left(value, (avail / char_w).max(0.0) as usize),
        font,
        theme::TEXT,
    );
    let elided = value.chars().count() as f32 * char_w > avail;
    if elided {
        resp.on_hover_text(format!("{key}: {value}"));
    }
}

/// Keep the trailing `fit` characters of `text`, marking the cut with `..`.
fn elide_left(text: &str, fit: usize) -> String {
    let len = text.chars().count();
    if len <= fit {
        return text.to_owned();
    }
    let keep = fit.saturating_sub(2);
    let tail: String = text.chars().skip(len - keep).collect();
    format!("..{tail}")
}

pub(super) fn kv_line(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(theme::label(key));
        ui.label(theme::mono(
            value.to_owned(),
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_DIM,
        ));
    });
}

pub(super) fn mono_row(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::hover());
    ui.painter_at(rect).text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        color,
    );
}

pub(super) fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}k", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

/// Percentage of `num`/`den` as text, or a dash when there are no samples.
pub(super) fn rate_str(num: u64, den: u64) -> String {
    if den == 0 {
        "--".to_owned()
    } else {
        format!("{:.1}%", num as f64 / den as f64 * 100.0)
    }
}

/// A count with its share of `den` appended, e.g. `123 (4.5%)`.
pub(super) fn percent(num: u64, den: u64, label: &str) -> String {
    if den == 0 {
        label.to_owned()
    } else {
        format!("{label} ({:.1}%)", num as f64 / den as f64 * 100.0)
    }
}

/// Cache size in binary units: `4 KiB`, `256 KiB`, `8 MiB`.
pub(super) fn cache_bytes(n: usize) -> String {
    if n >= 1 << 20 {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 10 {
        format!("{} KiB", n >> 10)
    } else {
        format!("{n} B")
    }
}
