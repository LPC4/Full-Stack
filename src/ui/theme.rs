//! Instrument-panel theme: a quiet desaturated iron-blue base so the depth
//! spectrum (see [`crate::ui::Layer`]) is the only loud thing on screen.

use egui::{Color32, FontId, Mesh, Rect, RichText, Stroke, epaint::Vertex};

pub const BEZEL: Color32 = Color32::from_rgb(0x09, 0x0B, 0x0F);
pub const WELL: Color32 = Color32::from_rgb(0x0D, 0x10, 0x15);
pub const WELL_FLOOR: Color32 = Color32::from_rgb(0x0A, 0x0D, 0x12);
pub const CANVAS: Color32 = Color32::from_rgb(0x12, 0x16, 0x1C);
pub const TERMINAL: Color32 = Color32::from_rgb(0x07, 0x0A, 0x0E);
pub const CHROME: Color32 = Color32::from_rgb(0x17, 0x1B, 0x24);
pub const SURFACE: Color32 = Color32::from_rgb(0x1E, 0x24, 0x30);
pub const SURFACE_ALT: Color32 = Color32::from_rgb(0x26, 0x2D, 0x3B);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x2C, 0x33, 0x40);
pub const HAIRLINE_SOFT: Color32 = Color32::from_rgb(0x1B, 0x20, 0x29);

pub const TEXT: Color32 = Color32::from_rgb(0xD7, 0xDC, 0xE4);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x79, 0x82, 0x8F);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x4C, 0x55, 0x63);
pub const ELECTRIC: Color32 = Color32::from_rgb(0x3F, 0xB6, 0xE6);
pub const DANGER: Color32 = Color32::from_rgb(0xD9, 0x6A, 0x5A);
pub const WARNING: Color32 = Color32::from_rgb(0xD9, 0xAA, 0x5A);
pub const CURRENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x14, 0x18, 0x22, 0x7A);
pub const FIND_MATCH: Color32 = Color32::from_rgba_premultiplied(0x0E, 0x28, 0x33, 0x38);
pub const FIND_MATCH_CURRENT: Color32 = Color32::from_rgba_premultiplied(0x57, 0x3C, 0x20, 0x5C);
pub const DANGER_WASH: Color32 = Color32::from_rgba_premultiplied(0x29, 0x14, 0x11, 0x30);
pub const WARNING_WASH: Color32 = Color32::from_rgba_premultiplied(0x29, 0x20, 0x11, 0x30);

/// Apply the instrument theme: fonts, dark base, hairline strokes, sharp corners,
/// tight spacing. Call once after the egui context exists.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.window_fill = CHROME;
    v.panel_fill = CANVAS;
    v.extreme_bg_color = BEZEL;
    v.faint_bg_color = SURFACE;
    v.code_bg_color = CANVAS;
    v.window_stroke = Stroke::new(1.0, HAIRLINE);
    v.selection.bg_fill = Color32::from_rgb(0x24, 0x3A, 0x55);
    v.selection.stroke = Stroke::new(1.0, ELECTRIC);
    v.hyperlink_color = ELECTRIC;

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
    v.widgets.hovered.bg_fill = SURFACE_ALT;
    v.widgets.hovered.weak_bg_fill = SURFACE_ALT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, TEXT_FAINT);
    v.widgets.active.bg_fill = SURFACE_ALT;
    v.widgets.active.weak_bg_fill = SURFACE_ALT;

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(6.0, 6.0);
    s.button_padding = egui::vec2(8.0, 4.0);
    style.interaction.resize_grab_radius_side = 8.0;

    ctx.set_global_style(style);
}

/// Fill `rect` with a top-to-bottom gradient from `top` to `bottom`. egui has no
/// gradient primitive, so paint a two-color quad.
pub fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    let vertex = |pos, color| Vertex {
        pos,
        uv: egui::epaint::WHITE_UV,
        color,
    };
    mesh.vertices.push(vertex(rect.left_top(), top));
    mesh.vertices.push(vertex(rect.right_top(), top));
    mesh.vertices.push(vertex(rect.right_bottom(), bottom));
    mesh.vertices.push(vertex(rect.left_bottom(), bottom));
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(mesh);
}

/// Register Space Grotesk (chrome) + IBM Plex Mono (code); egui's built-ins stay
/// as glyph fallback for symbols the design faces lack.
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
