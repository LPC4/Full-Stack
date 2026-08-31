//! RV64M - Multiply / Divide extension.
//!
//! All instructions are R-type with `funct7 = 0x01`.
//! 64-bit ops use opcode `0x33`; 32-bit word ops use `0x3B`.

// --- 64-bit multiply / divide  (opcode 0x33, funct7 = 0x01) ---

r_inst!(Mul);
r_inst!(Mulh);
r_inst!(Mulhsu);
r_inst!(Mulhu);
r_inst!(Div);
r_inst!(Divu);
r_inst!(Rem);
r_inst!(Remu);

// --- 32-bit word multiply / divide  (opcode 0x3B, funct7 = 0x01) ---

r_inst!(Mulw);
r_inst!(Divw);
r_inst!(Divuw);
r_inst!(Remw);
r_inst!(Remuw);
