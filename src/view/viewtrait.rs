use egui::Context;
use super::CompilationState;

pub trait CompilerView {
    fn title(&self) -> &'static str;
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &Context, state: &mut CompilationState);
}