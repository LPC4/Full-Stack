use super::{
    FB_DEFAULT_SCALE, FB_HEIGHT, FB_MIN_SCALE, FB_WIDTH, FullStackApp, Sense, egui, metrics, theme,
    vec2,
};

impl FullStackApp {
    // The display stays inside the IDE so the terminal and framebuffer remain visible together.
    pub(in crate::app) fn framebuffer_window(&mut self, ctx: &egui::Context) {
        let show = self.live.as_ref().is_some_and(|live| live.state.is_alive())
            && self.config_has_screen();
        if !show {
            return;
        }
        let title = format!(
            "{} \u{00B7} screen",
            self.session_tab_label.as_deref().unwrap_or("machine")
        );
        let close_requested = {
            let mut open = true;
            let min_size = vec2(
                FB_WIDTH as f32 * FB_MIN_SCALE,
                FB_HEIGHT as f32 * FB_MIN_SCALE,
            );
            let max_size = (ctx.content_rect().size() - vec2(32.0, 64.0)).max(min_size);
            egui::Window::new(title)
                .id(egui::Id::new("machine_framebuffer_window"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .default_size([
                    FB_WIDTH as f32 * FB_DEFAULT_SCALE,
                    FB_HEIGHT as f32 * FB_DEFAULT_SCALE,
                ])
                .min_size(min_size)
                .max_size(max_size)
                .resizable(true)
                .collapsible(false)
                .constrain_to(ctx.content_rect())
                .open(&mut open)
                .frame(
                    egui::Frame::new()
                        .fill(theme::CHROME)
                        .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
                        .inner_margin(egui::Margin::ZERO),
                )
                .show(ctx, |ui| self.framebuffer_window_body(ui));
            !open
        };

        if close_requested {
            self.stop_live_machine();
        }
    }

    fn framebuffer_window_body(&mut self, ui: &mut egui::Ui) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        // Key the texture on the content version, not FILL: demos that draw
        // pixels directly (mandelbrot) never touch FILL but must still refresh.
        let version = live.framebuffer_version();
        let pixels = live.framebuffer().to_vec();
        self.upload_framebuffer(ui.ctx(), version, &pixels);
        let Some(texture) = self.machine_texture.as_ref() else {
            return;
        };
        let available = ui.available_size_before_wrap().max(vec2(0.0, 0.0)).floor();
        let (rect, response) = ui.allocate_exact_size(available, Sense::click());
        if !rect.is_positive() {
            return;
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "machine framebuffer")
        });
        if response.clicked() {
            response.request_focus();
        }
        let focused = response.has_focus() || response.clicked();
        if focused {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
        }

        let size = fit_framebuffer_size(rect.size());
        let image_rect = egui::Rect::from_center_size(rect.center(), size.floor());
        ui.painter().rect_filled(rect, 0.0, theme::BEZEL);
        ui.painter().rect_filled(image_rect, 0.0, theme::BEZEL);
        ui.put(
            image_rect,
            egui::Image::new((texture.id(), image_rect.size()))
                .texture_options(egui::TextureOptions::NEAREST),
        );
        if version == 0 {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "waiting for framebuffer",
                egui::FontId::monospace(metrics::STATUS_TEXT_SIZE),
                theme::TEXT_FAINT,
            );
        }
        if focused {
            ui.painter().rect_stroke(
                image_rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, theme::ELECTRIC),
                egui::StrokeKind::Inside,
            );
            self.route_framebuffer_keys(ui);
        }
        ui.ctx().request_repaint();
    }

    fn upload_framebuffer(&mut self, ctx: &egui::Context, key: u64, pixels: &[u8]) {
        if self.machine_texture.is_some() && self.machine_texture_fill_count == Some(key) {
            return;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([FB_WIDTH, FB_HEIGHT], pixels);
        match self.machine_texture.as_mut() {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.machine_texture = Some(ctx.load_texture(
                    "machine_framebuffer",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        self.machine_texture_fill_count = Some(key);
    }

    fn route_framebuffer_keys(&mut self, ui: &egui::Ui) {
        let events = ui.input(|i| i.events.clone());
        let Some(live) = self.live.as_mut() else {
            return;
        };
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    for ch in text.chars() {
                        if ch.is_ascii_alphabetic() || ch == ' ' {
                            continue;
                        }
                        if let Some(code) = printable_scancode(ch) {
                            live.push_key_event(code, true);
                        }
                    }
                }
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    repeat,
                    ..
                } => {
                    if repeat {
                        continue;
                    }
                    if let Some(code) = special_scancode(key) {
                        live.push_key_event(code, pressed);
                    }
                    if let Some(code) = key_scancode(key) {
                        live.push_key_event(code, pressed);
                    }
                    if pressed
                        && (modifiers.command || modifiers.ctrl)
                        && let Some(ctrl) = ctrl_scancode(key)
                    {
                        live.push_uart(ctrl as u8);
                    }
                }
                // egui-winit swallows Ctrl+C/Ctrl+X into Copy/Cut (no Key event),
                // so mirror them to the guest UART as control bytes.
                egui::Event::Copy => live.push_uart(0x03),
                egui::Event::Cut => live.push_uart(0x18),
                _ => {}
            }
        }
    }
}

pub(super) fn fit_framebuffer_size(available: egui::Vec2) -> egui::Vec2 {
    if available.x <= 0.0 || available.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    let aspect = FB_WIDTH as f32 / FB_HEIGHT as f32;
    if available.x / available.y > aspect {
        vec2(available.y * aspect, available.y)
    } else {
        vec2(available.x, available.x / aspect)
    }
}

pub(super) fn printable_scancode(ch: char) -> Option<u16> {
    if ch.is_ascii() && !ch.is_control() {
        Some(ch as u16)
    } else {
        None
    }
}

pub(super) fn special_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::ArrowUp => Some(0x80),
        egui::Key::ArrowDown => Some(0x81),
        egui::Key::ArrowLeft => Some(0x82),
        egui::Key::ArrowRight => Some(0x83),
        egui::Key::Enter => Some(b'\n' as u16),
        egui::Key::Backspace => Some(8),
        egui::Key::Tab => Some(b'\t' as u16),
        egui::Key::Escape => Some(27),
        _ => None,
    }
}

pub(super) fn key_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::A => Some(b'A' as u16),
        egui::Key::B => Some(b'B' as u16),
        egui::Key::C => Some(b'C' as u16),
        egui::Key::D => Some(b'D' as u16),
        egui::Key::E => Some(b'E' as u16),
        egui::Key::F => Some(b'F' as u16),
        egui::Key::G => Some(b'G' as u16),
        egui::Key::H => Some(b'H' as u16),
        egui::Key::I => Some(b'I' as u16),
        egui::Key::J => Some(b'J' as u16),
        egui::Key::K => Some(b'K' as u16),
        egui::Key::L => Some(b'L' as u16),
        egui::Key::M => Some(b'M' as u16),
        egui::Key::N => Some(b'N' as u16),
        egui::Key::O => Some(b'O' as u16),
        egui::Key::P => Some(b'P' as u16),
        egui::Key::Q => Some(b'Q' as u16),
        egui::Key::R => Some(b'R' as u16),
        egui::Key::S => Some(b'S' as u16),
        egui::Key::T => Some(b'T' as u16),
        egui::Key::U => Some(b'U' as u16),
        egui::Key::V => Some(b'V' as u16),
        egui::Key::W => Some(b'W' as u16),
        egui::Key::X => Some(b'X' as u16),
        egui::Key::Y => Some(b'Y' as u16),
        egui::Key::Z => Some(b'Z' as u16),
        egui::Key::Space => Some(b' ' as u16),
        _ => None,
    }
}

pub(in crate::app) fn ctrl_scancode(key: egui::Key) -> Option<u16> {
    match key {
        egui::Key::A => Some(0x01),
        egui::Key::B => Some(0x02),
        egui::Key::C => Some(0x03),
        egui::Key::D => Some(0x04),
        egui::Key::E => Some(0x05),
        egui::Key::F => Some(0x06),
        egui::Key::G => Some(0x07),
        egui::Key::H => Some(0x08),
        egui::Key::I => Some(0x09),
        egui::Key::J => Some(0x0A),
        egui::Key::K => Some(0x0B),
        egui::Key::L => Some(0x0C),
        egui::Key::M => Some(0x0D),
        egui::Key::N => Some(0x0E),
        egui::Key::O => Some(0x0F),
        egui::Key::P => Some(0x10),
        egui::Key::Q => Some(0x11),
        egui::Key::R => Some(0x12),
        egui::Key::S => Some(0x13),
        egui::Key::T => Some(0x14),
        egui::Key::U => Some(0x15),
        egui::Key::V => Some(0x16),
        egui::Key::W => Some(0x17),
        egui::Key::X => Some(0x18),
        egui::Key::Y => Some(0x19),
        egui::Key::Z => Some(0x1A),
        _ => None,
    }
}
