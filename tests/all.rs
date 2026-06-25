#![expect(
    clippy::cast_possible_wrap,
    clippy::cloned_instead_of_copied,
    clippy::collapsible_if,
    clippy::doc_markdown,
    clippy::duplicate_mod,
    clippy::needless_lifetimes,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::match_wildcard_for_single_variants,
    clippy::needless_pass_by_value,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::str_to_string,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unwrap_used,
    clippy::unused_trait_names,
    reason = "integration harnesses prioritize fixture clarity and diagnostic output"
)]

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
