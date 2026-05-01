// Shared UI building blocks used across all views.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewType {
    Source,
    Tokens,
    AST,
    IR,
    Assembly,
}

impl ViewType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "Source Code",
            Self::Tokens => "Lexer Tokens",
            Self::AST => "Parser AST",
            Self::IR => "Intermediate Repr.",
            Self::Assembly => "Assembly Code",
        }
    }
}

/// Shows a dimmed message centred in the available space.  Used by views that
/// have nothing to display yet (empty compilation state).
pub fn centered_placeholder(ui: &mut egui::Ui, message: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(egui::RichText::new(message).weak());
    });
}

/// Renders a pre-highlighted `LayoutJob` inside a scrollable area that is
/// uniquely keyed to `panel_id` so multiple instances of the same view type
/// never share scroll state.
pub fn scrollable_code(ui: &mut egui::Ui, panel_id: egui::Id, job: egui::text::LayoutJob) {
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    egui::ScrollArea::both()
        .id_salt(panel_id)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.label(galley);
        });
}
