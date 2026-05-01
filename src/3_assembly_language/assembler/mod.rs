/// Multi-pass assembler: `Vec<RvInstruction>` → `AssembledOutput`.
///
/// # Passes
///
/// | Pass | File | Responsibility |
/// |------|------|----------------|
/// | 0 — Parse    | `parser.rs`   | `RvInstruction` → `Vec<AsmToken>` (typed, no raw strings) |
/// | 1 — Layout   | `layout.rs`   | Walk tokens, compute every label's section-relative address |
/// | 2 — Encode   | `encode.rs`   | Emit bytes, resolve branch/jump offsets via symbol table |

pub mod directive;
pub mod encode;
pub mod layout;
pub mod output;
pub mod parser;
pub mod reg_parse;
pub mod section;
pub mod symbol_table;
pub mod token;

use crate::assembly_language::rv_instruction::RvInstruction;
use output::AssembledOutput;

/// Error produced by any pass.
#[derive(Debug, Clone)]
pub struct AssemblerError {
    pub message: String,
}

impl std::fmt::Display for AssemblerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "assembler error: {}", self.message)
    }
}

impl AssemblerError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self { message: msg.into() }
    }
}

/// Top-level assembler — runs all three passes in sequence.
pub struct Assembler;

impl Assembler {
    /// Assemble a `RvInstruction` token stream into machine code.
    ///
    /// # Errors
    /// Returns an error if a label is undefined/duplicated, or if a branch
    /// offset falls outside the encodable range.
    pub fn assemble(tokens: &[RvInstruction]) -> Result<AssembledOutput, AssemblerError> {
        // Pass 0: parse raw strings into fully-typed AsmTokens.
        let asm_tokens = parser::parse(tokens);

        // Pass 1: compute label addresses.
        let layout = layout::compute_layout(&asm_tokens)?;

        // Pass 2: encode to bytes, resolving all symbol references.
        encode::encode(&asm_tokens, &layout)
    }
}
