use crate::high_level_language::view::HighLevelLanguageView;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateApp {
    high_level_language: HighLevelLanguageView,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            high_level_language: HighLevelLanguageView::default(),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: Self = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();

        app.high_level_language.post_load();
        app.high_level_language.compile();
        app
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.high_level_language.ui(ui, frame);
    }

    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.high_level_language.prepare_for_save();
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
