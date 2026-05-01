//! Instruction decode stage.

use crate::virtual_machine::cpu::decoder::{DecodedInsn, decode as decode_word};
use crate::virtual_machine::error::VmError;

/// Decode a raw 32-bit instruction word into a `DecodedInsn`.
pub fn decode(raw: u32) -> Result<DecodedInsn, VmError> {
    decode_word(raw)
}
