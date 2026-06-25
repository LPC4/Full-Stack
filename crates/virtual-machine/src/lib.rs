#![expect(
    clippy::absurd_extreme_comparisons,
    clippy::cast_possible_wrap,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::manual_let_else,
    clippy::manual_ok_err,
    clippy::map_err_ignore,
    clippy::match_same_arms,
    clippy::missing_assert_message,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::needless_range_loop,
    clippy::or_fun_call,
    clippy::print_stderr,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    reason = "legacy VM structure and bit-preserving ISA conversions are intentional"
)]
#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "VM unit tests unwrap controlled fixtures and expected results"
    )
)]

pub mod bus;
pub mod cpu;
pub mod devices;
pub mod elf_parser;
pub mod error;
pub mod memory;
pub mod rom;
pub mod virtual_machine;

pub use virtual_machine::{RunResult, VirtualMachine};
