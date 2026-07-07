use egui::Color32;

/// Token colors used by the syntax highlighters. A future editor can swap this
/// for a user theme; for now a single dark palette is provided.
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

impl SyntaxPalette {
    pub fn dark() -> Self {
        Self {
            keyword: Color32::from_rgb(200, 110, 240),
            string: Color32::from_rgb(104, 170, 110),
            number: Color32::from_rgb(110, 170, 240),
            comment: Color32::from_rgb(86, 92, 105),
            label: Color32::from_rgb(224, 186, 100),
            register: Color32::from_rgb(112, 190, 255),
            bracket: Color32::from_rgb(232, 210, 120),
            directive: Color32::from_rgb(255, 160, 110),
            identifier: Color32::from_rgb(180, 220, 180),
        }
    }
}

impl Default for SyntaxPalette {
    fn default() -> Self {
        Self::dark()
    }
}
