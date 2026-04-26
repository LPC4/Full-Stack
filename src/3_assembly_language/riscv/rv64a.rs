//! RV64A - Atomics extension.
//!
//! All instructions use opcode `0x2F`.
//! Width is selected by `funct3`: `2` = `.w` (32-bit), `3` = `.d` (64-bit).
//! `aq` (acquire) and `rl` (release) ordering bits can be independently set.

use super::super::traits::Instruction;
use crate::assembly_language::encode_decode::{AtomicType, Reg, RiscvFormat};
use crate::assembly_language::utils::reg_name;

const AMO_OPCODE: u8 = 0x2F;

/// Ordering annotation appended to an AMO mnemonic string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ordering {
    pub aq: bool,
    pub rl: bool,
}

impl Ordering {
    pub fn none() -> Self {
        Self {
            aq: false,
            rl: false,
        }
    }
    pub fn aq() -> Self {
        Self {
            aq: true,
            rl: false,
        }
    }
    pub fn rl() -> Self {
        Self {
            aq: false,
            rl: true,
        }
    }
    pub fn aqrl() -> Self {
        Self { aq: true, rl: true }
    }

    fn suffix(self) -> &'static str {
        match (self.aq, self.rl) {
            (false, false) => "",
            (true, false) => ".aq",
            (false, true) => ".rl",
            (true, true) => ".aqrl",
        }
    }
}

macro_rules! amo_inst {
    (
        $name:ident,
        funct3   = $f3:expr,
        funct5   = $f5:expr,
        mnemonic = $m:literal $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register (receives old memory value).
            pub rd: Reg,
            /// Address register.
            pub addr: Reg,
            /// Source register (value to apply atomically).
            pub src: Reg,
            pub ordering: Ordering,
        }

        impl $name {
            pub fn new(rd: Reg, addr: Reg, src: Reg) -> Self {
                Self {
                    rd,
                    addr,
                    src,
                    ordering: Ordering::none(),
                }
            }
            pub fn with_ordering(mut self, o: Ordering) -> Self {
                self.ordering = o;
                self
            }
        }

        impl Instruction for $name {
            fn encode(&self) -> u32 {
                AtomicType {
                    opcode: AMO_OPCODE,
                    rd: self.rd,
                    funct3: $f3,
                    rs1: self.addr,
                    rs2: self.src,
                    rl: self.ordering.rl,
                    aq: self.ordering.aq,
                    funct5: $f5,
                }
                .encode()
            }

            fn to_asm(&self) -> String {
                format!(
                    "{}{}{} {}, {}, ({})",
                    $m,
                    if $f3 == 2 { ".w" } else { ".d" },
                    self.ordering.suffix(),
                    reg_name(self.rd, false),
                    reg_name(self.src, false),
                    reg_name(self.addr, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                $m
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Word (.w, funct3=2) AMO instructions - funct5 values per spec
// ---------------------------------------------------------------------------

amo_inst!(AmoaddW, funct3 = 2, funct5 = 0x00, mnemonic = "amoadd");
amo_inst!(AmoswapW, funct3 = 2, funct5 = 0x01, mnemonic = "amoswap");
amo_inst!(AmoxorW, funct3 = 2, funct5 = 0x04, mnemonic = "amoxor");
amo_inst!(AmoandW, funct3 = 2, funct5 = 0x0C, mnemonic = "amoand");
amo_inst!(AmoorW, funct3 = 2, funct5 = 0x08, mnemonic = "amoor");
amo_inst!(AmominW, funct3 = 2, funct5 = 0x10, mnemonic = "amomin");
amo_inst!(AmomaxW, funct3 = 2, funct5 = 0x14, mnemonic = "amomax");
amo_inst!(AmominuW, funct3 = 2, funct5 = 0x18, mnemonic = "amominu");
amo_inst!(AmomaxuW, funct3 = 2, funct5 = 0x1C, mnemonic = "amomaxu");

// ---------------------------------------------------------------------------
// Doubleword (.d, funct3=3) AMO instructions
// ---------------------------------------------------------------------------

amo_inst!(AmoaddD, funct3 = 3, funct5 = 0x00, mnemonic = "amoadd");
amo_inst!(AmoswapD, funct3 = 3, funct5 = 0x01, mnemonic = "amoswap");
amo_inst!(AmoxorD, funct3 = 3, funct5 = 0x04, mnemonic = "amoxor");
amo_inst!(AmoandD, funct3 = 3, funct5 = 0x0C, mnemonic = "amoand");
amo_inst!(AmoorD, funct3 = 3, funct5 = 0x08, mnemonic = "amoor");
amo_inst!(AmominD, funct3 = 3, funct5 = 0x10, mnemonic = "amomin");
amo_inst!(AmomaxD, funct3 = 3, funct5 = 0x14, mnemonic = "amomax");
amo_inst!(AmominuD, funct3 = 3, funct5 = 0x18, mnemonic = "amominu");
amo_inst!(AmomaxuD, funct3 = 3, funct5 = 0x1C, mnemonic = "amomaxu");

// ---------------------------------------------------------------------------
// Load-Reserved / Store-Conditional  (LR has no rs2; SC has all three)
// ---------------------------------------------------------------------------

/// `lr.w rd, (rs1)` / `lr.d rd, (rs1)` — Load-reserved.
///
/// `rs2` must be `x0` per spec. This struct has no `src` field to enforce that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lr {
    pub rd: Reg,
    pub addr: Reg,
    pub ordering: Ordering,
    /// `false` = `.w` (32-bit), `true` = `.d` (64-bit).
    pub double: bool,
}

impl Lr {
    pub fn w(rd: Reg, addr: Reg) -> Self {
        Self {
            rd,
            addr,
            ordering: Ordering::none(),
            double: false,
        }
    }
    pub fn d(rd: Reg, addr: Reg) -> Self {
        Self {
            rd,
            addr,
            ordering: Ordering::none(),
            double: true,
        }
    }
    pub fn with_ordering(mut self, o: Ordering) -> Self {
        self.ordering = o;
        self
    }
}

impl Instruction for Lr {
    fn encode(&self) -> u32 {
        AtomicType {
            opcode: AMO_OPCODE,
            rd: self.rd,
            funct3: if self.double { 3 } else { 2 },
            rs1: self.addr,
            rs2: 0, // must be x0
            rl: self.ordering.rl,
            aq: self.ordering.aq,
            funct5: 0x02,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        format!(
            "lr.{}{} {}, ({})",
            if self.double { "d" } else { "w" },
            self.ordering.suffix(),
            reg_name(self.rd, false),
            reg_name(self.addr, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        "lr"
    }
}

/// `sc.w rd, rs2, (rs1)` / `sc.d rd, rs2, (rs1)` — Store-conditional.
///
/// `rd = 0` on success, `rd = 1` on failure. `rd = x0` is valid (status discarded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sc {
    /// Result register: 0 = success, 1 = failure.
    pub rd: Reg,
    pub addr: Reg,
    pub src: Reg,
    pub ordering: Ordering,
    /// `false` = `.w`, `true` = `.d`.
    pub double: bool,
}

impl Sc {
    pub fn w(rd: Reg, addr: Reg, src: Reg) -> Self {
        Self {
            rd,
            addr,
            src,
            ordering: Ordering::none(),
            double: false,
        }
    }
    pub fn d(rd: Reg, addr: Reg, src: Reg) -> Self {
        Self {
            rd,
            addr,
            src,
            ordering: Ordering::none(),
            double: true,
        }
    }
    pub fn with_ordering(mut self, o: Ordering) -> Self {
        self.ordering = o;
        self
    }
}

impl Instruction for Sc {
    fn encode(&self) -> u32 {
        AtomicType {
            opcode: AMO_OPCODE,
            rd: self.rd,
            funct3: if self.double { 3 } else { 2 },
            rs1: self.addr,
            rs2: self.src,
            rl: self.ordering.rl,
            aq: self.ordering.aq,
            funct5: 0x03,
        }
        .encode()
    }

    fn to_asm(&self) -> String {
        format!(
            "sc.{}{} {}, {}, ({})",
            if self.double { "d" } else { "w" },
            self.ordering.suffix(),
            reg_name(self.rd, false),
            reg_name(self.src, false),
            reg_name(self.addr, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        "sc"
    }
}
