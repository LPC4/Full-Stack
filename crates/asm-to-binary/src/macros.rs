macro_rules! instruction_mnemonic {
    ($id:ident) => {
        riscv_model::InstructionId::$id.mnemonic()
    };
}

/// Generates an R-type instruction struct with fields `rd`, `rs1`, `rs2`
/// implementing [`Instruction`](super::traits::Instruction).
macro_rules! r_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::encode_decode::Reg,
            pub rs1: crate::encode_decode::Reg,
            pub rs2: crate::encode_decode::Reg,
        }

        impl $name {
            pub fn new(
                rd: crate::encode_decode::Reg,
                rs1: crate::encode_decode::Reg,
                rs2: crate::encode_decode::Reg,
            ) -> Self {
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    rs1 < 32,
                    "{}: register x{rs1} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    rs2 < 32,
                    "{}: register x{rs2} is out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, rs1, rs2 }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let Some(rd) = riscv_model::IntReg::new(self.rd) else {
                    panic!(
                        "{}: register x{} is out of range",
                        instruction_mnemonic!($name),
                        self.rd
                    );
                };
                let Some(rs1) = riscv_model::IntReg::new(self.rs1) else {
                    panic!(
                        "{}: register x{} is out of range",
                        instruction_mnemonic!($name),
                        self.rs1
                    );
                };
                let Some(rs2) = riscv_model::IntReg::new(self.rs2) else {
                    panic!(
                        "{}: register x{} is out of range",
                        instruction_mnemonic!($name),
                        self.rs2
                    );
                };
                match riscv_model::encode_register_register(
                    riscv_model::InstructionId::$name,
                    rd,
                    rs1,
                    rs2,
                ) {
                    Ok(word) => word,
                    Err(error) => panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    ),
                }
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    reg_name(self.rs1, false),
                    reg_name(self.rs2, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates an I-type immediate instruction (opcode 0x13).
/// Operand order in `to_asm`: `rd, rs1, imm`
macro_rules! i_imm_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::encode_decode::Reg,
            pub rs1: crate::encode_decode::Reg,
            pub imm: i32,
        }

        impl $name {
            pub fn new(
                rd: crate::encode_decode::Reg,
                rs1: crate::encode_decode::Reg,
                imm: i32,
            ) -> Self {
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    rs1 < 32,
                    "{}: register x{rs1} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (-2048..=2047).contains(&imm),
                    "{}: immediate {} out of range [-2048, 2047]",
                    instruction_mnemonic!($name),
                    imm
                );
                Self { rd, rs1, imm }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                let rs1 = riscv_model::IntReg::new(self.rs1).expect("validated source register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Immediate {
                        rd,
                        rs1,
                        immediate: self.imm,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    reg_name(self.rs1, false),
                    self.imm,
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates an I-type load instruction (opcode 0x03).
/// Operand order in `to_asm` is `rd, imm(rs1)`, the standard RISC-V load syntax.
macro_rules! i_load_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Destination register.
            pub rd: crate::encode_decode::Reg,
            /// Base address register.
            pub base: crate::encode_decode::Reg,
            /// Byte offset added to `base`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                rd: crate::encode_decode::Reg,
                base: crate::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    base < 32,
                    "{}: register x{base} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (-2048..=2047).contains(&offset),
                    "{}: offset {} out of range [-2048, 2047]",
                    instruction_mnemonic!($name),
                    offset
                );
                Self { rd, base, offset }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                let rs1 = riscv_model::IntReg::new(self.base).expect("validated base register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Immediate {
                        rd,
                        rs1,
                        immediate: self.offset,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}({})",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates an S-type store instruction (opcode 0x23).
/// Operand order in `to_asm` is `rs2, offset(rs1)`, the standard RISC-V store syntax.
macro_rules! s_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            /// Base address register.
            pub base: crate::encode_decode::Reg,
            /// Source data register.
            pub src: crate::encode_decode::Reg,
            /// Byte offset added to `base`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                base: crate::encode_decode::Reg,
                src: crate::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                assert!(
                    base < 32,
                    "{}: register x{base} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    src < 32,
                    "{}: register x{src} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (-2048..=2047).contains(&offset),
                    "{}: offset {} out of range [-2048, 2047]",
                    instruction_mnemonic!($name),
                    offset
                );
                Self { base, src, offset }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let rs1 = riscv_model::IntReg::new(self.base).expect("validated base register");
                let rs2 = riscv_model::IntReg::new(self.src).expect("validated source register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Store {
                        rs1,
                        rs2,
                        immediate: self.offset,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}({})",
                    instruction_mnemonic!($name),
                    reg_name(self.src, false),
                    self.offset,
                    reg_name(self.base, false),
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates a B-type branch instruction (opcode 0x63).
/// Operand order in `to_asm`: `rs1, rs2, offset`
macro_rules! b_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rs1: crate::encode_decode::Reg,
            pub rs2: crate::encode_decode::Reg,
            /// PC-relative byte offset. Must be even, in `[-4096, +4094]`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(
                rs1: crate::encode_decode::Reg,
                rs2: crate::encode_decode::Reg,
                offset: i32,
            ) -> Self {
                assert!(
                    rs1 < 32,
                    "{}: register x{rs1} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    rs2 < 32,
                    "{}: register x{rs2} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (offset & 1) == 0,
                    "{}: offset {} must be even",
                    instruction_mnemonic!($name),
                    offset
                );
                assert!(
                    (-4096..=4094).contains(&offset),
                    "{}: offset {} out of range [-4096, 4094]",
                    instruction_mnemonic!($name),
                    offset
                );
                Self { rs1, rs2, offset }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let rs1 =
                    riscv_model::IntReg::new(self.rs1).expect("validated first source register");
                let rs2 =
                    riscv_model::IntReg::new(self.rs2).expect("validated second source register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Branch {
                        rs1,
                        rs2,
                        offset: self.offset,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rs1, false),
                    reg_name(self.rs2, false),
                    self.offset,
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates a U-type instruction (`lui` or `auipc`).
/// Operand order in `to_asm`: `rd, imm` (imm printed as upper-20-bit hex)
macro_rules! u_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::encode_decode::Reg,
            /// Full 32-bit value whose upper 20 bits are the immediate.
            /// Lower 12 bits should be zero; they are masked during encode.
            pub imm: i32,
        }

        impl $name {
            pub fn new(rd: crate::encode_decode::Reg, imm: i32) -> Self {
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                Self { rd, imm }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let Some(rd) = riscv_model::IntReg::new(self.rd) else {
                    panic!(
                        "{}: register x{} is out of range",
                        instruction_mnemonic!($name),
                        self.rd
                    );
                };
                match riscv_model::encode_upper_immediate(
                    riscv_model::InstructionId::$name,
                    rd,
                    self.imm,
                ) {
                    Ok(word) => word,
                    Err(error) => panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    ),
                }
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {:#x}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    (self.imm as u32) >> 12,
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}

/// Generates a J-type instruction (`jal`).
/// Operand order in `to_asm`: `rd, offset`
macro_rules! j_inst {
    ($name:ident $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub rd: crate::encode_decode::Reg,
            /// PC-relative byte offset. Must be even, in `[-1048576, +1048574]`.
            pub offset: i32,
        }

        impl $name {
            pub fn new(rd: crate::encode_decode::Reg, offset: i32) -> Self {
                assert!(
                    rd < 32,
                    "{}: register x{rd} is out of range",
                    instruction_mnemonic!($name)
                );
                assert!(
                    (offset & 1) == 0,
                    "{}: offset {} must be even",
                    instruction_mnemonic!($name),
                    offset
                );
                assert!(
                    (-1048576..=1048574).contains(&offset),
                    "{}: offset {} out of range [-1048576, 1048574]",
                    instruction_mnemonic!($name),
                    offset
                );
                Self { rd, offset }
            }
        }

        impl crate::traits::Instruction for $name {
            fn encode(&self) -> u32 {
                let rd = riscv_model::IntReg::new(self.rd).expect("validated destination register");
                riscv_model::encode_base_integer(
                    riscv_model::InstructionId::$name,
                    riscv_model::DecodedOperands::Jump {
                        rd,
                        offset: self.offset,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} descriptor error: {error:?}",
                        instruction_mnemonic!($name)
                    )
                })
            }

            fn to_asm(&self) -> String {
                use crate::utils::reg_name;
                format!(
                    "{:<6} {}, {}",
                    instruction_mnemonic!($name),
                    reg_name(self.rd, false),
                    self.offset,
                )
            }

            fn mnemonic(&self) -> &'static str {
                instruction_mnemonic!($name)
            }
        }
    };
}
