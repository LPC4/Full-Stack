//! RV64M - Multiply / Divide extension.
//!
//! All instructions are R-type with `funct7 = 0x01`.
//! 64-bit ops use opcode `0x33`; 32-bit word ops use `0x3B`.

// --- 64-bit multiply / divide  (opcode 0x33, funct7 = 0x01) ---

r_inst!(
    Mul,
    opcode = 0x33,
    funct3 = 0,
    funct7 = 0x01,
    mnemonic = "mul"
);
r_inst!(
    Mulh,
    opcode = 0x33,
    funct3 = 1,
    funct7 = 0x01,
    mnemonic = "mulh"
);
r_inst!(
    Mulhsu,
    opcode = 0x33,
    funct3 = 2,
    funct7 = 0x01,
    mnemonic = "mulhsu"
);
r_inst!(
    Mulhu,
    opcode = 0x33,
    funct3 = 3,
    funct7 = 0x01,
    mnemonic = "mulhu"
);
r_inst!(
    Div,
    opcode = 0x33,
    funct3 = 4,
    funct7 = 0x01,
    mnemonic = "div"
);
r_inst!(
    Divu,
    opcode = 0x33,
    funct3 = 5,
    funct7 = 0x01,
    mnemonic = "divu"
);
r_inst!(
    Rem,
    opcode = 0x33,
    funct3 = 6,
    funct7 = 0x01,
    mnemonic = "rem"
);
r_inst!(
    Remu,
    opcode = 0x33,
    funct3 = 7,
    funct7 = 0x01,
    mnemonic = "remu"
);

// --- 32-bit word multiply / divide  (opcode 0x3B, funct7 = 0x01) ---

r_inst!(
    Mulw,
    opcode = 0x3B,
    funct3 = 0,
    funct7 = 0x01,
    mnemonic = "mulw"
);
r_inst!(
    Divw,
    opcode = 0x3B,
    funct3 = 4,
    funct7 = 0x01,
    mnemonic = "divw"
);
r_inst!(
    Divuw,
    opcode = 0x3B,
    funct3 = 5,
    funct7 = 0x01,
    mnemonic = "divuw"
);
r_inst!(
    Remw,
    opcode = 0x3B,
    funct3 = 6,
    funct7 = 0x01,
    mnemonic = "remw"
);
r_inst!(
    Remuw,
    opcode = 0x3B,
    funct3 = 7,
    funct7 = 0x01,
    mnemonic = "remuw"
);
