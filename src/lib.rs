#![warn(clippy::all, rust_2018_idioms)]

mod app;

#[path = "1_high_level_language/mod.rs"]
pub mod high_level_language;

#[path = "2_intermediate_language/mod.rs"]
pub mod intermediate_language;

#[path = "3_assembly_language/mod.rs"]
pub mod assembly_language;

pub use app::TemplateApp;
