// Integration tests drive the native pipeline and VM; never built for wasm.
#![cfg(not(target_arch = "wasm32"))]
#![expect(
    clippy::cast_possible_wrap,
    clippy::cloned_instead_of_copied,
    clippy::collapsible_if,
    clippy::doc_markdown,
    clippy::duplicate_mod,
    clippy::needless_lifetimes,
    clippy::let_underscore_untyped,
    clippy::needless_pass_by_value,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::str_to_string,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unwrap_used,
    reason = "integration harnesses prioritize fixture clarity and diagnostic output"
)]

#[path = "common/conformance_support.rs"]
mod conformance_support;
#[path = "common/generated_conformance_support.rs"]
mod generated_conformance_support;
#[path = "common/qemu_support.rs"]
mod qemu_support;

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
