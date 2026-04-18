#![warn(clippy::all, rust_2018_idioms)]

mod app;

#[path = "1_high_level_language/mod.rs"]
pub mod high_level_language;

pub use app::TemplateApp;
