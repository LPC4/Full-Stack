//! RV64A - Atomics extension.
//!
//! All instructions use opcode `0x2F`.
//! Width is selected by `funct3`: `2` = `.w` (32-bit), `3` = `.d` (64-bit).
//! `aq` (acquire) and `rl` (release) ordering bits can be independently set.

use super::super::traits::Instruction;
use crate::encode_decode::Reg;
use crate::utils::reg_name;

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
    ($name:ident $(,)?) => {
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
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    addr < 32,
                    "{}: register x{addr} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    src < 32,
                    "{}: register x{src} is out of range",
                    instruction_mnemonic!($name)
                );
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
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                let rs1 = riscv_model::IntReg::new(self.addr).expect("validated address register");
                let rs2 = riscv_model::IntReg::new(self.src).expect("validated source register");
                riscv_model::encode_atomic(
                    riscv_model::InstructionId::$name,
                    rd,
                    rs1,
                    rs2,
                    self.ordering.aq,
                    self.ordering.rl,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                format!(
                    "{}{} {}, {}, ({})",
                    instruction_mnemonic!($name),
                    self.ordering.suffix(),
                    reg_name(self.rd, false),
                    reg_name(self.src, false),
                    reg_name(self.addr, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

// --- Word (.w, funct3=2) AMO instructions - funct5 values per spec ---

amo_inst!(AmoaddW);
amo_inst!(AmoswapW);
amo_inst!(AmoxorW);
amo_inst!(AmoandW);
amo_inst!(AmoorW);
amo_inst!(AmominW);
amo_inst!(AmomaxW);
amo_inst!(AmominuW);
amo_inst!(AmomaxuW);

// --- Doubleword (.d, funct3=3) AMO instructions ---

amo_inst!(AmoaddD);
amo_inst!(AmoswapD);
amo_inst!(AmoxorD);
amo_inst!(AmoandD);
amo_inst!(AmoorD);
amo_inst!(AmominD);
amo_inst!(AmomaxD);
amo_inst!(AmominuD);
amo_inst!(AmomaxuD);

// --- Load-Reserved / Store-Conditional  (LR has no rs2; SC has all three) ---

/// Load-reserved: `lr.w rd, (rs1)` / `lr.d rd, (rs1)`.
/// The spec requires `rs2 == x0`, so this struct deliberately has no `src` field.
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
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(LrW)
        );
        assert!(
            addr < 32,
            "{}: register x{addr} is out of range",
            instruction_mnemonic!(LrW)
        );
        Self {
            rd,
            addr,
            ordering: Ordering::none(),
            double: false,
        }
    }
    pub fn d(rd: Reg, addr: Reg) -> Self {
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(LrD)
        );
        assert!(
            addr < 32,
            "{}: register x{addr} is out of range",
            instruction_mnemonic!(LrD)
        );
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

    fn instruction_id(&self) -> riscv_model::InstructionId {
        if self.double {
            riscv_model::InstructionId::LrD
        } else {
            riscv_model::InstructionId::LrW
        }
    }
}

impl Instruction for Lr {
    fn encode(&self) -> u32 {
        let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
        let rs1 = riscv_model::IntReg::new(self.addr).expect("validated address register");
        let rs2 = riscv_model::IntReg::new(0).expect("zero register is valid");
        let id = self.instruction_id();
        riscv_model::encode_atomic(id, rd, rs1, rs2, self.ordering.aq, self.ordering.rl)
            .unwrap_or_else(|error| panic!("lr descriptor error: {error:?}"))
    }

    fn to_asm(&self) -> String {
        format!(
            "{}{} {}, ({})",
            self.instruction_id().mnemonic(),
            self.ordering.suffix(),
            reg_name(self.rd, false),
            reg_name(self.addr, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        self.instruction_id().mnemonic()
    }
}

/// Store-conditional: `sc.w rd, rs2, (rs1)` / `sc.d rd, rs2, (rs1)`.
/// `rd` is 0 on success and 1 on failure; `rd = x0` is valid and discards the status.
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
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(ScW)
        );
        assert!(
            addr < 32,
            "{}: register x{addr} is out of range",
            instruction_mnemonic!(ScW)
        );
        assert!(
            src < 32,
            "{}: register x{src} is out of range",
            instruction_mnemonic!(ScW)
        );
        Self {
            rd,
            addr,
            src,
            ordering: Ordering::none(),
            double: false,
        }
    }
    pub fn d(rd: Reg, addr: Reg, src: Reg) -> Self {
        assert!(
            rd < 32,
            "{}: register x{rd} is out of range",
            instruction_mnemonic!(ScD)
        );
        assert!(
            addr < 32,
            "{}: register x{addr} is out of range",
            instruction_mnemonic!(ScD)
        );
        assert!(
            src < 32,
            "{}: register x{src} is out of range",
            instruction_mnemonic!(ScD)
        );
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

    fn instruction_id(&self) -> riscv_model::InstructionId {
        if self.double {
            riscv_model::InstructionId::ScD
        } else {
            riscv_model::InstructionId::ScW
        }
    }
}

impl Instruction for Sc {
    fn encode(&self) -> u32 {
        let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
        let rs1 = riscv_model::IntReg::new(self.addr).expect("validated address register");
        let rs2 = riscv_model::IntReg::new(self.src).expect("validated source register");
        let id = self.instruction_id();
        riscv_model::encode_atomic(id, rd, rs1, rs2, self.ordering.aq, self.ordering.rl)
            .unwrap_or_else(|error| panic!("sc descriptor error: {error:?}"))
    }

    fn to_asm(&self) -> String {
        format!(
            "{}{} {}, {}, ({})",
            self.instruction_id().mnemonic(),
            self.ordering.suffix(),
            reg_name(self.rd, false),
            reg_name(self.src, false),
            reg_name(self.addr, false),
        )
    }

    fn mnemonic(&self) -> &'static str {
        self.instruction_id().mnemonic()
    }
}
