use full_stack::view::{BgPreset, apply_ui_theme, set_ui_theme, ui_theme};

use super::{AccentPreset, AppSettings, FontScale, FullStackApp};

impl FullStackApp {
    pub(super) fn apply_settings(&self, ctx: &egui::Context) {
        let theme = self
            .settings
            .accent
            .theme()
            .with_background(self.settings.bg);
        set_ui_theme(theme);
        apply_ui_theme(ctx);
        ctx.set_zoom_factor(self.settings.font_scale.zoom());
    }

    pub(super) fn settings_window_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;

        // --- Appearance ---
        ui.add_space(4.0);
        ui.heading("Appearance");
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(6.0);

        // -- Accent color
        ui.label("Accent color");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for &preset in AccentPreset::ALL {
                let (color, _) = preset.colors();
                let selected = self.settings.accent == preset;
                let size = egui::vec2(28.0, 28.0);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let response = response.on_hover_text(preset.label());
                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.rect_filled(rect, 6.0, color);
                    if selected {
                        painter.rect_stroke(
                            rect.shrink(2.0),
                            5.0,
                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                if response.clicked() && !selected {
                    self.settings.accent = preset;
                    changed = true;
                }
            }
        });

        ui.add_space(10.0);

        // -- Background
        ui.label("Background");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for &preset in BgPreset::ALL {
                let (canvas, panel, _, _, _) = preset.palette();
                let selected = self.settings.bg == preset;
                let size = egui::vec2(28.0, 28.0);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let response = response.on_hover_text(preset.label());
                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.rect_filled(rect, 6.0, canvas);
                    painter.circle_filled(rect.center(), 7.0, panel);
                    if selected {
                        painter.rect_stroke(
                            rect.shrink(2.0),
                            5.0,
                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                if response.clicked() && !selected {
                    self.settings.bg = preset;
                    changed = true;
                }
            }
        });

        ui.add_space(10.0);

        // -- Font size
        ui.label("Font size");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let scales = [
                FontScale::Small,
                FontScale::Medium,
                FontScale::Large,
                FontScale::Larger,
            ];
            for &scale in &scales {
                if ui
                    .selectable_label(self.settings.font_scale == scale, scale.label())
                    .clicked()
                {
                    self.settings.font_scale = scale;
                    changed = true;
                }
                if scale != *scales.last().unwrap() {
                    ui.add_space(4.0);
                }
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "Current zoom: {:.0}%",
                self.settings.font_scale.zoom() * 100.0
            ))
            .small()
            .weak(),
        );

        // --- Theme preview ---
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label("Theme preview");
        ui.add_space(4.0);

        let theme = ui_theme();
        let preview_w = ui.available_width();
        let preview_h = 56.0;
        let (preview_rect, _) =
            ui.allocate_exact_size(egui::vec2(preview_w, preview_h), egui::Sense::hover());

        if ui.is_rect_visible(preview_rect) {
            let painter = ui.painter();
            // Canvas strip
            painter.rect_filled(
                egui::Rect::from_min_size(
                    preview_rect.min,
                    egui::vec2(preview_w * 0.20, preview_h),
                ),
                0.0,
                theme.canvas,
            );
            // Panel strip
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x + preview_w * 0.20, preview_rect.min.y),
                    egui::vec2(preview_w * 0.20, preview_h),
                ),
                0.0,
                theme.panel,
            );
            // Surface strip
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x + preview_w * 0.40, preview_rect.min.y),
                    egui::vec2(preview_w * 0.20, preview_h),
                ),
                0.0,
                theme.surface,
            );
            // Accent strip
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x + preview_w * 0.60, preview_rect.min.y),
                    egui::vec2(preview_w * 0.20, preview_h),
                ),
                0.0,
                theme.accent,
            );
            // Text preview
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x + preview_w * 0.80, preview_rect.min.y),
                    egui::vec2(preview_w * 0.20, preview_h),
                ),
                0.0,
                theme.surface_alt,
            );
            painter.text(
                preview_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::monospace(16.0),
                theme.text,
            );
        }

        // --- Execution ---
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);
        ui.heading("Execution");
        ui.add_space(6.0);

        ui.label("Max VM steps");
        ui.add_space(4.0);
        let mut steps = self.settings.max_vm_steps as f64;
        let slider_response = ui.add(
            egui::Slider::new(&mut steps, 1.0..=500_000_000.0)
                .logarithmic(true)
                .text("steps"),
        );
        if slider_response.changed() {
            self.settings.max_vm_steps = steps as u64;
        }
        // Show formatted value
        let steps_formatted = if self.settings.max_vm_steps >= 1_000_000 {
            format!("{:.1}M", self.settings.max_vm_steps as f64 / 1_000_000.0)
        } else if self.settings.max_vm_steps >= 1_000 {
            format!("{}k", self.settings.max_vm_steps / 1_000)
        } else {
            self.settings.max_vm_steps.to_string()
        };
        ui.label(
            egui::RichText::new(format!("Current: {steps_formatted} steps"))
                .small()
                .weak(),
        );

        // --- Reset ---
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        if ui
            .add(egui::Button::new("Reset to defaults").min_size(egui::vec2(140.0, 28.0)))
            .clicked()
        {
            self.settings = AppSettings::default();
            changed = true;
        }

        if changed {
            self.apply_settings(ui.ctx());
        }
    }
}
