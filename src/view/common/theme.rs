// Theme definitions and UI styling

use std::sync::{LazyLock, Mutex};

use egui::{Color32, Frame, Stroke};

// --- Background presets ---

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BgPreset {
    #[default]
    Dark,
    Darker,
    Midnight,
    Slate,
    Warm,
}

impl BgPreset {
    pub const ALL: &'static [Self] = &[
        Self::Dark,
        Self::Darker,
        Self::Midnight,
        Self::Slate,
        Self::Warm,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Darker => "Darker",
            Self::Midnight => "Midnight",
            Self::Slate => "Slate",
            Self::Warm => "Warm",
        }
    }

    pub fn palette(self) -> (Color32, Color32, Color32, Color32, Color32) {
        // (canvas, panel, panel_alt, surface, surface_alt)
        match self {
            Self::Dark => (
                Color32::from_rgb(13, 15, 20),
                Color32::from_rgb(18, 20, 28),
                Color32::from_rgb(24, 27, 37),
                Color32::from_rgb(27, 31, 42),
                Color32::from_rgb(33, 37, 50),
            ),
            Self::Darker => (
                Color32::from_rgb(7, 8, 11),
                Color32::from_rgb(10, 11, 16),
                Color32::from_rgb(14, 16, 22),
                Color32::from_rgb(17, 19, 27),
                Color32::from_rgb(21, 24, 33),
            ),
            Self::Midnight => (
                Color32::from_rgb(4, 4, 8),
                Color32::from_rgb(8, 8, 14),
                Color32::from_rgb(12, 13, 20),
                Color32::from_rgb(16, 17, 26),
                Color32::from_rgb(21, 22, 34),
            ),
            Self::Slate => (
                Color32::from_rgb(14, 15, 19),
                Color32::from_rgb(20, 22, 28),
                Color32::from_rgb(26, 28, 36),
                Color32::from_rgb(31, 34, 44),
                Color32::from_rgb(38, 42, 54),
            ),
            Self::Warm => (
                Color32::from_rgb(14, 11, 9),
                Color32::from_rgb(20, 16, 13),
                Color32::from_rgb(27, 22, 17),
                Color32::from_rgb(33, 27, 21),
                Color32::from_rgb(41, 34, 26),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SyntaxPalette {
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub label: Color32,
    pub register: Color32,
    pub bracket: Color32,
    pub directive: Color32,
    pub identifier: Color32,
}

#[derive(Clone, Copy, Debug)]
pub struct PipelinePalette {
    pub background: Color32,
    pub cell: Color32,
    pub grid: Color32,
    pub stage: [Color32; 5],
    pub stall: Color32,
    pub flush: Color32,
    pub cycle_text: Color32,
}

#[derive(Clone, Copy, Debug)]
pub struct StackPalette {
    pub background: Color32,
    pub frame: Color32,
    pub return_address: Color32,
    pub saved_register: Color32,
    pub local_variable: Color32,
    pub parameter: Color32,
    pub dim_text: Color32,
    pub label_text: Color32,
}

#[derive(Clone, Copy, Debug)]
pub struct MemoryPalette {
    pub kernel: Color32,
    pub text: Color32,
    pub data: Color32,
    pub bss: Color32,
    pub stack: Color32,
    pub link: Color32,
    pub muted: Color32,
}

#[derive(Clone, Copy, Debug)]
pub struct UiTheme {
    pub canvas: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub text: Color32,
    pub text_soft: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_alt: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub info: Color32,
    pub highlight: Color32,
    pub syntax: SyntaxPalette,
    pub pipeline: PipelinePalette,
    pub stack: StackPalette,
    pub memory: MemoryPalette,
}

impl UiTheme {
    pub fn dark() -> Self {
        Self {
            canvas: Color32::from_rgb(13, 15, 20),
            panel: Color32::from_rgb(18, 20, 28),
            panel_alt: Color32::from_rgb(24, 27, 37),
            surface: Color32::from_rgb(27, 31, 42),
            surface_alt: Color32::from_rgb(33, 37, 50),
            border: Color32::from_rgb(72, 82, 106),
            border_soft: Color32::from_rgb(44, 50, 64),
            text: Color32::from_rgb(232, 236, 244),
            text_soft: Color32::from_rgb(198, 205, 219),
            text_dim: Color32::from_rgb(132, 140, 160),
            accent: Color32::from_rgb(80, 120, 220),
            accent_alt: Color32::from_rgb(126, 104, 240),
            success: Color32::from_rgb(90, 200, 120),
            warning: Color32::from_rgb(232, 182, 72),
            error: Color32::from_rgb(230, 88, 88),
            info: Color32::from_rgb(110, 180, 255),
            highlight: Color32::from_rgb(255, 215, 110),
            syntax: SyntaxPalette {
                keyword: Color32::from_rgb(200, 110, 240),
                string: Color32::from_rgb(104, 170, 110),
                number: Color32::from_rgb(110, 170, 240),
                comment: Color32::from_rgb(86, 92, 105),
                label: Color32::from_rgb(224, 186, 100),
                register: Color32::from_rgb(112, 190, 255),
                bracket: Color32::from_rgb(232, 210, 120),
                directive: Color32::from_rgb(255, 160, 110),
                identifier: Color32::from_rgb(180, 220, 180),
            },
            pipeline: PipelinePalette {
                background: Color32::from_rgb(16, 18, 24),
                cell: Color32::from_rgb(23, 26, 35),
                grid: Color32::from_rgb(40, 46, 60),
                stage: [
                    Color32::from_rgb(92, 156, 255),
                    Color32::from_rgb(120, 118, 255),
                    Color32::from_rgb(176, 110, 255),
                    Color32::from_rgb(255, 110, 170),
                    Color32::from_rgb(255, 170, 100),
                ],
                stall: Color32::from_rgb(236, 184, 72),
                flush: Color32::from_rgb(240, 92, 92),
                cycle_text: Color32::from_rgb(142, 150, 170),
            },
            stack: StackPalette {
                background: Color32::from_rgb(18, 20, 28),
                frame: Color32::from_rgb(44, 50, 64),
                return_address: Color32::from_rgb(232, 92, 92),
                saved_register: Color32::from_rgb(255, 198, 96),
                local_variable: Color32::from_rgb(112, 232, 156),
                parameter: Color32::from_rgb(184, 150, 255),
                dim_text: Color32::from_rgb(132, 140, 160),
                label_text: Color32::from_rgb(224, 186, 100),
            },
            memory: MemoryPalette {
                kernel: Color32::from_rgb(110, 116, 128),
                text: Color32::from_rgb(92, 170, 255),
                data: Color32::from_rgb(188, 132, 232),
                bss: Color32::from_rgb(220, 182, 76),
                stack: Color32::from_rgb(220, 96, 120),
                link: Color32::from_rgb(110, 170, 255),
                muted: Color32::from_rgb(180, 186, 198),
            },
        }
    }

    pub fn with_accent(mut self, accent: Color32, accent_alt: Color32) -> Self {
        self.accent = accent;
        self.accent_alt = accent_alt;
        self
    }

    pub fn with_background(mut self, bg: BgPreset) -> Self {
        let (canvas, panel, panel_alt, surface, surface_alt) = bg.palette();
        self.canvas = canvas;
        self.panel = panel;
        self.panel_alt = panel_alt;
        self.surface = surface;
        self.surface_alt = surface_alt;
        self
    }

    pub fn panel_frame(self) -> Frame {
        Frame::NONE
            .fill(self.panel)
            .stroke(Stroke::new(1.0, self.border_soft))
            .inner_margin(8.0)
    }

    pub fn surface_frame(self) -> Frame {
        Frame::NONE
            .fill(self.surface)
            .stroke(Stroke::new(1.0, self.border_soft))
            .inner_margin(8.0)
    }

    pub fn alert_frame(self, fill: Color32, stroke: Color32) -> Frame {
        Frame::NONE
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke))
            .inner_margin(8.0)
    }
}

static UI_THEME: LazyLock<Mutex<UiTheme>> = LazyLock::new(|| Mutex::new(UiTheme::dark()));

pub fn ui_theme() -> UiTheme {
    *UI_THEME.lock().expect("theme mutex poisoned")
}

pub fn set_ui_theme(theme: UiTheme) {
    *UI_THEME.lock().expect("theme mutex poisoned") = theme;
}

pub fn apply_ui_theme(ctx: &egui::Context) {
    let theme = ui_theme();

    // -- Visuals ---------------------------------------------------------------
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(theme.text);
    visuals.panel_fill = theme.panel;
    visuals.window_fill = theme.panel_alt;
    visuals.extreme_bg_color = theme.canvas;
    visuals.faint_bg_color = theme.panel_alt;
    visuals.hyperlink_color = theme.info;
    visuals.selection.bg_fill = theme.accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, theme.accent);
    visuals.window_stroke = Stroke::new(1.0, theme.accent.gamma_multiply(0.45));
    visuals.window_shadow = egui::Shadow::NONE;

    // Style interactive widgets (buttons, combo-box buttons, etc.)
    // so they match the theme instead of default gray.
    let accent_mult = |f: f32| theme.accent.linear_multiply(f);
    let surface_mult = |f: f32| theme.surface.linear_multiply(f);

    visuals.widgets.inactive.bg_fill = theme.surface_alt;
    visuals.widgets.inactive.weak_bg_fill = theme.surface;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.text_soft);

    visuals.widgets.hovered.bg_fill = accent_mult(0.22);
    visuals.widgets.hovered.weak_bg_fill = accent_mult(0.14);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, theme.text);

    visuals.widgets.active.bg_fill = accent_mult(0.35);
    visuals.widgets.active.weak_bg_fill = accent_mult(0.25);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, theme.text);

    visuals.widgets.open.bg_fill = accent_mult(0.22);
    visuals.widgets.open.weak_bg_fill = accent_mult(0.14);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, theme.text);

    visuals.widgets.noninteractive.bg_fill = surface_mult(0.60);
    visuals.widgets.noninteractive.weak_bg_fill = surface_mult(0.45);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.text_dim);

    ctx.set_visuals(visuals);

    // -- Style ----------------------------------------------------------------
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    ctx.set_global_style(style);
}
