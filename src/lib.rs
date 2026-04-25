#![allow(clippy::all)]
#![warn(rust_2018_idioms)]

mod app;

#[path = "1_high_level_language/mod.rs"]
pub mod high_level_language;

#[path = "2_intermediate_language/mod.rs"]
pub mod intermediate_language;

#[path = "3_assembly_language/mod.rs"]
pub mod assembly_language;

#[path = "4_virtual_machine/mod.rs"]
pub mod virtual_machine;

pub use app::TemplateApp;
