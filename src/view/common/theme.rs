// Theme definitions and UI styling

use std::sync::OnceLock;

use egui::{Color32, Frame, Stroke};

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

static UI_THEME: OnceLock<UiTheme> = OnceLock::new();

pub fn ui_theme() -> &'static UiTheme {
    UI_THEME.get_or_init(UiTheme::dark)
}

pub fn apply_ui_theme(ctx: &egui::Context) {
    let theme = ui_theme();
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(theme.text);
    visuals.panel_fill = theme.panel;
    visuals.window_fill = theme.panel;
    visuals.extreme_bg_color = theme.canvas;
    visuals.faint_bg_color = theme.panel_alt;
    visuals.hyperlink_color = theme.info;
    visuals.selection.bg_fill = theme.accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, theme.accent);
    ctx.set_visuals(visuals);
}
