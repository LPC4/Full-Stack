//! Inverse of `utils::reg_name`: map an ABI register name back to its number (0-31).

#[cfg(test)]
pub use crate::isa::parse_float_register as parse_float_reg;
pub use crate::isa::parse_integer_register as parse_int_reg;
