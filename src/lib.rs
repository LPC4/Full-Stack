#![expect(
    clippy::missing_errors_doc,
    clippy::string_add,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::use_self,
    reason = "compiler pipeline structure remains covered while lint cleanup proceeds incrementally"
)]
#![cfg_attr(
    test,
    expect(
        clippy::cast_possible_wrap,
        clippy::let_underscore_must_use,
        clippy::let_underscore_untyped,
        clippy::manual_let_else,
        reason = "pipeline test helpers use wrapping casts, ignore cleanup results, and bind via let/else"
    )
)]

pub mod build;
pub mod compilation_pipeline;
pub mod highlight;
pub mod inspect;
pub mod program_catalog;
pub mod runconfig;
pub mod session;
pub mod target_mode;
pub mod userspace;
pub mod workspace;
