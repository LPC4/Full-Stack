#![expect(
    clippy::cast_possible_wrap,
    clippy::collapsible_match,
    clippy::doc_markdown,
    clippy::iter_over_hash_type,
    clippy::match_same_arms,
    clippy::needless_pass_by_ref_mut,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::unwrap_used,
    reason = "legacy code generator structure and target-width conversions are intentional"
)]

pub mod compiler;

pub use compiler::compiler_rv64::CompilerRv64;
