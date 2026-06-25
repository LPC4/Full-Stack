#![expect(
    clippy::cast_possible_wrap,
    clippy::iter_over_hash_type,
    clippy::let_underscore_untyped,
    clippy::map_err_ignore,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_assert_message,
    clippy::missing_errors_doc,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    reason = "legacy assembler structure and bit-preserving ISA conversions are intentional"
)]
#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "assembler unit tests unwrap setup and assertion values"
    )
)]

#[macro_use]
mod macros;

pub mod assembler;
pub mod encode_decode;
pub mod object_linker;
pub mod pseudo;
pub mod real;
pub mod riscv;
pub mod rv_instruction;
pub mod traits;
pub mod utils;

pub use assembler::link_layout::LinkLayout;
pub use assembler::output::{AssembledOutput, RelocationKind, RelocationRecord, SectionInfo};
pub use assembler::reg_parse::parse_int_reg;
pub use assembler::{Assembler, AssemblerError};
pub use object_linker::{LinkerError, ObjectLinker};
pub use rv_instruction::RvInstruction;
