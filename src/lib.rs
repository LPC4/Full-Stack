#![expect(
    clippy::cast_possible_wrap,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::string_add,
    clippy::too_many_lines,
    clippy::unnecessary_debug_formatting,
    clippy::unused_self,
    clippy::use_self,
    reason = "legacy pipeline and UI structure remain covered while lint cleanup proceeds incrementally"
)]
#![cfg_attr(
    test,
    expect(
        clippy::manual_let_else,
        reason = "legacy pipeline test helpers remain while lint cleanup proceeds incrementally"
    )
)]

pub mod compilation_pipeline;
pub mod target_mode;
pub mod view;
