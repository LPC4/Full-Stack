//! The Stack Spine: compact navigation through the compilation stages.

use crate::ui::{Layer, theme};
use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, vec2};

const RAIL_WIDTH: f32 = 58.0;
const NODE_R: f32 = 4.5;
const PAD_Y: f32 = 26.0;
// Fixed rung pitch: the rungs must not re-flow when the terminal or other
// panels change the column height, only compress when space truly runs out.
const RUNG_PITCH: f32 = 118.0;
const TICK_PITCH: f32 = 12.0;
const TICK_HALF: f32 = 3.0;

/// What the spine renders this frame; `pulse_t` is the 0..1 progress of the
/// build sweep (`None` when idle).
#[derive(Clone, Copy)]
pub struct SpineProps<'a> {
    pub layers: &'a [Layer],
    pub active: Layer,
    pub pulse_t: Option<f32>,
    pub build_lit_until: Option<Layer>,
    pub always_enabled_until: Option<Layer>,
}

/// Draw the spine; returns the rung the user clicked this frame, if any.
pub fn show(ui: &mut egui::Ui, props: SpineProps<'_>) -> Option<Layer> {
    let SpineProps {
        layers,
        active,
        pulse_t,
        build_lit_until,
        always_enabled_until,
    } = props;
    // Fill the whole strip and center the rail in it, so panel margins never leave a
    // mismatched band behind the rail.
    let size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::CANVAS);

    let x = rect.center().x;
    let top = rect.top() + PAD_Y;
    let bottom = rect.bottom() - PAD_Y;
    let n = layers.len();
    if n == 0 {
        return None;
    }
    // Anchor at the top with a fixed pitch; compress only when the column is
    // genuinely too short. This keeps every rung in place across layout changes.
    let step = if n > 1 {
        RUNG_PITCH.min(((bottom - top) / (n as f32 - 1.0)).max(0.0))
    } else {
        0.0
    };
    let center = |i: usize| Pos2::new(x, top + step * i as f32);
    // The whole column is interactive: any y maps to the nearest rung.
    let rung_at = |y: f32| {
        if n <= 1 || step <= f32::EPSILON {
            0
        } else {
            ((y - top) / step).round().clamp(0.0, (n - 1) as f32) as usize
        }
    };

    let hovered = response.hover_pos().map(|p| rung_at(p.y));
    let hovered_enabled = hovered
        .map(|i| stage_enabled(layers, i, build_lit_until, always_enabled_until))
        .unwrap_or(false);
    if hovered_enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    draw_tail_ticks(&painter, x, center(n - 1).y, rect.bottom());
    draw_connectors(&painter, layers, pulse_t, build_lit_until, &center);
    draw_rungs(
        &painter,
        layers,
        RungPaintState {
            active,
            hovered,
            pulse_t,
            build_lit_until,
            always_enabled_until,
        },
        &center,
    );

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let i = rung_at(pos.y);
        if stage_enabled(layers, i, build_lit_until, always_enabled_until) {
            return Some(layers[i]);
        }
    }
    None
}

fn draw_tail_ticks(painter: &egui::Painter, x: f32, last_rung_y: f32, bottom: f32) {
    // Ruler graduations continue below the last rung so the rail still reads
    // as a full-height instrument whatever the column height is.
    let mut tick_y = last_rung_y + NODE_R + 20.0;
    while tick_y < bottom - 10.0 {
        painter.line_segment(
            [
                Pos2::new(x - TICK_HALF, tick_y),
                Pos2::new(x + TICK_HALF, tick_y),
            ],
            Stroke::new(1.0, theme::HAIRLINE_SOFT),
        );
        tick_y += TICK_PITCH;
    }
}

fn draw_connectors(
    painter: &egui::Painter,
    layers: &[Layer],
    pulse_t: Option<f32>,
    build_lit_until: Option<Layer>,
    center: &impl Fn(usize) -> Pos2,
) {
    for i in 0..layers.len() - 1 {
        let build = build_strength(layers, pulse_t, i + 1, build_lit_until);
        let col = if build > 0.0 {
            dim(layers[i + 1].color(), 0.42 + build * 0.30)
        } else {
            dim(layers[i + 1].color(), 0.34)
        };
        painter.line_segment([center(i), center(i + 1)], Stroke::new(1.25, col));
    }
}

#[derive(Clone, Copy)]
struct RungPaintState {
    active: Layer,
    hovered: Option<usize>,
    pulse_t: Option<f32>,
    build_lit_until: Option<Layer>,
    always_enabled_until: Option<Layer>,
}

fn draw_rungs(
    painter: &egui::Painter,
    layers: &[Layer],
    state: RungPaintState,
    center: &impl Fn(usize) -> Pos2,
) {
    for (i, layer) in layers.iter().enumerate() {
        let c = center(i);
        let is_active = *layer == state.active;
        let is_enabled =
            stage_enabled(layers, i, state.build_lit_until, state.always_enabled_until);
        let is_hover = state.hovered == Some(i) && !is_active && is_enabled;
        let build = build_strength(layers, state.pulse_t, i, state.build_lit_until);
        draw_rung_node(
            painter,
            c,
            layer.color(),
            is_active,
            is_enabled,
            is_hover,
            build,
        );
        draw_rung_label(painter, c, layer, is_active, is_enabled, is_hover, build);
    }
}

fn draw_rung_node(
    painter: &egui::Painter,
    c: Pos2,
    col: Color32,
    is_active: bool,
    is_enabled: bool,
    is_hover: bool,
    build: f32,
) {
    if is_active {
        // A soft halo behind the active node keeps the current stage loudest.
        painter.circle_filled(c, NODE_R + 6.0, dim(col, 0.14));
        painter.circle_filled(c, NODE_R + 1.5, dim(col, 0.88));
        painter.circle_stroke(c, NODE_R + 3.5, Stroke::new(1.0, dim(col, 0.65)));
    } else if build > 0.0 {
        painter.circle_filled(c, NODE_R + 0.5, dim(col, 0.44 + build * 0.36));
        painter.circle_stroke(
            c,
            NODE_R + 2.0,
            Stroke::new(1.0, dim(col, 0.36 + build * 0.34)),
        );
    } else if is_hover {
        painter.circle_filled(c, NODE_R + 0.5, dim(col, 0.56));
        painter.circle_stroke(c, NODE_R + 2.5, Stroke::new(1.0, dim(col, 0.72)));
    } else if !is_enabled {
        painter.circle_filled(c, NODE_R - 0.5, dim(col, 0.3));
        painter.circle_stroke(c, NODE_R, Stroke::new(1.0, theme::HAIRLINE));
    } else {
        painter.circle_filled(c, NODE_R, dim(col, 0.48));
        painter.circle_stroke(c, NODE_R + 1.0, Stroke::new(1.0, dim(col, 0.5)));
    }
}

fn draw_rung_label(
    painter: &egui::Painter,
    c: Pos2,
    layer: &Layer,
    is_active: bool,
    is_enabled: bool,
    is_hover: bool,
    build: f32,
) {
    // One label per rung: the stage name under the node in its spectrum color.
    let col = layer.color();
    let name_col = if is_active {
        col
    } else if build > 0.0 {
        dim(col, 0.52 + build * 0.24)
    } else if is_hover {
        dim(col, 0.76)
    } else if !is_enabled {
        dim(theme::TEXT_FAINT, 0.8)
    } else {
        dim(col, 0.5)
    };
    painter.text(
        c + vec2(0.0, NODE_R + 11.0),
        Align2::CENTER_CENTER,
        layer.name(),
        FontId::monospace(if is_active { 10.5 } else { 9.5 }),
        name_col,
    );
}

fn stage_enabled(
    layers: &[Layer],
    i: usize,
    build_lit_until: Option<Layer>,
    always_enabled_until: Option<Layer>,
) -> bool {
    let reached = |until: Layer| {
        layers
            .iter()
            .position(|layer| *layer == until)
            .is_some_and(|max| i <= max)
    };
    always_enabled_until.is_some_and(reached) || build_lit_until.is_some_and(reached)
}

// Built rungs hold a steady base; the pulse is additive, so a rebuild never goes dark.
fn build_strength(
    layers: &[Layer],
    pulse_t: Option<f32>,
    i: usize,
    build_lit_until: Option<Layer>,
) -> f32 {
    const BUILT: f32 = 0.58;
    const BOOST: f32 = 0.42;
    let Some(lit_until) = build_lit_until else {
        return 0.0;
    };
    let Some(max_stage) = layers.iter().position(|layer| *layer == lit_until) else {
        return 0.0;
    };
    if i > max_stage {
        return 0.0;
    }
    let Some(t) = pulse_t else {
        return BUILT;
    };
    if max_stage == 0 {
        return BUILT + (1.0 - t.clamp(0.0, 1.0)) * BOOST;
    }
    let head = t.clamp(0.0, 1.0) * max_stage as f32;
    let dist = (i as f32 - head).abs();
    BUILT + (1.0 - dist).clamp(0.0, 1.0) * BOOST
}

fn dim(c: Color32, amount: f32) -> Color32 {
    let bg = theme::CANVAS;
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount) as u8;
    Color32::from_rgb(
        lerp(bg.r(), c.r()),
        lerp(bg.g(), c.g()),
        lerp(bg.b(), c.b()),
    )
}

/// Width the spine reserves, so the shell can lay out around it.
pub const fn width() -> f32 {
    RAIL_WIDTH
}
