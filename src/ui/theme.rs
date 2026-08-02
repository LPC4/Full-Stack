//! Instrument-panel theme: a quiet GitHub Dark base so the depth spectrum
//! (see [`crate::ui::Layer`]) is the only loud thing on screen.

use egui::{Color32, FontId, RichText, Stroke};

#[expect(
    dead_code,
    reason = "the shared palette also contains Guide syntax tokens"
)]
mod palette {
    use egui::Color32;

    include!(concat!(env!("OUT_DIR"), "/theme_palette.rs"));
}

pub use palette::{
    ACCENT, BEZEL, BORDER, BORDER_MUTED, CANVAS, CHROME, DANGER, SCROLL_HOVER, SELECTION, SURFACE,
    SURFACE_HOVER, TEXT, TEXT_FAINT, TEXT_MUTED, TEXT_STRONG, WARNING,
};

pub const WELL: Color32 = CHROME;
pub const TERMINAL: Color32 = CANVAS;
pub const SURFACE_ALT: Color32 = SURFACE_HOVER;
pub const HAIRLINE: Color32 = BORDER;
pub const HAIRLINE_SOFT: Color32 = BORDER_MUTED;
pub const TEXT_DIM: Color32 = TEXT_MUTED;
pub const ELECTRIC: Color32 = ACCENT;
pub const CURRENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x0B, 0x0D, 0x10, 0x7A);
pub const FIND_MATCH: Color32 = Color32::from_rgba_premultiplied(0x13, 0x24, 0x38, 0x38);
pub const FIND_MATCH_CURRENT: Color32 = Color32::from_rgba_premultiplied(0x4C, 0x37, 0x0C, 0x5C);
pub const DANGER_WASH: Color32 = Color32::from_rgba_premultiplied(0x2F, 0x0F, 0x0E, 0x30);
pub const WARNING_WASH: Color32 = Color32::from_rgba_premultiplied(0x28, 0x1D, 0x06, 0x30);

/// Apply the instrument theme: fonts, dark base, hairline strokes, sharp corners,
/// tight spacing. Call once after the egui context exists.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.weak_text_color = Some(TEXT_DIM);
    v.window_fill = CHROME;
    v.panel_fill = CANVAS;
    v.extreme_bg_color = BEZEL;
    v.text_edit_bg_color = Some(CANVAS);
    v.faint_bg_color = SURFACE;
    v.code_bg_color = CANVAS;
    v.warn_fg_color = WARNING;
    v.error_fg_color = DANGER;
    v.window_stroke = Stroke::new(1.0, HAIRLINE);
    v.selection.bg_fill = SELECTION;
    v.selection.stroke = Stroke::new(1.0, ELECTRIC);
    v.hyperlink_color = ELECTRIC;
    v.text_cursor.stroke = Stroke::new(1.5, ELECTRIC);
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v.disabled_alpha = 0.55;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::ZERO;
        w.bg_stroke = Stroke::new(1.0, HAIRLINE);
        w.fg_stroke = Stroke::new(1.0, TEXT);
    }
    v.widgets.noninteractive.bg_fill = CHROME;
    v.widgets.noninteractive.weak_bg_fill = CHROME;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.bg_fill = SURFACE_ALT;
    v.widgets.hovered.weak_bg_fill = SURFACE_ALT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, SCROLL_HOVER);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    v.widgets.active.bg_fill = SURFACE_ALT;
    v.widgets.active.weak_bg_fill = SURFACE_ALT;
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    v.widgets.open.bg_fill = SURFACE_ALT;
    v.widgets.open.weak_bg_fill = SURFACE_ALT;
    v.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_STRONG);

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(6.0, 6.0);
    s.button_padding = egui::vec2(8.0, 4.0);
    style.interaction.resize_grab_radius_side = 8.0;

    // Tab/workspace/pc-follow swaps reuse rects with new ids by design; silence
    // egui's red-rect warning. `Style::debug` exists only under debug_assertions.
    #[cfg(debug_assertions)]
    {
        style.debug.warn_if_rect_changes_id = false;
    }

    ctx.set_global_style(style);
}

// egui's built-ins stay as glyph fallback for symbols the design faces lack.
fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "space_grotesk".to_owned(),
        FontData::from_static(include_bytes!("../../assets/fonts/SpaceGrotesk.ttf")).into(),
    );
    fonts.font_data.insert(
        "plex_mono".to_owned(),
        FontData::from_static(include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "space_grotesk".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "plex_mono".to_owned());
    // Phosphor Light is the icon face. add_to_fonts wires only Proportional; add
    // Monospace too, since instrument labels (theme::mono) render there.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Light);
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("phosphor".to_owned());
    ctx.set_fonts(fonts);
}

/// The icon vocabulary, one glyph per verb so every surface draws the same one.
pub mod icons {
    pub use egui_phosphor::light::{
        ARROW_ARC_RIGHT as STEP_OVER, ARROW_CLOCKWISE as RESTART, ARROW_LINE_DOWN as STEP_IN,
        ARROW_LINE_UP as STEP_OUT, BUG as DEBUG, CPU as INSPECT, FOLDER_SIMPLE as FILES,
        GEAR_SIX as SETTINGS, HAMMER as BUILD, MAGNIFYING_GLASS as SEARCH, PAUSE, PLAY as RUN,
        STOP, TERMINAL_WINDOW as TERMINAL,
    };
}

/// A tracked, uppercase instrument label (panel eyebrows, status, rung names).
pub fn label(text: &str) -> RichText {
    let spaced: String = text
        .to_uppercase()
        .chars()
        .flat_map(|c| [c, '\u{2009}'])
        .collect();
    RichText::new(spaced)
        .font(FontId::proportional(10.5))
        .color(TEXT_DIM)
}

/// Monospace code/data text at a given size.
pub fn mono(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text.into())
        .font(FontId::monospace(size))
        .color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_selects_the_dark_palette() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let style = ctx.global_style();

        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(style.visuals.panel_fill, CANVAS);
        assert_eq!(style.visuals.widgets.inactive.bg_fill, SURFACE);
        assert_eq!(style.visuals.widgets.inactive.weak_bg_fill, SURFACE);
        assert_eq!(style.visuals.text_color(), TEXT);
    }
}
