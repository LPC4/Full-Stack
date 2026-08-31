//! RV64M - Multiply / Divide extension.
//!
//! All instructions are R-type with `funct7 = 0x01`.
//! 64-bit ops use opcode `0x33`; 32-bit word ops use `0x3B`.

// --- 64-bit multiply / divide  (opcode 0x33, funct7 = 0x01) ---

r_inst!(Mul, mnemonic = "mul");
r_inst!(Mulh, mnemonic = "mulh");
r_inst!(Mulhsu, mnemonic = "mulhsu");
r_inst!(Mulhu, mnemonic = "mulhu");
r_inst!(Div, mnemonic = "div");
r_inst!(Divu, mnemonic = "divu");
r_inst!(Rem, mnemonic = "rem");
r_inst!(Remu, mnemonic = "remu");

// --- 32-bit word multiply / divide  (opcode 0x3B, funct7 = 0x01) ---

r_inst!(Mulw, mnemonic = "mulw");
r_inst!(Divw, mnemonic = "divw");
r_inst!(Divuw, mnemonic = "divuw");
r_inst!(Remw, mnemonic = "remw");
r_inst!(Remuw, mnemonic = "remuw");
