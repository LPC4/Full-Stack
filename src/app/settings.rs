use full_stack::view::{BgPreset, apply_ui_theme, set_ui_theme};

use super::{AccentPreset, AppSettings, FontScale, FullStackApp};

impl FullStackApp {
    pub(super) fn apply_settings(&self, ctx: &egui::Context) {
        let theme = self.settings.accent.theme().with_background(self.settings.bg);
        set_ui_theme(theme);
        apply_ui_theme(ctx);
        ctx.set_zoom_factor(self.settings.font_scale.zoom());
    }

    pub(super) fn settings_window_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;

        ui.heading("Appearance");
        ui.add_space(6.0);

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
                if response.clicked() {
                    self.settings.accent = preset;
                    changed = true;
                }
            }
        });

        ui.add_space(10.0);
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
                    // outer square = canvas color, inner circle = panel color
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
                if response.clicked() {
                    self.settings.bg = preset;
                    changed = true;
                }
            }
        });

        ui.add_space(10.0);
        ui.label("Font size");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for scale in [FontScale::Small, FontScale::Medium, FontScale::Large, FontScale::Larger]
            {
                if ui
                    .selectable_label(self.settings.font_scale == scale, scale.label())
                    .clicked()
                {
                    self.settings.font_scale = scale;
                    changed = true;
                }
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.heading("Execution");
        ui.add_space(6.0);

        ui.label("Max VM steps");
        ui.add_space(4.0);
        let mut steps = self.settings.max_vm_steps as f64;
        if ui
            .add(
                egui::DragValue::new(&mut steps)
                    .range(1.0..=500_000_000.0)
                    .speed(50_000.0),
            )
            .changed()
        {
            self.settings.max_vm_steps = steps as u64;
        }

        ui.add_space(10.0);
        ui.separator();
        if ui.button("Reset to defaults").clicked() {
            self.settings = AppSettings::default();
            changed = true;
        }

        if changed {
            self.apply_settings(ui.ctx());
        }
    }
}
