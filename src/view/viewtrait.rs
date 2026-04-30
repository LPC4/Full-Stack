// file: src/view/viewtrait.rs
use super::{CompilationState, ProgramCatalog};
use egui::Context;

pub trait CompilerView {
    fn title(&self) -> &'static str;
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        state: &mut CompilationState,
        catalog: &mut ProgramCatalog,
    );

    /// Returns a boxed clone of this view.
    fn clone_box(&self) -> Box<dyn CompilerView>;
}