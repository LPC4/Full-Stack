use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use crate::virtual_machine::bus::RAM_BASE;
use egui::{Color32, ColorImage, RichText, TextureHandle, TextureOptions, Ui};

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
    mode: FbMode,
    #[allow(clippy::option_option)]
    texture: Option<TextureHandle>,
}

impl Default for FramebufferView {
    fn default() -> Self {
        Self {
            addr_input: format!("{RAM_BASE:#010x}"),
            base_addr: RAM_BASE,
            width: 120,
            height: 27,
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
        let Some(session) = state.debug_session.as_mut() else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        // ---- Config bar ----
        ui.horizontal(|ui| {
            ui.label("Base addr:");
            let resp = ui.text_edit_singleline(&mut self.addr_input);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let cleaned = self.addr_input.trim().trim_start_matches("0x");
                if let Ok(addr) = u64::from_str_radix(cleaned, 16) {
                    self.base_addr = addr;
                    self.texture = None;
                }
            }

            ui.separator();
            ui.label("W:");
            let mut w_str = self.width.to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut w_str).desired_width(40.0))
                .changed()
            {
                if let Ok(v) = w_str.parse::<usize>() {
                    self.width = v.clamp(1, 1024);
                    self.texture = None;
                }
            }
            ui.label("H:");
            let mut h_str = self.height.to_string();
            if ui
                .add(egui::TextEdit::singleline(&mut h_str).desired_width(40.0))
                .changed()
            {
                if let Ok(v) = h_str.parse::<usize>() {
                    self.height = v.clamp(1, 768);
                    self.texture = None;
                }
            }

            ui.separator();
            ui.radio_value(&mut self.mode, FbMode::Text, "Text");
            ui.radio_value(&mut self.mode, FbMode::Pixel, "Pixel");
        });

        ui.add_space(6.0);

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
    fn render_text(&self, ui: &mut Ui, session: &mut crate::view::debug::DebugSession) {
        let bytes_needed = self.width * self.height;
        let bytes = session.peek_bytes(self.base_addr, bytes_needed);

        egui::ScrollArea::both().show(ui, |ui| {
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
            ui.label(RichText::new(text).monospace());
        });
    }

    fn render_pixel(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        session: &mut crate::view::debug::DebugSession,
    ) {
        let bytes_needed = self.width * self.height * 4; // RGBA
        let bytes = session.peek_bytes(self.base_addr, bytes_needed);

        // Build ColorImage from RGBA bytes
        let mut pixels = Vec::with_capacity(self.width * self.height);
        for chunk in bytes.chunks_exact(4) {
            pixels.push(Color32::from_rgba_premultiplied(
                chunk[0], chunk[1], chunk[2], chunk[3],
            ));
        }
        // Pad if the region wasn't fully populated
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
