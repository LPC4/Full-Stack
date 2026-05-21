use crate::view::{CompilationState, CompilerView, ProgramCatalog, ui_theme};
use egui::{Color32, ColorImage, Frame, RichText, Stroke, TextureHandle, TextureOptions, Ui};
use virtual_machine::bus::RAM_BASE;

const TERM_BG: Color32 = Color32::from_rgb(7, 9, 12);
const TERM_FG: Color32 = Color32::from_rgb(185, 210, 185);

#[derive(Clone, PartialEq, Eq)]
pub enum FbMode {
    Text,
    Pixel,
}

#[derive(Clone)]
pub struct FramebufferView {
    addr_input: String,
    base_addr: u64,
    width: usize,
    height: usize,
    width_input: String,
    height_input: String,
    mode: FbMode,
    #[expect(clippy::option_option)]
    texture: Option<TextureHandle>,
}

impl Default for FramebufferView {
    fn default() -> Self {
        Self {
            addr_input: format!("{RAM_BASE:#010x}"),
            base_addr: RAM_BASE,
            width: 120,
            height: 27,
            width_input: "120".to_owned(),
            height_input: "27".to_owned(),
            mode: FbMode::Text,
            texture: None,
        }
    }
}

impl CompilerView for FramebufferView {
    fn title(&self) -> &'static str {
        "Framebuffer"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let Some(session) = state.debug_session.as_ref() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let theme = ui_theme();

        // Config strip
        Frame::NONE
            .fill(theme.surface_alt)
            .stroke(Stroke::new(1.0, theme.border_soft))
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Address").small().color(theme.text_dim));
                    let addr_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.addr_input)
                            .desired_width(110.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("0x80000000"),
                    );
                    if addr_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let s = self.addr_input.trim().trim_start_matches("0x");
                        if let Ok(addr) = u64::from_str_radix(s, 16) {
                            self.base_addr = addr;
                            self.texture = None;
                        }
                    }

                    ui.separator();

                    ui.label(RichText::new("W").small().color(theme.text_dim));
                    let w_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.width_input)
                            .desired_width(42.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if w_resp.lost_focus() {
                        if let Ok(v) = self.width_input.trim().parse::<usize>() {
                            self.width = v.clamp(1, 1024);
                            self.texture = None;
                        }
                        self.width_input = self.width.to_string();
                    }

                    ui.label(RichText::new("H").small().color(theme.text_dim));
                    let h_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.height_input)
                            .desired_width(42.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    if h_resp.lost_focus() {
                        if let Ok(v) = self.height_input.trim().parse::<usize>() {
                            self.height = v.clamp(1, 768);
                            self.texture = None;
                        }
                        self.height_input = self.height.to_string();
                    }

                    ui.separator();

                    // Mode toggle
                    ui.selectable_value(&mut self.mode, FbMode::Text, "Text");
                    ui.selectable_value(&mut self.mode, FbMode::Pixel, "Pixel");

                    // Dimension and memory info on right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let info = match self.mode {
                            FbMode::Pixel => {
                                let bytes = self.width * self.height * 4;
                                format!("{}x{}  4 bpp  {}KB", self.width, self.height, bytes / 1024)
                            }
                            FbMode::Text => {
                                format!("{}x{}", self.width, self.height)
                            }
                        };
                        ui.label(
                            RichText::new(info)
                                .small()
                                .monospace()
                                .color(theme.text_dim),
                        );
                    });
                });
            });

        ui.add_space(4.0);

        match self.mode {
            FbMode::Text => self.render_text(ui, session),
            FbMode::Pixel => self.render_pixel(ui, ctx, session),
        }
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

impl FramebufferView {
    fn render_text(&self, ui: &mut Ui, session: &crate::view::debug::DebugSession) {
        let theme = ui_theme();
        let bytes_needed = self.width * self.height;
        let bytes = session.peek_bytes_raw(self.base_addr, bytes_needed);

        let mut text = String::with_capacity(bytes_needed + self.height);
        for row in bytes.chunks(self.width) {
            for &b in row {
                text.push(if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                });
            }
            text.push('\n');
        }

        let avail = ui.available_height();
        Frame::NONE
            .fill(TERM_BG)
            .stroke(Stroke::new(1.0, theme.border_soft))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), avail));
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.label(RichText::new(text).monospace().color(TERM_FG));
                });
            });
    }

    fn render_pixel(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        session: &crate::view::debug::DebugSession,
    ) {
        let bytes_needed = self.width * self.height * 4; // RGBA
        let bytes = session.peek_bytes_raw(self.base_addr, bytes_needed);

        let mut pixels = Vec::with_capacity(self.width * self.height);
        for chunk in bytes.chunks_exact(4) {
            pixels.push(Color32::from_rgba_premultiplied(
                chunk[0], chunk[1], chunk[2], chunk[3],
            ));
        }
        while pixels.len() < self.width * self.height {
            pixels.push(Color32::BLACK);
        }

        let image = ColorImage::new([self.width, self.height], pixels);

        let texture = self.texture.get_or_insert_with(|| {
            ctx.load_texture("framebuffer", image.clone(), TextureOptions::NEAREST)
        });
        texture.set(image, TextureOptions::NEAREST);

        let avail = ui.available_size();
        let scale = (avail.x / self.width as f32)
            .min(avail.y / self.height as f32)
            .max(1.0);
        let display_size = egui::vec2(self.width as f32 * scale, self.height as f32 * scale);
        ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
    }
}
