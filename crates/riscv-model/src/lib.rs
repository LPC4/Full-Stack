//! Shared, dependency-light definitions for the supported RISC-V instruction set.

pub const INTEGER_REGISTER_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

pub const FLOAT_REGISTER_NAMES: [&str; 32] = [
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2",
    "fa3", "fa4", "fa5", "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9",
    "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IntReg(u8);

impl IntReg {
    pub const fn new(number: u8) -> Option<Self> {
        if number < 32 {
            Some(Self(number))
        } else {
            None
        }
    }

    pub const fn number(self) -> u8 {
        self.0
    }

    pub fn abi_name(self) -> &'static str {
        INTEGER_REGISTER_NAMES[usize::from(self.0)]
    }

    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        if name == "fp" {
            return Some(Self(8));
        }
        let number = parse_numbered_register(name, 'x').or_else(|| {
            INTEGER_REGISTER_NAMES
                .iter()
                .position(|candidate| *candidate == name)
        })?;
        Some(Self(number as u8))
    }
}

impl From<IntReg> for u8 {
    fn from(register: IntReg) -> Self {
        register.number()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FpReg(u8);

impl FpReg {
    pub const fn new(number: u8) -> Option<Self> {
        if number < 32 {
            Some(Self(number))
        } else {
            None
        }
    }

    pub const fn number(self) -> u8 {
        self.0
    }

    pub fn abi_name(self) -> &'static str {
        FLOAT_REGISTER_NAMES[usize::from(self.0)]
    }

    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        let number = parse_numbered_register(name, 'f').or_else(|| {
            FLOAT_REGISTER_NAMES
                .iter()
                .position(|candidate| *candidate == name)
        })?;
        Some(Self(number as u8))
    }
}

impl From<FpReg> for u8 {
    fn from(register: FpReg) -> Self {
        register.number()
    }
}

pub fn integer_register_name(register: usize) -> Option<&'static str> {
    INTEGER_REGISTER_NAMES.get(register).copied()
}

pub fn float_register_name(register: usize) -> Option<&'static str> {
    FLOAT_REGISTER_NAMES.get(register).copied()
}

pub fn is_register_name(name: &str) -> bool {
    IntReg::parse(name).is_some() || FpReg::parse(name).is_some()
}

fn parse_numbered_register(name: &str, prefix: char) -> Option<usize> {
    let number = name.strip_prefix(prefix)?.parse::<usize>().ok()?;
    (number < 32).then_some(number)
}

pub fn pcrel_split(offset: i64) -> (i32, i32) {
    let low = ((offset & 0xfff) as i32).wrapping_sub(if offset & 0x800 == 0 { 0 } else { 0x1000 });
    let high = ((offset - i64::from(low)) >> 12) as i32;
    (high, low)
}

/// The architectural register file used by an instruction operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterFile {
    Integer,
    FloatingPoint,
}

/// Explicit register operands used by an instruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegisterUse {
    pub source1: Option<RegisterFile>,
    pub source2: Option<RegisterFile>,
    pub source3: Option<RegisterFile>,
    pub destination: Option<RegisterFile>,
}

impl RegisterUse {
    const NONE: Self = Self {
        source1: None,
        source2: None,
        source3: None,
        destination: None,
    };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Extension {
    Rv64I,
    M,
    A,
    F,
    D,
    Zicsr,
    Zifencei,
    Privileged,
}

impl Extension {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rv64I => "RV64I",
            Self::M => "M",
            Self::A => "A",
            Self::F => "F",
            Self::D => "D",
            Self::Zicsr => "Zicsr",
            Self::Zifencei => "Zifencei",
            Self::Privileged => "privileged",
        }
    }
}

impl std::fmt::Display for Extension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Extensions enabled for an instruction-selection target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IsaProfile(u16);

impl IsaProfile {
    /// The base RV64I instruction set without optional extensions.
    pub const RV64I: Self = Self(extension_bit(Extension::Rv64I));

    /// Every extension implemented by this workspace.
    pub const FULL_STACK: Self = Self(
        extension_bit(Extension::Rv64I)
            | extension_bit(Extension::M)
            | extension_bit(Extension::A)
            | extension_bit(Extension::F)
            | extension_bit(Extension::D)
            | extension_bit(Extension::Zicsr)
            | extension_bit(Extension::Zifencei)
            | extension_bit(Extension::Privileged),
    );

    pub const fn with_extension(self, extension: Extension) -> Self {
        Self(self.0 | extension_bit(extension))
    }

    pub const fn supports_extension(self, extension: Extension) -> bool {
        self.0 & extension_bit(extension) != 0
    }

    pub const fn supports_instruction(self, instruction: InstructionId) -> bool {
        self.supports_extension(instruction.extension())
    }
}

const fn extension_bit(extension: Extension) -> u16 {
    match extension {
        Extension::Rv64I => 1 << 0,
        Extension::M => 1 << 1,
        Extension::A => 1 << 2,
        Extension::F => 1 << 3,
        Extension::D => 1 << 4,
        Extension::Zicsr => 1 << 5,
        Extension::Zifencei => 1 << 6,
        Extension::Privileged => 1 << 7,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegeRequirement {
    Any,
    Supervisor,
    Machine,
    EncodedByOperand,
    RuntimeControlled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportStatus {
    Implemented,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstructionParity {
    pub id: InstructionId,
    pub extension: Extension,
    pub privilege: PrivilegeRequirement,
    pub assembler: SupportStatus,
    pub decoder: SupportStatus,
    pub pipeline: SupportStatus,
    pub metadata: SupportStatus,
    pub operand_validation: SupportStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodingVector {
    pub id: InstructionId,
    pub word: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionFormat {
    UpperImmediate,
    RegisterRegister,
    RegisterRegisterWord,
    Immediate,
    ImmediateWord,
    ShiftImmediate,
    ShiftImmediateWord,
    Load,
    Store,
    Branch,
    Jump,
    JumpRegister,
    Fence,
    FenceI,
    System,
    CsrRegister,
    CsrImmediate,
    Atomic,
    FloatingLoad,
    FloatingStore,
    FloatingOp,
    FloatingFused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandKind {
    IntegerRegister,
    Immediate,
    Csr,
    FenceSet,
    FloatingRegister,
    RoundingMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImmediateKind {
    Upper20,
    Signed12,
    Shift5,
    Shift6,
    Branch13,
    Jump21,
    Csr12,
    Fence12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyClass {
    Integer,
    Multiply,
    Divide,
    Atomic,
    FloatingPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryAccessClass {
    None,
    Load,
    Store,
    Atomic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlFlowClass {
    Sequential,
    Branch,
    DirectJump,
    IndirectJump,
    Environment,
    TrapReturn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstructionSpec {
    pub id: InstructionId,
    pub mask: u32,
    pub match_bits: u32,
    pub format: InstructionFormat,
    pub operands: &'static [OperandKind],
    pub immediate: Option<ImmediateKind>,
    pub extension: Extension,
    pub privilege: PrivilegeRequirement,
    pub register_use: RegisterUse,
    pub latency: LatencyClass,
    pub memory_access: MemoryAccessClass,
    pub control_flow: ControlFlowClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodedOperands {
    UpperImmediate {
        rd: IntReg,
        immediate: i32,
    },
    RegisterRegister {
        rd: IntReg,
        rs1: IntReg,
        rs2: IntReg,
    },
    Immediate {
        rd: IntReg,
        rs1: IntReg,
        immediate: i32,
    },
    Store {
        rs1: IntReg,
        rs2: IntReg,
        immediate: i32,
    },
    Branch {
        rs1: IntReg,
        rs2: IntReg,
        offset: i32,
    },
    Jump {
        rd: IntReg,
        offset: i32,
    },
    Fence {
        fm: u8,
        predecessor: u8,
        successor: u8,
    },
    Csr {
        rd: IntReg,
        source: u8,
        csr: u16,
    },
    Atomic {
        rd: IntReg,
        rs1: IntReg,
        rs2: IntReg,
        acquire: bool,
        release: bool,
    },
    Floating {
        rd: u8,
        rs1: u8,
        rs2: u8,
        rs3: u8,
        rounding_mode: u8,
        immediate: i32,
    },
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedInstruction {
    pub id: InstructionId,
    pub operands: DecodedOperands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodingError {
    UnsupportedInstruction(InstructionId),
    ImmediateOutOfRange {
        id: InstructionId,
        value: i32,
        minimum: i32,
        maximum: i32,
    },
    MisalignedImmediate {
        id: InstructionId,
        value: i32,
        alignment: u8,
    },
    FieldOutOfRange {
        id: InstructionId,
        field: &'static str,
        value: u32,
        maximum: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    UnsupportedEncoding(u32),
}

macro_rules! define_instruction_ids {
    ($($id:ident),+ $(,)?) => {
        /// Canonical identity of a real instruction supported by the current VM.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum InstructionId {
            $($id),+
        }

        impl InstructionId {
            pub const ALL: &'static [Self] = &[$(Self::$id),+];
        }
    };
}

define_instruction_ids! {
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Ld,
    Lbu,
    Lhu,
    Lwu,
    Sb,
    Sh,
    Sw,
    Sd,
    Addi,
    Slli,
    Slti,
    Sltiu,
    Xori,
    Srli,
    Srai,
    Ori,
    Andi,
    Addiw,
    Slliw,
    Srliw,
    Sraiw,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
    Addw,
    Subw,
    Sllw,
    Srlw,
    Sraw,
    Mulw,
    Divw,
    Divuw,
    Remw,
    Remuw,
    Fence,
    FenceI,
    Ecall,
    Ebreak,
    Mret,
    Sret,
    SfenceVma,
    Wfi,
    Csrrw,
    Csrrs,
    Csrrc,
    Csrrwi,
    Csrrsi,
    Csrrci,
    Flw,
    Fld,
    Fsw,
    Fsd,
    FaddS,
    FaddD,
    FsubS,
    FsubD,
    FmulS,
    FmulD,
    FdivS,
    FdivD,
    FsqrtS,
    FsqrtD,
    FsgnjS,
    FsgnjD,
    FsgnjnS,
    FsgnjnD,
    FsgnjxS,
    FsgnjxD,
    FminS,
    FminD,
    FmaxS,
    FmaxD,
    FleS,
    FleD,
    FltS,
    FltD,
    FeqS,
    FeqD,
    FcvtWS,
    FcvtWuS,
    FcvtLS,
    FcvtLuS,
    FcvtWD,
    FcvtWuD,
    FcvtLD,
    FcvtLuD,
    FcvtSW,
    FcvtSWu,
    FcvtSL,
    FcvtSLu,
    FcvtDW,
    FcvtDWu,
    FcvtDL,
    FcvtDLu,
    FcvtSD,
    FcvtDS,
    FmvXW,
    FmvXD,
    FclassS,
    FclassD,
    FmvWX,
    FmvDX,
    FmaddS,
    FmaddD,
    FmsubS,
    FmsubD,
    FnmsubS,
    FnmsubD,
    FnmaddS,
    FnmaddD,
    LrW,
    LrD,
    ScW,
    ScD,
    AmoswapW,
    AmoswapD,
    AmoaddW,
    AmoaddD,
    AmoxorW,
    AmoxorD,
    AmoorW,
    AmoorD,
    AmoandW,
    AmoandD,
    AmominW,
    AmominD,
    AmomaxW,
    AmomaxD,
    AmominuW,
    AmominuD,
    AmomaxuW,
    AmomaxuD,
}

const UPPER_IMMEDIATE_OPERANDS: &[OperandKind] =
    &[OperandKind::IntegerRegister, OperandKind::Immediate];

macro_rules! define_upper_immediate_family {
    ($($id:ident => $opcode:literal),+ $(,)?) => {
        pub const UPPER_IMMEDIATE_SPECS: &[InstructionSpec] = &[
            $(InstructionSpec {
                id: InstructionId::$id,
                mask: 0x7f,
                match_bits: $opcode,
                format: InstructionFormat::UpperImmediate,
                operands: UPPER_IMMEDIATE_OPERANDS,
                immediate: Some(ImmediateKind::Upper20),
                extension: Extension::Rv64I,
                privilege: PrivilegeRequirement::Any,
                register_use: RegisterUse {
                    source1: None,
                    source2: None,
                    source3: None,
                    destination: Some(RegisterFile::Integer),
                },
                latency: LatencyClass::Integer,
                memory_access: MemoryAccessClass::None,
                control_flow: ControlFlowClass::Sequential,
            }),+
        ];
    };
}

define_upper_immediate_family! {
    Lui => 0x37,
    Auipc => 0x17,
}

const REGISTER_REGISTER_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::IntegerRegister,
    OperandKind::IntegerRegister,
];

macro_rules! define_register_register_family {
    ($($id:ident => ($format:ident, $extension:ident, $opcode:literal, $funct3:literal, $funct7:literal, $latency:ident)),+ $(,)?) => {
        pub const REGISTER_REGISTER_SPECS: &[InstructionSpec] = &[
            $(InstructionSpec {
                id: InstructionId::$id,
                mask: 0xfe00_707f,
                match_bits: ($funct7 << 25) | ($funct3 << 12) | $opcode,
                format: InstructionFormat::$format,
                operands: REGISTER_REGISTER_OPERANDS,
                immediate: None,
                extension: Extension::$extension,
                privilege: PrivilegeRequirement::Any,
                register_use: RegisterUse {
                    source1: Some(RegisterFile::Integer),
                    source2: Some(RegisterFile::Integer),
                    source3: None,
                    destination: Some(RegisterFile::Integer),
                },
                latency: LatencyClass::$latency,
                memory_access: MemoryAccessClass::None,
                control_flow: ControlFlowClass::Sequential,
            }),+
        ];
    };
}

define_register_register_family! {
    Add => (RegisterRegister, Rv64I, 0x33, 0, 0x00, Integer),
    Sub => (RegisterRegister, Rv64I, 0x33, 0, 0x20, Integer),
    Sll => (RegisterRegister, Rv64I, 0x33, 1, 0x00, Integer),
    Slt => (RegisterRegister, Rv64I, 0x33, 2, 0x00, Integer),
    Sltu => (RegisterRegister, Rv64I, 0x33, 3, 0x00, Integer),
    Xor => (RegisterRegister, Rv64I, 0x33, 4, 0x00, Integer),
    Srl => (RegisterRegister, Rv64I, 0x33, 5, 0x00, Integer),
    Sra => (RegisterRegister, Rv64I, 0x33, 5, 0x20, Integer),
    Or => (RegisterRegister, Rv64I, 0x33, 6, 0x00, Integer),
    And => (RegisterRegister, Rv64I, 0x33, 7, 0x00, Integer),
    Addw => (RegisterRegisterWord, Rv64I, 0x3b, 0, 0x00, Integer),
    Subw => (RegisterRegisterWord, Rv64I, 0x3b, 0, 0x20, Integer),
    Sllw => (RegisterRegisterWord, Rv64I, 0x3b, 1, 0x00, Integer),
    Srlw => (RegisterRegisterWord, Rv64I, 0x3b, 5, 0x00, Integer),
    Sraw => (RegisterRegisterWord, Rv64I, 0x3b, 5, 0x20, Integer),
    Mul => (RegisterRegister, M, 0x33, 0, 0x01, Multiply),
    Mulh => (RegisterRegister, M, 0x33, 1, 0x01, Multiply),
    Mulhsu => (RegisterRegister, M, 0x33, 2, 0x01, Multiply),
    Mulhu => (RegisterRegister, M, 0x33, 3, 0x01, Multiply),
    Div => (RegisterRegister, M, 0x33, 4, 0x01, Divide),
    Divu => (RegisterRegister, M, 0x33, 5, 0x01, Divide),
    Rem => (RegisterRegister, M, 0x33, 6, 0x01, Divide),
    Remu => (RegisterRegister, M, 0x33, 7, 0x01, Divide),
    Mulw => (RegisterRegisterWord, M, 0x3b, 0, 0x01, Multiply),
    Divw => (RegisterRegisterWord, M, 0x3b, 4, 0x01, Divide),
    Divuw => (RegisterRegisterWord, M, 0x3b, 5, 0x01, Divide),
    Remw => (RegisterRegisterWord, M, 0x3b, 6, 0x01, Divide),
    Remuw => (RegisterRegisterWord, M, 0x3b, 7, 0x01, Divide),
}

const IMMEDIATE_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::IntegerRegister,
    OperandKind::Immediate,
];
const STORE_BRANCH_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::IntegerRegister,
    OperandKind::Immediate,
];
const JUMP_OPERANDS: &[OperandKind] = &[OperandKind::IntegerRegister, OperandKind::Immediate];
const FENCE_OPERANDS: &[OperandKind] = &[OperandKind::FenceSet, OperandKind::FenceSet];
const CSR_REGISTER_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::Csr,
    OperandKind::IntegerRegister,
];
const CSR_IMMEDIATE_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::Csr,
    OperandKind::Immediate,
];
const NO_OPERANDS: &[OperandKind] = &[];

macro_rules! define_base_integer_family {
    ($($id:ident => ($format:ident, $mask:literal, $match_bits:literal, $operands:ident, $immediate:expr, $memory:ident, $control:ident)),+ $(,)?) => {
        pub const BASE_INTEGER_SPECS: &[InstructionSpec] = &[
            $(InstructionSpec {
                id: InstructionId::$id,
                mask: $mask,
                match_bits: $match_bits,
                format: InstructionFormat::$format,
                operands: $operands,
                immediate: $immediate,
                extension: InstructionId::$id.extension(),
                privilege: InstructionId::$id.privilege_requirement(),
                register_use: InstructionId::$id.register_use(),
                latency: LatencyClass::Integer,
                memory_access: MemoryAccessClass::$memory,
                control_flow: ControlFlowClass::$control,
            }),+
        ];
    };
}

define_base_integer_family! {
    Jal => (Jump, 0x0000_007f, 0x0000_006f, JUMP_OPERANDS, Some(ImmediateKind::Jump21), None, DirectJump),
    Jalr => (JumpRegister, 0x0000_707f, 0x0000_0067, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, IndirectJump),
    Beq => (Branch, 0x0000_707f, 0x0000_0063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Bne => (Branch, 0x0000_707f, 0x0000_1063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Blt => (Branch, 0x0000_707f, 0x0000_4063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Bge => (Branch, 0x0000_707f, 0x0000_5063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Bltu => (Branch, 0x0000_707f, 0x0000_6063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Bgeu => (Branch, 0x0000_707f, 0x0000_7063, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Branch13), None, Branch),
    Lb => (Load, 0x0000_707f, 0x0000_0003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Lh => (Load, 0x0000_707f, 0x0000_1003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Lw => (Load, 0x0000_707f, 0x0000_2003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Ld => (Load, 0x0000_707f, 0x0000_3003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Lbu => (Load, 0x0000_707f, 0x0000_4003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Lhu => (Load, 0x0000_707f, 0x0000_5003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Lwu => (Load, 0x0000_707f, 0x0000_6003, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), Load, Sequential),
    Sb => (Store, 0x0000_707f, 0x0000_0023, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Signed12), Store, Sequential),
    Sh => (Store, 0x0000_707f, 0x0000_1023, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Signed12), Store, Sequential),
    Sw => (Store, 0x0000_707f, 0x0000_2023, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Signed12), Store, Sequential),
    Sd => (Store, 0x0000_707f, 0x0000_3023, STORE_BRANCH_OPERANDS, Some(ImmediateKind::Signed12), Store, Sequential),
    Addi => (Immediate, 0x0000_707f, 0x0000_0013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Slti => (Immediate, 0x0000_707f, 0x0000_2013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Sltiu => (Immediate, 0x0000_707f, 0x0000_3013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Xori => (Immediate, 0x0000_707f, 0x0000_4013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Ori => (Immediate, 0x0000_707f, 0x0000_6013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Andi => (Immediate, 0x0000_707f, 0x0000_7013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Slli => (ShiftImmediate, 0xfc00_707f, 0x0000_1013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift6), None, Sequential),
    Srli => (ShiftImmediate, 0xfc00_707f, 0x0000_5013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift6), None, Sequential),
    Srai => (ShiftImmediate, 0xfc00_707f, 0x4000_5013, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift6), None, Sequential),
    Addiw => (ImmediateWord, 0x0000_707f, 0x0000_001b, IMMEDIATE_OPERANDS, Some(ImmediateKind::Signed12), None, Sequential),
    Slliw => (ShiftImmediateWord, 0xfe00_707f, 0x0000_101b, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift5), None, Sequential),
    Srliw => (ShiftImmediateWord, 0xfe00_707f, 0x0000_501b, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift5), None, Sequential),
    Sraiw => (ShiftImmediateWord, 0xfe00_707f, 0x4000_501b, IMMEDIATE_OPERANDS, Some(ImmediateKind::Shift5), None, Sequential),
    Fence => (Fence, 0x000f_ffff, 0x0000_000f, FENCE_OPERANDS, Some(ImmediateKind::Fence12), None, Sequential),
    FenceI => (FenceI, 0xffff_ffff, 0x0000_100f, NO_OPERANDS, None, None, Sequential),
    Ecall => (System, 0xffff_ffff, 0x0000_0073, NO_OPERANDS, None, None, Environment),
    Ebreak => (System, 0xffff_ffff, 0x0010_0073, NO_OPERANDS, None, None, Environment),
    Mret => (System, 0xffff_ffff, 0x3020_0073, NO_OPERANDS, None, None, TrapReturn),
    Sret => (System, 0xffff_ffff, 0x1020_0073, NO_OPERANDS, None, None, TrapReturn),
    SfenceVma => (System, 0xffff_ffff, 0x1200_0073, NO_OPERANDS, None, None, Sequential),
    Wfi => (System, 0xffff_ffff, 0x1050_0073, NO_OPERANDS, None, None, Sequential),
    Csrrw => (CsrRegister, 0x0000_707f, 0x0000_1073, CSR_REGISTER_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
    Csrrs => (CsrRegister, 0x0000_707f, 0x0000_2073, CSR_REGISTER_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
    Csrrc => (CsrRegister, 0x0000_707f, 0x0000_3073, CSR_REGISTER_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
    Csrrwi => (CsrImmediate, 0x0000_707f, 0x0000_5073, CSR_IMMEDIATE_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
    Csrrsi => (CsrImmediate, 0x0000_707f, 0x0000_6073, CSR_IMMEDIATE_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
    Csrrci => (CsrImmediate, 0x0000_707f, 0x0000_7073, CSR_IMMEDIATE_OPERANDS, Some(ImmediateKind::Csr12), None, Sequential),
}

macro_rules! define_atomic_family {
    ($($id:ident => ($mask:literal, $match_bits:literal)),+ $(,)?) => {
        pub const ATOMIC_SPECS: &[InstructionSpec] = &[
            $(InstructionSpec {
                id: InstructionId::$id,
                mask: $mask,
                match_bits: $match_bits,
                format: InstructionFormat::Atomic,
                operands: REGISTER_REGISTER_OPERANDS,
                immediate: None,
                extension: Extension::A,
                privilege: PrivilegeRequirement::Any,
                register_use: InstructionId::$id.register_use(),
                latency: LatencyClass::Atomic,
                memory_access: MemoryAccessClass::Atomic,
                control_flow: ControlFlowClass::Sequential,
            }),+
        ];
    };
}

define_atomic_family! {
    LrW => (0xf9f0_707f, 0x1000_202f),
    LrD => (0xf9f0_707f, 0x1000_302f),
    ScW => (0xf800_707f, 0x1800_202f),
    ScD => (0xf800_707f, 0x1800_302f),
    AmoaddW => (0xf800_707f, 0x0000_202f),
    AmoaddD => (0xf800_707f, 0x0000_302f),
    AmoswapW => (0xf800_707f, 0x0800_202f),
    AmoswapD => (0xf800_707f, 0x0800_302f),
    AmoxorW => (0xf800_707f, 0x2000_202f),
    AmoxorD => (0xf800_707f, 0x2000_302f),
    AmoorW => (0xf800_707f, 0x4000_202f),
    AmoorD => (0xf800_707f, 0x4000_302f),
    AmoandW => (0xf800_707f, 0x6000_202f),
    AmoandD => (0xf800_707f, 0x6000_302f),
    AmominW => (0xf800_707f, 0x8000_202f),
    AmominD => (0xf800_707f, 0x8000_302f),
    AmomaxW => (0xf800_707f, 0xa000_202f),
    AmomaxD => (0xf800_707f, 0xa000_302f),
    AmominuW => (0xf800_707f, 0xc000_202f),
    AmominuD => (0xf800_707f, 0xc000_302f),
    AmomaxuW => (0xf800_707f, 0xe000_202f),
    AmomaxuD => (0xf800_707f, 0xe000_302f),
}

const FLOAT_LOAD_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::IntegerRegister,
    OperandKind::Immediate,
];
const FLOAT_STORE_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::IntegerRegister,
    OperandKind::Immediate,
];
const FLOAT_BINARY_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::RoundingMode,
];
const FLOAT_UNARY_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::RoundingMode,
];
const FLOAT_TO_INT_OPERANDS: &[OperandKind] = &[
    OperandKind::IntegerRegister,
    OperandKind::FloatingRegister,
    OperandKind::RoundingMode,
];
const INT_TO_FLOAT_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::IntegerRegister,
    OperandKind::RoundingMode,
];
const FLOAT_FUSED_OPERANDS: &[OperandKind] = &[
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::FloatingRegister,
    OperandKind::RoundingMode,
];

macro_rules! define_floating_family {
    ($($id:ident => ($format:ident, $mask:literal, $match_bits:literal, $operands:ident, $immediate:expr, $memory:ident)),+ $(,)?) => {
        pub const FLOATING_SPECS: &[InstructionSpec] = &[
            $(InstructionSpec {
                id: InstructionId::$id,
                mask: $mask,
                match_bits: $match_bits,
                format: InstructionFormat::$format,
                operands: $operands,
                immediate: $immediate,
                extension: InstructionId::$id.extension(),
                privilege: PrivilegeRequirement::Any,
                register_use: InstructionId::$id.register_use(),
                latency: LatencyClass::FloatingPoint,
                memory_access: MemoryAccessClass::$memory,
                control_flow: ControlFlowClass::Sequential,
            }),+
        ];
    };
}

define_floating_family! {
    Flw => (FloatingLoad, 0x0000_707f, 0x0000_2007, FLOAT_LOAD_OPERANDS, Some(ImmediateKind::Signed12), Load),
    Fld => (FloatingLoad, 0x0000_707f, 0x0000_3007, FLOAT_LOAD_OPERANDS, Some(ImmediateKind::Signed12), Load),
    Fsw => (FloatingStore, 0x0000_707f, 0x0000_2027, FLOAT_STORE_OPERANDS, Some(ImmediateKind::Signed12), Store),
    Fsd => (FloatingStore, 0x0000_707f, 0x0000_3027, FLOAT_STORE_OPERANDS, Some(ImmediateKind::Signed12), Store),
    FaddS => (FloatingOp, 0xfe00_007f, 0x0000_0053, FLOAT_BINARY_OPERANDS, None, None),
    FaddD => (FloatingOp, 0xfe00_007f, 0x0200_0053, FLOAT_BINARY_OPERANDS, None, None),
    FsubS => (FloatingOp, 0xfe00_007f, 0x0800_0053, FLOAT_BINARY_OPERANDS, None, None),
    FsubD => (FloatingOp, 0xfe00_007f, 0x0a00_0053, FLOAT_BINARY_OPERANDS, None, None),
    FmulS => (FloatingOp, 0xfe00_007f, 0x1000_0053, FLOAT_BINARY_OPERANDS, None, None),
    FmulD => (FloatingOp, 0xfe00_007f, 0x1200_0053, FLOAT_BINARY_OPERANDS, None, None),
    FdivS => (FloatingOp, 0xfe00_007f, 0x1800_0053, FLOAT_BINARY_OPERANDS, None, None),
    FdivD => (FloatingOp, 0xfe00_007f, 0x1a00_0053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjS => (FloatingOp, 0xfe00_707f, 0x2000_0053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjD => (FloatingOp, 0xfe00_707f, 0x2200_0053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjnS => (FloatingOp, 0xfe00_707f, 0x2000_1053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjnD => (FloatingOp, 0xfe00_707f, 0x2200_1053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjxS => (FloatingOp, 0xfe00_707f, 0x2000_2053, FLOAT_BINARY_OPERANDS, None, None),
    FsgnjxD => (FloatingOp, 0xfe00_707f, 0x2200_2053, FLOAT_BINARY_OPERANDS, None, None),
    FminS => (FloatingOp, 0xfe00_707f, 0x2800_0053, FLOAT_BINARY_OPERANDS, None, None),
    FminD => (FloatingOp, 0xfe00_707f, 0x2a00_0053, FLOAT_BINARY_OPERANDS, None, None),
    FmaxS => (FloatingOp, 0xfe00_707f, 0x2800_1053, FLOAT_BINARY_OPERANDS, None, None),
    FmaxD => (FloatingOp, 0xfe00_707f, 0x2a00_1053, FLOAT_BINARY_OPERANDS, None, None),
    FsqrtS => (FloatingOp, 0xfff0_007f, 0x5800_0053, FLOAT_UNARY_OPERANDS, None, None),
    FsqrtD => (FloatingOp, 0xfff0_007f, 0x5a00_0053, FLOAT_UNARY_OPERANDS, None, None),
    FleS => (FloatingOp, 0xfe00_707f, 0xa000_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FleD => (FloatingOp, 0xfe00_707f, 0xa200_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FltS => (FloatingOp, 0xfe00_707f, 0xa000_1053, FLOAT_TO_INT_OPERANDS, None, None),
    FltD => (FloatingOp, 0xfe00_707f, 0xa200_1053, FLOAT_TO_INT_OPERANDS, None, None),
    FeqS => (FloatingOp, 0xfe00_707f, 0xa000_2053, FLOAT_TO_INT_OPERANDS, None, None),
    FeqD => (FloatingOp, 0xfe00_707f, 0xa200_2053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtWS => (FloatingOp, 0xfff0_007f, 0xc000_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtWuS => (FloatingOp, 0xfff0_007f, 0xc010_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtLS => (FloatingOp, 0xfff0_007f, 0xc020_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtLuS => (FloatingOp, 0xfff0_007f, 0xc030_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtWD => (FloatingOp, 0xfff0_007f, 0xc200_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtWuD => (FloatingOp, 0xfff0_007f, 0xc210_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtLD => (FloatingOp, 0xfff0_007f, 0xc220_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtLuD => (FloatingOp, 0xfff0_007f, 0xc230_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FcvtSW => (FloatingOp, 0xfff0_007f, 0xd000_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtSWu => (FloatingOp, 0xfff0_007f, 0xd010_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtSL => (FloatingOp, 0xfff0_007f, 0xd020_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtSLu => (FloatingOp, 0xfff0_007f, 0xd030_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtDW => (FloatingOp, 0xfff0_007f, 0xd200_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtDWu => (FloatingOp, 0xfff0_007f, 0xd210_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtDL => (FloatingOp, 0xfff0_007f, 0xd220_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtDLu => (FloatingOp, 0xfff0_007f, 0xd230_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FcvtSD => (FloatingOp, 0xfff0_007f, 0x4010_0053, FLOAT_UNARY_OPERANDS, None, None),
    FcvtDS => (FloatingOp, 0xfff0_007f, 0x4a00_0053, FLOAT_UNARY_OPERANDS, None, None),
    FmvXW => (FloatingOp, 0xfff0_707f, 0xe000_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FmvXD => (FloatingOp, 0xfff0_707f, 0xe200_0053, FLOAT_TO_INT_OPERANDS, None, None),
    FclassS => (FloatingOp, 0xfff0_707f, 0xe000_1053, FLOAT_TO_INT_OPERANDS, None, None),
    FclassD => (FloatingOp, 0xfff0_707f, 0xe200_1053, FLOAT_TO_INT_OPERANDS, None, None),
    FmvWX => (FloatingOp, 0xfff0_707f, 0xf000_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FmvDX => (FloatingOp, 0xfff0_707f, 0xf200_0053, INT_TO_FLOAT_OPERANDS, None, None),
    FmaddS => (FloatingFused, 0x0600_007f, 0x0000_0043, FLOAT_FUSED_OPERANDS, None, None),
    FmaddD => (FloatingFused, 0x0600_007f, 0x0200_0043, FLOAT_FUSED_OPERANDS, None, None),
    FmsubS => (FloatingFused, 0x0600_007f, 0x0000_0047, FLOAT_FUSED_OPERANDS, None, None),
    FmsubD => (FloatingFused, 0x0600_007f, 0x0200_0047, FLOAT_FUSED_OPERANDS, None, None),
    FnmsubS => (FloatingFused, 0x0600_007f, 0x0000_004b, FLOAT_FUSED_OPERANDS, None, None),
    FnmsubD => (FloatingFused, 0x0600_007f, 0x0200_004b, FLOAT_FUSED_OPERANDS, None, None),
    FnmaddS => (FloatingFused, 0x0600_007f, 0x0000_004f, FLOAT_FUSED_OPERANDS, None, None),
    FnmaddD => (FloatingFused, 0x0600_007f, 0x0200_004f, FLOAT_FUSED_OPERANDS, None, None),
}

pub fn instruction_spec(id: InstructionId) -> Option<&'static InstructionSpec> {
    instruction_specs().find(|spec| spec.id == id)
}

pub fn instruction_specs() -> impl Iterator<Item = &'static InstructionSpec> {
    UPPER_IMMEDIATE_SPECS
        .iter()
        .chain(REGISTER_REGISTER_SPECS)
        .chain(BASE_INTEGER_SPECS)
        .chain(ATOMIC_SPECS)
        .chain(FLOATING_SPECS)
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_instruction(word: u32) -> Result<DecodedInstruction, DecodeError> {
    decode_upper_immediate(word)
        .or_else(|_| decode_register_register(word))
        .or_else(|_| decode_base_integer(word))
        .or_else(|_| decode_atomic(word))
        .or_else(|_| decode_floating(word))
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant identifies an instruction outside this family"
)]
pub fn encode_register_register(
    id: InstructionId,
    rd: IntReg,
    rs1: IntReg,
    rs2: IntReg,
) -> Result<u32, EncodingError> {
    let Some(spec) = REGISTER_REGISTER_SPECS.iter().find(|spec| spec.id == id) else {
        return Err(EncodingError::UnsupportedInstruction(id));
    };
    Ok(spec.match_bits
        | (u32::from(rs2.number()) << 20)
        | (u32::from(rs1.number()) << 15)
        | (u32::from(rd.number()) << 7))
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_register_register(word: u32) -> Result<DecodedInstruction, DecodeError> {
    let Some(spec) = REGISTER_REGISTER_SPECS
        .iter()
        .find(|spec| word & spec.mask == spec.match_bits)
    else {
        return Err(DecodeError::UnsupportedEncoding(word));
    };
    Ok(DecodedInstruction {
        id: spec.id,
        operands: DecodedOperands::RegisterRegister {
            rd: IntReg(((word >> 7) & 0x1f) as u8),
            rs1: IntReg(((word >> 15) & 0x1f) as u8),
            rs2: IntReg(((word >> 20) & 0x1f) as u8),
        },
    })
}

pub fn register_register_instruction(
    format: InstructionFormat,
    funct3: u8,
    funct7: u8,
) -> Option<InstructionId> {
    REGISTER_REGISTER_SPECS
        .iter()
        .find(|spec| {
            spec.format == format
                && ((spec.match_bits >> 12) & 0x7) == u32::from(funct3)
                && ((spec.match_bits >> 25) & 0x7f) == u32::from(funct7)
        })
        .map(|spec| spec.id)
}

fn validate_immediate(
    id: InstructionId,
    value: i32,
    minimum: i32,
    maximum: i32,
) -> Result<(), EncodingError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(EncodingError::ImmediateOutOfRange {
            id,
            value,
            minimum,
            maximum,
        })
    }
}

#[expect(
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    reason = "error variants identify invalid operands or an instruction outside this family"
)]
pub fn encode_base_integer(
    id: InstructionId,
    operands: DecodedOperands,
) -> Result<u32, EncodingError> {
    let Some(spec) = BASE_INTEGER_SPECS.iter().find(|spec| spec.id == id) else {
        return Err(EncodingError::UnsupportedInstruction(id));
    };
    match (spec.format, operands) {
        (
            InstructionFormat::Immediate
            | InstructionFormat::ImmediateWord
            | InstructionFormat::Load
            | InstructionFormat::JumpRegister,
            DecodedOperands::Immediate { rd, rs1, immediate },
        ) => {
            validate_immediate(id, immediate, -2048, 2047)?;
            Ok(spec.match_bits
                | ((immediate.cast_unsigned() & 0xfff) << 20)
                | (u32::from(rs1.number()) << 15)
                | (u32::from(rd.number()) << 7))
        }
        (
            InstructionFormat::ShiftImmediate | InstructionFormat::ShiftImmediateWord,
            DecodedOperands::Immediate { rd, rs1, immediate },
        ) => {
            let maximum = if spec.format == InstructionFormat::ShiftImmediate {
                63
            } else {
                31
            };
            validate_immediate(id, immediate, 0, maximum)?;
            Ok(spec.match_bits
                | (immediate.cast_unsigned() << 20)
                | (u32::from(rs1.number()) << 15)
                | (u32::from(rd.number()) << 7))
        }
        (
            InstructionFormat::Store,
            DecodedOperands::Store {
                rs1,
                rs2,
                immediate,
            },
        ) => {
            validate_immediate(id, immediate, -2048, 2047)?;
            let immediate = immediate.cast_unsigned() & 0xfff;
            Ok(spec.match_bits
                | ((immediate >> 5) << 25)
                | (u32::from(rs2.number()) << 20)
                | (u32::from(rs1.number()) << 15)
                | ((immediate & 0x1f) << 7))
        }
        (InstructionFormat::Branch, DecodedOperands::Branch { rs1, rs2, offset }) => {
            validate_immediate(id, offset, -4096, 4094)?;
            if offset & 1 != 0 {
                return Err(EncodingError::MisalignedImmediate {
                    id,
                    value: offset,
                    alignment: 2,
                });
            }
            let immediate = offset.cast_unsigned();
            Ok(spec.match_bits
                | ((immediate & 0x1000) << 19)
                | ((immediate & 0x07e0) << 20)
                | (u32::from(rs2.number()) << 20)
                | (u32::from(rs1.number()) << 15)
                | ((immediate & 0x001e) << 7)
                | ((immediate & 0x0800) >> 4))
        }
        (InstructionFormat::Jump, DecodedOperands::Jump { rd, offset }) => {
            validate_immediate(id, offset, -1_048_576, 1_048_574)?;
            if offset & 1 != 0 {
                return Err(EncodingError::MisalignedImmediate {
                    id,
                    value: offset,
                    alignment: 2,
                });
            }
            let immediate = offset.cast_unsigned();
            Ok(spec.match_bits
                | ((immediate & 0x10_0000) << 11)
                | ((immediate & 0x0000_07fe) << 20)
                | ((immediate & 0x0000_0800) << 9)
                | (immediate & 0x0f_f000)
                | (u32::from(rd.number()) << 7))
        }
        (
            InstructionFormat::Fence,
            DecodedOperands::Fence {
                fm,
                predecessor,
                successor,
            },
        ) => {
            for (field, value) in [
                ("fm", fm),
                ("predecessor", predecessor),
                ("successor", successor),
            ] {
                if value > 0xf {
                    return Err(EncodingError::FieldOutOfRange {
                        id,
                        field,
                        value: u32::from(value),
                        maximum: 0xf,
                    });
                }
            }
            Ok(spec.match_bits
                | (u32::from(fm) << 28)
                | (u32::from(predecessor) << 24)
                | (u32::from(successor) << 20))
        }
        (
            InstructionFormat::CsrRegister | InstructionFormat::CsrImmediate,
            DecodedOperands::Csr { rd, source, csr },
        ) => {
            if csr > 0xfff {
                return Err(EncodingError::FieldOutOfRange {
                    id,
                    field: "csr",
                    value: u32::from(csr),
                    maximum: 0xfff,
                });
            }
            if source > 31 {
                return Err(EncodingError::FieldOutOfRange {
                    id,
                    field: "source",
                    value: u32::from(source),
                    maximum: 31,
                });
            }
            Ok(spec.match_bits
                | (u32::from(csr) << 20)
                | (u32::from(source) << 15)
                | (u32::from(rd.number()) << 7))
        }
        (InstructionFormat::FenceI | InstructionFormat::System, DecodedOperands::None) => {
            Ok(spec.match_bits)
        }
        _ => Err(EncodingError::UnsupportedInstruction(id)),
    }
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_base_integer(word: u32) -> Result<DecodedInstruction, DecodeError> {
    let Some(spec) = BASE_INTEGER_SPECS
        .iter()
        .find(|spec| word & spec.mask == spec.match_bits)
    else {
        return Err(DecodeError::UnsupportedEncoding(word));
    };
    let rd = IntReg(((word >> 7) & 0x1f) as u8);
    let rs1 = IntReg(((word >> 15) & 0x1f) as u8);
    let rs2 = IntReg(((word >> 20) & 0x1f) as u8);
    let operands = match spec.format {
        InstructionFormat::Immediate
        | InstructionFormat::ImmediateWord
        | InstructionFormat::Load
        | InstructionFormat::JumpRegister => DecodedOperands::Immediate {
            rd,
            rs1,
            immediate: sign_extend(word >> 20, 12),
        },
        InstructionFormat::ShiftImmediate => DecodedOperands::Immediate {
            rd,
            rs1,
            immediate: ((word >> 20) & 0x3f).cast_signed(),
        },
        InstructionFormat::ShiftImmediateWord => DecodedOperands::Immediate {
            rd,
            rs1,
            immediate: ((word >> 20) & 0x1f).cast_signed(),
        },
        InstructionFormat::Store => {
            let immediate = ((word >> 25) << 5) | ((word >> 7) & 0x1f);
            DecodedOperands::Store {
                rs1,
                rs2,
                immediate: sign_extend(immediate, 12),
            }
        }
        InstructionFormat::Branch => {
            let immediate = ((word >> 31) << 12)
                | (((word >> 7) & 1) << 11)
                | (((word >> 25) & 0x3f) << 5)
                | (((word >> 8) & 0xf) << 1);
            DecodedOperands::Branch {
                rs1,
                rs2,
                offset: sign_extend(immediate, 13),
            }
        }
        InstructionFormat::Jump => {
            let immediate = ((word >> 31) << 20)
                | (((word >> 12) & 0xff) << 12)
                | (((word >> 20) & 1) << 11)
                | (((word >> 21) & 0x3ff) << 1);
            DecodedOperands::Jump {
                rd,
                offset: sign_extend(immediate, 21),
            }
        }
        InstructionFormat::Fence => DecodedOperands::Fence {
            fm: ((word >> 28) & 0xf) as u8,
            predecessor: ((word >> 24) & 0xf) as u8,
            successor: ((word >> 20) & 0xf) as u8,
        },
        InstructionFormat::CsrRegister | InstructionFormat::CsrImmediate => DecodedOperands::Csr {
            rd,
            source: rs1.number(),
            csr: ((word >> 20) & 0xfff) as u16,
        },
        InstructionFormat::FenceI | InstructionFormat::System => DecodedOperands::None,
        InstructionFormat::UpperImmediate
        | InstructionFormat::RegisterRegister
        | InstructionFormat::RegisterRegisterWord
        | InstructionFormat::Atomic
        | InstructionFormat::FloatingLoad
        | InstructionFormat::FloatingStore
        | InstructionFormat::FloatingOp
        | InstructionFormat::FloatingFused => {
            return Err(DecodeError::UnsupportedEncoding(word));
        }
    };
    Ok(DecodedInstruction {
        id: spec.id,
        operands,
    })
}

#[expect(
    clippy::missing_errors_doc,
    reason = "error variants identify invalid operands or an instruction outside this family"
)]
pub fn encode_atomic(
    id: InstructionId,
    rd: IntReg,
    rs1: IntReg,
    rs2: IntReg,
    acquire: bool,
    release: bool,
) -> Result<u32, EncodingError> {
    let Some(spec) = ATOMIC_SPECS.iter().find(|spec| spec.id == id) else {
        return Err(EncodingError::UnsupportedInstruction(id));
    };
    if matches!(id, InstructionId::LrW | InstructionId::LrD) && rs2.number() != 0 {
        return Err(EncodingError::FieldOutOfRange {
            id,
            field: "rs2",
            value: u32::from(rs2.number()),
            maximum: 0,
        });
    }
    Ok(spec.match_bits
        | (u32::from(acquire) << 26)
        | (u32::from(release) << 25)
        | (u32::from(rs2.number()) << 20)
        | (u32::from(rs1.number()) << 15)
        | (u32::from(rd.number()) << 7))
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_atomic(word: u32) -> Result<DecodedInstruction, DecodeError> {
    let Some(spec) = ATOMIC_SPECS
        .iter()
        .find(|spec| word & spec.mask == spec.match_bits)
    else {
        return Err(DecodeError::UnsupportedEncoding(word));
    };
    Ok(DecodedInstruction {
        id: spec.id,
        operands: DecodedOperands::Atomic {
            rd: IntReg(((word >> 7) & 0x1f) as u8),
            rs1: IntReg(((word >> 15) & 0x1f) as u8),
            rs2: IntReg(((word >> 20) & 0x1f) as u8),
            acquire: word & (1 << 26) != 0,
            release: word & (1 << 25) != 0,
        },
    })
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the raw fields mirror the largest supported floating-point encoding"
)]
pub fn encode_floating(
    id: InstructionId,
    rd: u8,
    rs1: u8,
    rs2: u8,
    rs3: u8,
    rounding_mode: u8,
    immediate: i32,
) -> Result<u32, EncodingError> {
    let Some(spec) = FLOATING_SPECS.iter().find(|spec| spec.id == id) else {
        return Err(EncodingError::UnsupportedInstruction(id));
    };
    for (field, value) in [("rd", rd), ("rs1", rs1), ("rs2", rs2), ("rs3", rs3)] {
        if value > 31 {
            return Err(EncodingError::FieldOutOfRange {
                id,
                field,
                value: u32::from(value),
                maximum: 31,
            });
        }
    }
    match spec.format {
        InstructionFormat::FloatingLoad => {
            validate_immediate(id, immediate, -2048, 2047)?;
            Ok(spec.match_bits
                | ((immediate.cast_unsigned() & 0xfff) << 20)
                | (u32::from(rs1) << 15)
                | (u32::from(rd) << 7))
        }
        InstructionFormat::FloatingStore => {
            validate_immediate(id, immediate, -2048, 2047)?;
            let immediate = immediate.cast_unsigned() & 0xfff;
            Ok(spec.match_bits
                | ((immediate >> 5) << 25)
                | (u32::from(rs2) << 20)
                | (u32::from(rs1) << 15)
                | ((immediate & 0x1f) << 7))
        }
        InstructionFormat::FloatingOp | InstructionFormat::FloatingFused => {
            let fixed_rounding_mode = spec.mask & 0x7000 != 0;
            if fixed_rounding_mode {
                let expected = ((spec.match_bits >> 12) & 0x7) as u8;
                if rounding_mode != expected {
                    return Err(EncodingError::FieldOutOfRange {
                        id,
                        field: "rounding_mode",
                        value: u32::from(rounding_mode),
                        maximum: u32::from(expected),
                    });
                }
            } else if !(rounding_mode <= 4 || rounding_mode == 7) {
                return Err(EncodingError::FieldOutOfRange {
                    id,
                    field: "rounding_mode",
                    value: u32::from(rounding_mode),
                    maximum: 7,
                });
            }
            let mut word = spec.match_bits | (u32::from(rs1) << 15) | (u32::from(rd) << 7);
            if !fixed_rounding_mode {
                word |= u32::from(rounding_mode) << 12;
            }
            if spec.mask & 0x01f0_0000 == 0 {
                word |= u32::from(rs2) << 20;
            }
            if spec.format == InstructionFormat::FloatingFused {
                word |= u32::from(rs3) << 27;
            }
            Ok(word)
        }
        _ => Err(EncodingError::UnsupportedInstruction(id)),
    }
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_floating(word: u32) -> Result<DecodedInstruction, DecodeError> {
    let Some(spec) = FLOATING_SPECS
        .iter()
        .find(|spec| word & spec.mask == spec.match_bits)
    else {
        return Err(DecodeError::UnsupportedEncoding(word));
    };
    let rounding_mode = ((word >> 12) & 0x7) as u8;
    if spec.mask & 0x7000 == 0 && !(rounding_mode <= 4 || rounding_mode == 7) {
        return Err(DecodeError::UnsupportedEncoding(word));
    }
    let immediate = match spec.format {
        InstructionFormat::FloatingLoad => sign_extend(word >> 20, 12),
        InstructionFormat::FloatingStore => {
            sign_extend(((word >> 25) << 5) | ((word >> 7) & 0x1f), 12)
        }
        _ => 0,
    };
    Ok(DecodedInstruction {
        id: spec.id,
        operands: DecodedOperands::Floating {
            rd: ((word >> 7) & 0x1f) as u8,
            rs1: ((word >> 15) & 0x1f) as u8,
            rs2: ((word >> 20) & 0x1f) as u8,
            rs3: ((word >> 27) & 0x1f) as u8,
            rounding_mode,
            immediate,
        },
    })
}

pub fn floating_default_rounding_mode(id: InstructionId) -> Option<u8> {
    let spec = FLOATING_SPECS.iter().find(|spec| spec.id == id)?;
    Some(if spec.mask & 0x7000 == 0 {
        0
    } else {
        ((spec.match_bits >> 12) & 0x7) as u8
    })
}

fn sign_extend(value: u32, bits: u8) -> i32 {
    let shift = 32 - u32::from(bits);
    (value << shift).cast_signed() >> shift
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant identifies an instruction outside this family"
)]
pub fn encode_upper_immediate(
    id: InstructionId,
    rd: IntReg,
    immediate: i32,
) -> Result<u32, EncodingError> {
    let Some(spec) = UPPER_IMMEDIATE_SPECS.iter().find(|spec| spec.id == id) else {
        return Err(EncodingError::UnsupportedInstruction(id));
    };
    Ok((immediate.cast_unsigned() & 0xffff_f000) | (u32::from(rd.number()) << 7) | spec.match_bits)
}

#[expect(
    clippy::missing_errors_doc,
    reason = "the sole error variant preserves the unsupported instruction word"
)]
pub fn decode_upper_immediate(word: u32) -> Result<DecodedInstruction, DecodeError> {
    let Some(spec) = UPPER_IMMEDIATE_SPECS
        .iter()
        .find(|spec| word & spec.mask == spec.match_bits)
    else {
        return Err(DecodeError::UnsupportedEncoding(word));
    };
    let rd = IntReg(((word >> 7) & 0x1f) as u8);
    Ok(DecodedInstruction {
        id: spec.id,
        operands: DecodedOperands::UpperImmediate {
            rd,
            immediate: (word & 0xffff_f000).cast_signed(),
        },
    })
}

pub const ENCODING_VECTOR_SOURCE: &str =
    "riscv/riscv-opcodes@7afd3dc8772909d8c94ceeb208467cff93896396";

pub const RETAINED_ENCODING_VECTORS: &[EncodingVector] = &[
    EncodingVector {
        id: InstructionId::Lui,
        word: 0x1234_50b7,
    },
    EncodingVector {
        id: InstructionId::Addi,
        word: 0xfff1_0093,
    },
    EncodingVector {
        id: InstructionId::Add,
        word: 0x0031_00b3,
    },
    EncodingVector {
        id: InstructionId::Sub,
        word: 0x4031_00b3,
    },
    EncodingVector {
        id: InstructionId::Mul,
        word: 0x0231_00b3,
    },
    EncodingVector {
        id: InstructionId::Ecall,
        word: 0x0000_0073,
    },
];

impl InstructionId {
    pub const fn extension(self) -> Extension {
        use Extension::{A, D, F, M, Privileged, Rv64I, Zicsr, Zifencei};
        use InstructionId::{
            Add, Addi, Addiw, Addw, AmoaddD, AmoaddW, AmoandD, AmoandW, AmomaxD, AmomaxW, AmomaxuD,
            AmomaxuW, AmominD, AmominW, AmominuD, AmominuW, AmoorD, AmoorW, AmoswapD, AmoswapW,
            AmoxorD, AmoxorW, And, Andi, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne, Csrrc, Csrrci,
            Csrrs, Csrrsi, Csrrw, Csrrwi, Div, Divu, Divuw, Divw, Ebreak, Ecall, FaddD, FaddS,
            FclassD, FclassS, FcvtDL, FcvtDLu, FcvtDS, FcvtDW, FcvtDWu, FcvtLD, FcvtLS, FcvtLuD,
            FcvtLuS, FcvtSD, FcvtSL, FcvtSLu, FcvtSW, FcvtSWu, FcvtWD, FcvtWS, FcvtWuD, FcvtWuS,
            FdivD, FdivS, Fence, FenceI, FeqD, FeqS, Fld, FleD, FleS, FltD, FltS, Flw, FmaddD,
            FmaddS, FmaxD, FmaxS, FminD, FminS, FmsubD, FmsubS, FmulD, FmulS, FmvDX, FmvWX, FmvXD,
            FmvXW, FnmaddD, FnmaddS, FnmsubD, FnmsubS, Fsd, FsgnjD, FsgnjS, FsgnjnD, FsgnjnS,
            FsgnjxD, FsgnjxS, FsqrtD, FsqrtS, FsubD, FsubS, Fsw, Jal, Jalr, Lb, Lbu, Ld, Lh, Lhu,
            LrD, LrW, Lui, Lw, Lwu, Mret, Mul, Mulh, Mulhsu, Mulhu, Mulw, Or, Ori, Rem, Remu,
            Remuw, Remw, Sb, ScD, ScW, Sd, SfenceVma, Sh, Sll, Slli, Slliw, Sllw, Slt, Slti, Sltiu,
            Sltu, Sra, Srai, Sraiw, Sraw, Sret, Srl, Srli, Srliw, Srlw, Sub, Subw, Sw, Wfi, Xor,
            Xori,
        };

        match self {
            Lui | Auipc | Jal | Jalr | Beq | Bne | Blt | Bge | Bltu | Bgeu | Lb | Lh | Lw | Ld
            | Lbu | Lhu | Lwu | Sb | Sh | Sw | Sd | Addi | Slli | Slti | Sltiu | Xori | Srli
            | Srai | Ori | Andi | Addiw | Slliw | Srliw | Sraiw | Add | Sub | Sll | Slt | Sltu
            | Xor | Srl | Sra | Or | And | Addw | Subw | Sllw | Srlw | Sraw | Fence | Ecall
            | Ebreak => Rv64I,
            Mul | Mulh | Mulhsu | Mulhu | Div | Divu | Rem | Remu | Mulw | Divw | Divuw | Remw
            | Remuw => M,
            LrW | LrD | ScW | ScD | AmoswapW | AmoswapD | AmoaddW | AmoaddD | AmoxorW | AmoxorD
            | AmoorW | AmoorD | AmoandW | AmoandD | AmominW | AmominD | AmomaxW | AmomaxD
            | AmominuW | AmominuD | AmomaxuW | AmomaxuD => A,
            Flw | Fsw | FaddS | FsubS | FmulS | FdivS | FsqrtS | FsgnjS | FsgnjnS | FsgnjxS
            | FminS | FmaxS | FleS | FltS | FeqS | FcvtWS | FcvtWuS | FcvtLS | FcvtLuS | FcvtSW
            | FcvtSWu | FcvtSL | FcvtSLu | FmvXW | FclassS | FmvWX | FmaddS | FmsubS | FnmsubS
            | FnmaddS => F,
            Fld | Fsd | FaddD | FsubD | FmulD | FdivD | FsqrtD | FsgnjD | FsgnjnD | FsgnjxD
            | FminD | FmaxD | FleD | FltD | FeqD | FcvtWD | FcvtWuD | FcvtLD | FcvtLuD | FcvtDW
            | FcvtDWu | FcvtDL | FcvtDLu | FcvtSD | FcvtDS | FmvXD | FclassD | FmvDX | FmaddD
            | FmsubD | FnmsubD | FnmaddD => D,
            Csrrw | Csrrs | Csrrc | Csrrwi | Csrrsi | Csrrci => Zicsr,
            FenceI => Zifencei,
            Mret | Sret | SfenceVma | Wfi => Privileged,
        }
    }

    pub const fn privilege_requirement(self) -> PrivilegeRequirement {
        match self {
            Self::Mret => PrivilegeRequirement::Machine,
            Self::Sret | Self::SfenceVma => PrivilegeRequirement::Supervisor,
            Self::Csrrw
            | Self::Csrrs
            | Self::Csrrc
            | Self::Csrrwi
            | Self::Csrrsi
            | Self::Csrrci => PrivilegeRequirement::EncodedByOperand,
            Self::Wfi => PrivilegeRequirement::RuntimeControlled,
            _ => PrivilegeRequirement::Any,
        }
    }

    pub const fn is_trap_return(self) -> bool {
        matches!(self, Self::Mret | Self::Sret)
    }

    pub const fn parity(self) -> InstructionParity {
        let assembler = match self {
            Self::SfenceVma => SupportStatus::Partial,
            _ => SupportStatus::Implemented,
        };
        let decoder = assembler;
        let pipeline = match self {
            Self::Mret | Self::SfenceVma | Self::Wfi => SupportStatus::Partial,
            _ => SupportStatus::Implemented,
        };
        let operand_validation = if matches!(self, Self::SfenceVma) {
            SupportStatus::Partial
        } else {
            SupportStatus::Implemented
        };
        InstructionParity {
            id: self,
            extension: self.extension(),
            privilege: self.privilege_requirement(),
            assembler,
            decoder,
            pipeline,
            metadata: SupportStatus::Implemented,
            operand_validation,
        }
    }

    /// Returns the canonical assembly mnemonic.
    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive identity table is easier to audit in one place"
    )]
    pub const fn mnemonic(self) -> &'static str {
        use InstructionId::{
            Add as AddId, Addi, Addiw, Addw, AmoaddD, AmoaddW, AmoandD, AmoandW, AmomaxD, AmomaxW,
            AmomaxuD, AmomaxuW, AmominD, AmominW, AmominuD, AmominuW, AmoorD, AmoorW, AmoswapD,
            AmoswapW, AmoxorD, AmoxorW, And as AndId, Andi, Auipc, Beq, Bge, Bgeu, Blt, Bltu, Bne,
            Csrrc, Csrrci, Csrrs, Csrrsi, Csrrw, Csrrwi, Div, Divu, Divuw, Divw, Ebreak, Ecall,
            FaddD, FaddS, FclassD, FclassS, FcvtDL, FcvtDLu, FcvtDS, FcvtDW, FcvtDWu, FcvtLD,
            FcvtLS, FcvtLuD, FcvtLuS, FcvtSD, FcvtSL, FcvtSLu, FcvtSW, FcvtSWu, FcvtWD, FcvtWS,
            FcvtWuD, FcvtWuS, FdivD, FdivS, Fence, FenceI, FeqD, FeqS, Fld, FleD, FleS, FltD, FltS,
            Flw, FmaddD, FmaddS, FmaxD, FmaxS, FminD, FminS, FmsubD, FmsubS, FmulD, FmulS, FmvDX,
            FmvWX, FmvXD, FmvXW, FnmaddD, FnmaddS, FnmsubD, FnmsubS, Fsd, FsgnjD, FsgnjS, FsgnjnD,
            FsgnjnS, FsgnjxD, FsgnjxS, FsqrtD, FsqrtS, FsubD, FsubS, Fsw, Jal, Jalr, Lb, Lbu, Ld,
            Lh, Lhu, LrD, LrW, Lui, Lw, Lwu, Mret, Mul, Mulh, Mulhsu, Mulhu, Mulw, Or as OrId, Ori,
            Rem, Remu, Remuw, Remw, Sb, ScD, ScW, Sd, SfenceVma, Sh, Sll, Slli, Slliw, Sllw, Slt,
            Slti, Sltiu, Sltu, Sra, Srai, Sraiw, Sraw, Sret, Srl, Srli, Srliw, Srlw, Sub, Subw, Sw,
            Wfi, Xor as XorId, Xori,
        };

        match self {
            Lui => "lui",
            Auipc => "auipc",
            Jal => "jal",
            Jalr => "jalr",
            Beq => "beq",
            Bne => "bne",
            Blt => "blt",
            Bge => "bge",
            Bltu => "bltu",
            Bgeu => "bgeu",
            Lb => "lb",
            Lh => "lh",
            Lw => "lw",
            Ld => "ld",
            Lbu => "lbu",
            Lhu => "lhu",
            Lwu => "lwu",
            Sb => "sb",
            Sh => "sh",
            Sw => "sw",
            Sd => "sd",
            Addi => "addi",
            Slli => "slli",
            Slti => "slti",
            Sltiu => "sltiu",
            Xori => "xori",
            Srli => "srli",
            Srai => "srai",
            Ori => "ori",
            Andi => "andi",
            Addiw => "addiw",
            Slliw => "slliw",
            Srliw => "srliw",
            Sraiw => "sraiw",
            AddId => "add",
            Sub => "sub",
            Sll => "sll",
            Slt => "slt",
            Sltu => "sltu",
            XorId => "xor",
            Srl => "srl",
            Sra => "sra",
            OrId => "or",
            AndId => "and",
            Mul => "mul",
            Mulh => "mulh",
            Mulhsu => "mulhsu",
            Mulhu => "mulhu",
            Div => "div",
            Divu => "divu",
            Rem => "rem",
            Remu => "remu",
            Addw => "addw",
            Subw => "subw",
            Sllw => "sllw",
            Srlw => "srlw",
            Sraw => "sraw",
            Mulw => "mulw",
            Divw => "divw",
            Divuw => "divuw",
            Remw => "remw",
            Remuw => "remuw",
            Fence => "fence",
            FenceI => "fence.i",
            Ecall => "ecall",
            Ebreak => "ebreak",
            Mret => "mret",
            Sret => "sret",
            SfenceVma => "sfence.vma",
            Wfi => "wfi",
            Csrrw => "csrrw",
            Csrrs => "csrrs",
            Csrrc => "csrrc",
            Csrrwi => "csrrwi",
            Csrrsi => "csrrsi",
            Csrrci => "csrrci",
            Flw => "flw",
            Fld => "fld",
            Fsw => "fsw",
            Fsd => "fsd",
            FaddS => "fadd.s",
            FaddD => "fadd.d",
            FsubS => "fsub.s",
            FsubD => "fsub.d",
            FmulS => "fmul.s",
            FmulD => "fmul.d",
            FdivS => "fdiv.s",
            FdivD => "fdiv.d",
            FsqrtS => "fsqrt.s",
            FsqrtD => "fsqrt.d",
            FsgnjS => "fsgnj.s",
            FsgnjD => "fsgnj.d",
            FsgnjnS => "fsgnjn.s",
            FsgnjnD => "fsgnjn.d",
            FsgnjxS => "fsgnjx.s",
            FsgnjxD => "fsgnjx.d",
            FminS => "fmin.s",
            FminD => "fmin.d",
            FmaxS => "fmax.s",
            FmaxD => "fmax.d",
            FleS => "fle.s",
            FleD => "fle.d",
            FltS => "flt.s",
            FltD => "flt.d",
            FeqS => "feq.s",
            FeqD => "feq.d",
            FcvtWS => "fcvt.w.s",
            FcvtWuS => "fcvt.wu.s",
            FcvtLS => "fcvt.l.s",
            FcvtLuS => "fcvt.lu.s",
            FcvtWD => "fcvt.w.d",
            FcvtWuD => "fcvt.wu.d",
            FcvtLD => "fcvt.l.d",
            FcvtLuD => "fcvt.lu.d",
            FcvtSW => "fcvt.s.w",
            FcvtSWu => "fcvt.s.wu",
            FcvtSL => "fcvt.s.l",
            FcvtSLu => "fcvt.s.lu",
            FcvtDW => "fcvt.d.w",
            FcvtDWu => "fcvt.d.wu",
            FcvtDL => "fcvt.d.l",
            FcvtDLu => "fcvt.d.lu",
            FcvtSD => "fcvt.s.d",
            FcvtDS => "fcvt.d.s",
            FmvXW => "fmv.x.w",
            FmvXD => "fmv.x.d",
            FclassS => "fclass.s",
            FclassD => "fclass.d",
            FmvWX => "fmv.w.x",
            FmvDX => "fmv.d.x",
            FmaddS => "fmadd.s",
            FmaddD => "fmadd.d",
            FmsubS => "fmsub.s",
            FmsubD => "fmsub.d",
            FnmsubS => "fnmsub.s",
            FnmsubD => "fnmsub.d",
            FnmaddS => "fnmadd.s",
            FnmaddD => "fnmadd.d",
            LrW => "lr.w",
            LrD => "lr.d",
            ScW => "sc.w",
            ScD => "sc.d",
            AmoswapW => "amoswap.w",
            AmoswapD => "amoswap.d",
            AmoaddW => "amoadd.w",
            AmoaddD => "amoadd.d",
            AmoxorW => "amoxor.w",
            AmoxorD => "amoxor.d",
            AmoorW => "amoor.w",
            AmoorD => "amoor.d",
            AmoandW => "amoand.w",
            AmoandD => "amoand.d",
            AmominW => "amomin.w",
            AmominD => "amomin.d",
            AmomaxW => "amomax.w",
            AmomaxD => "amomax.d",
            AmominuW => "amominu.w",
            AmominuD => "amominu.d",
            AmomaxuW => "amomaxu.w",
            AmomaxuD => "amomaxu.d",
        }
    }

    /// Returns explicit register-file use for hazard and forwarding logic.
    #[cfg_attr(
        not(test),
        expect(
            clippy::enum_glob_use,
            reason = "the exhaustive identity table reads as an instruction vocabulary"
        )
    )]
    pub const fn register_use(self) -> RegisterUse {
        use InstructionId::*;
        use RegisterFile::{FloatingPoint as Fp, Integer as Int};

        let int_rd = RegisterUse {
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let int_rs1_rd = RegisterUse {
            source1: Some(Int),
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let int_rs1_rs2_rd = RegisterUse {
            source1: Some(Int),
            source2: Some(Int),
            destination: Some(Int),
            ..RegisterUse::NONE
        };
        let fp_rs1_rs2_rd = RegisterUse {
            source1: Some(Fp),
            source2: Some(Fp),
            destination: Some(Fp),
            ..RegisterUse::NONE
        };

        match self {
            Lui | Auipc | Jal | Csrrwi | Csrrsi | Csrrci => int_rd,
            Jalr | Lb | Lh | Lw | Ld | Lbu | Lhu | Lwu | Addi | Slli | Slti | Sltiu | Xori
            | Srli | Srai | Ori | Andi | Addiw | Slliw | Srliw | Sraiw | Csrrw | Csrrs | Csrrc
            | LrW | LrD => int_rs1_rd,
            Beq | Bne | Blt | Bge | Bltu | Bgeu | Sb | Sh | Sw | Sd => RegisterUse {
                source1: Some(Int),
                source2: Some(Int),
                ..RegisterUse::NONE
            },
            Add | Sub | Sll | Slt | Sltu | Xor | Srl | Sra | Or | And | Mul | Mulh | Mulhsu
            | Mulhu | Div | Divu | Rem | Remu | Addw | Subw | Sllw | Srlw | Sraw | Mulw | Divw
            | Divuw | Remw | Remuw | ScW | ScD | AmoswapW | AmoswapD | AmoaddW | AmoaddD
            | AmoxorW | AmoxorD | AmoorW | AmoorD | AmoandW | AmoandD | AmominW | AmominD
            | AmomaxW | AmomaxD | AmominuW | AmominuD | AmomaxuW | AmomaxuD => int_rs1_rs2_rd,
            Fence | FenceI | Ecall | Ebreak | Mret | Sret | SfenceVma | Wfi => RegisterUse::NONE,
            Flw | Fld | FcvtSW | FcvtSWu | FcvtSL | FcvtSLu | FcvtDW | FcvtDWu | FcvtDL
            | FcvtDLu | FmvWX | FmvDX => RegisterUse {
                source1: Some(Int),
                destination: Some(Fp),
                ..RegisterUse::NONE
            },
            Fsw | Fsd => RegisterUse {
                source1: Some(Int),
                source2: Some(Fp),
                ..RegisterUse::NONE
            },
            FaddS | FaddD | FsubS | FsubD | FmulS | FmulD | FdivS | FdivD | FsgnjS | FsgnjD
            | FsgnjnS | FsgnjnD | FsgnjxS | FsgnjxD | FminS | FminD | FmaxS | FmaxD => {
                fp_rs1_rs2_rd
            }
            FsqrtS | FsqrtD | FcvtSD | FcvtDS => RegisterUse {
                source1: Some(Fp),
                destination: Some(Fp),
                ..RegisterUse::NONE
            },
            FleS | FleD | FltS | FltD | FeqS | FeqD | FcvtWS | FcvtWuS | FcvtLS | FcvtLuS
            | FcvtWD | FcvtWuD | FcvtLD | FcvtLuD | FmvXW | FmvXD | FclassS | FclassD => {
                RegisterUse {
                    source1: Some(Fp),
                    destination: Some(Int),
                    ..RegisterUse::NONE
                }
            }
            FmaddS | FmaddD | FmsubS | FmsubD | FnmsubS | FnmsubD | FnmaddS | FnmaddD => {
                RegisterUse {
                    source1: Some(Fp),
                    source2: Some(Fp),
                    source3: Some(Fp),
                    destination: Some(Fp),
                }
            }
        }
    }

    /// Returns whether memory produces the destination value after execute.
    pub const fn is_load(self) -> bool {
        matches!(
            self,
            Self::Lb
                | Self::Lh
                | Self::Lw
                | Self::Ld
                | Self::Lbu
                | Self::Lhu
                | Self::Lwu
                | Self::Flw
                | Self::Fld
        )
    }

    /// Returns whether this is an atomic memory instruction.
    pub const fn is_atomic(self) -> bool {
        matches!(
            self,
            Self::LrW
                | Self::LrD
                | Self::ScW
                | Self::ScD
                | Self::AmoswapW
                | Self::AmoswapD
                | Self::AmoaddW
                | Self::AmoaddD
                | Self::AmoxorW
                | Self::AmoxorD
                | Self::AmoorW
                | Self::AmoorD
                | Self::AmoandW
                | Self::AmoandD
                | Self::AmominW
                | Self::AmominD
                | Self::AmomaxW
                | Self::AmomaxD
                | Self::AmominuW
                | Self::AmominuD
                | Self::AmomaxuW
                | Self::AmomaxuD
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecodeError, DecodedOperands, EncodingError, Extension, FpReg, ImmediateKind,
        InstructionFormat, InstructionId, IntReg, IsaProfile, LatencyClass, PrivilegeRequirement,
        REGISTER_REGISTER_SPECS, RegisterFile, SupportStatus, UPPER_IMMEDIATE_SPECS,
        decode_instruction, decode_register_register, decode_upper_immediate,
        encode_register_register, encode_upper_immediate, instruction_spec, instruction_specs,
        pcrel_split, register_register_instruction,
    };

    #[test]
    fn register_names_and_aliases_round_trip() {
        for number in 0..32 {
            let integer = IntReg::new(number).expect("valid integer register");
            let float = FpReg::new(number).expect("valid floating-point register");
            assert_eq!(IntReg::parse(integer.abi_name()), Some(integer));
            assert_eq!(FpReg::parse(float.abi_name()), Some(float));
            assert_eq!(IntReg::parse(&format!("x{number}")), Some(integer));
            assert_eq!(FpReg::parse(&format!("f{number}")), Some(float));
        }
        assert_eq!(IntReg::parse("fp"), IntReg::new(8));
    }

    #[test]
    fn register_parsing_rejects_invalid_names_and_numbers() {
        assert_eq!(IntReg::new(32), None);
        assert_eq!(FpReg::new(32), None);
        assert_eq!(IntReg::parse("x32"), None);
        assert_eq!(FpReg::parse("f32"), None);
        assert_eq!(IntReg::parse("notareg"), None);
        assert_eq!(FpReg::parse(""), None);
    }

    #[test]
    fn pc_relative_split_handles_rounding_boundaries() {
        for offset in [-4097, -4096, -2049, -2048, -1, 0, 2047, 2048, 4095, 4096] {
            let (high, low) = pcrel_split(offset);
            assert!((-2048..=2047).contains(&low));
            assert_eq!((i64::from(high) << 12) + i64::from(low), offset);
        }
    }

    #[test]
    fn support_inventory_covers_every_canonical_instruction() {
        assert_eq!(InstructionId::ALL.len(), 160);
        for (index, instruction) in InstructionId::ALL.iter().enumerate() {
            assert!(!InstructionId::ALL[..index].contains(instruction));
            assert!(
                !InstructionId::ALL[..index]
                    .iter()
                    .any(|prior| prior.mnemonic() == instruction.mnemonic())
            );
            let parity = instruction.parity();
            assert_eq!(parity.id, *instruction);
            let expected_implementation = if *instruction == InstructionId::SfenceVma {
                SupportStatus::Partial
            } else {
                SupportStatus::Implemented
            };
            assert_eq!(parity.assembler, expected_implementation);
            assert_eq!(parity.decoder, expected_implementation);
            assert_eq!(parity.metadata, SupportStatus::Implemented);
            let expected_validation = if instruction_spec(*instruction).is_some()
                && *instruction != InstructionId::SfenceVma
            {
                SupportStatus::Implemented
            } else {
                SupportStatus::Partial
            };
            assert_eq!(parity.operand_validation, expected_validation);
        }
    }

    #[test]
    fn support_inventory_records_extensions_and_privilege_gaps() {
        assert_eq!(InstructionId::Add.extension(), Extension::Rv64I);
        assert_eq!(InstructionId::Mul.extension(), Extension::M);
        assert_eq!(InstructionId::LrD.extension(), Extension::A);
        assert_eq!(InstructionId::FaddS.extension(), Extension::F);
        assert_eq!(InstructionId::FaddD.extension(), Extension::D);
        assert_eq!(InstructionId::Csrrw.extension(), Extension::Zicsr);
        assert_eq!(InstructionId::FenceI.extension(), Extension::Zifencei);
        assert_eq!(InstructionId::Mret.extension(), Extension::Privileged);
        assert_eq!(
            InstructionId::Csrrw.privilege_requirement(),
            PrivilegeRequirement::EncodedByOperand
        );
        assert_eq!(
            InstructionId::SfenceVma.privilege_requirement(),
            PrivilegeRequirement::Supervisor
        );
        let partial: Vec<_> = InstructionId::ALL
            .iter()
            .filter(|instruction| instruction.parity().pipeline == SupportStatus::Partial)
            .copied()
            .collect();
        assert_eq!(
            partial,
            [
                InstructionId::Mret,
                InstructionId::SfenceVma,
                InstructionId::Wfi
            ]
        );
    }

    #[test]
    fn isa_profiles_check_shared_extension_metadata() {
        assert!(
            InstructionId::ALL
                .iter()
                .all(|id| IsaProfile::FULL_STACK.supports_instruction(*id))
        );
        assert!(IsaProfile::RV64I.supports_instruction(InstructionId::Add));
        assert!(!IsaProfile::RV64I.supports_instruction(InstructionId::Mul));
        assert!(!IsaProfile::RV64I.supports_instruction(InstructionId::FaddD));
        assert!(!IsaProfile::RV64I.supports_instruction(InstructionId::Mret));
        assert!(
            IsaProfile::RV64I
                .with_extension(Extension::M)
                .supports_instruction(InstructionId::Mul)
        );
    }

    #[test]
    fn upper_immediate_descriptors_are_unambiguous_and_match_metadata() {
        assert_eq!(UPPER_IMMEDIATE_SPECS.len(), 2);
        for (index, spec) in UPPER_IMMEDIATE_SPECS.iter().enumerate() {
            assert_eq!(spec.format, InstructionFormat::UpperImmediate);
            assert_eq!(spec.immediate, Some(ImmediateKind::Upper20));
            assert_eq!(spec.register_use, spec.id.register_use());
            assert_eq!(instruction_spec(spec.id), Some(spec));
            assert!(
                !UPPER_IMMEDIATE_SPECS[..index]
                    .iter()
                    .any(|prior| prior.match_bits == spec.match_bits)
            );
        }
    }

    #[test]
    fn descriptors_cover_every_canonical_instruction_once() {
        let specs: Vec<_> = instruction_specs().collect();
        assert_eq!(specs.len(), InstructionId::ALL.len());
        for id in InstructionId::ALL {
            assert!(
                instruction_spec(*id).is_some(),
                "missing descriptor for {id:?}"
            );
        }
        for (index, spec) in specs.iter().enumerate() {
            assert!(!specs[..index].iter().any(|prior| prior.id == spec.id));
            for other in &specs[..index] {
                assert_ne!(
                    (spec.match_bits ^ other.match_bits) & (spec.mask & other.mask),
                    0,
                    "overlapping descriptors for {:?} and {:?}",
                    spec.id,
                    other.id
                );
            }
            assert_eq!(
                decode_instruction(spec.match_bits).map(|decoded| decoded.id),
                Ok(spec.id)
            );
        }
    }

    #[test]
    fn shared_decoder_rejects_reserved_and_fixed_field_violations() {
        let invalid_rounding_mode = 0x0000_5053;
        let load_reserved_with_rs2 = 0x1010_202f;
        let fence_i_with_destination = 0x0000_108f;
        for word in [
            invalid_rounding_mode,
            load_reserved_with_rs2,
            fence_i_with_destination,
        ] {
            assert_eq!(
                decode_instruction(word),
                Err(DecodeError::UnsupportedEncoding(word))
            );
        }
    }

    #[test]
    fn upper_immediate_family_round_trips_all_registers_and_boundaries() {
        let immediates = [i32::MIN, -4097, -4096, -1, 0, 1, 4095, 4096, i32::MAX];
        for spec in UPPER_IMMEDIATE_SPECS {
            for number in 0..32 {
                let rd = IntReg::new(number).expect("valid register number");
                for immediate in immediates {
                    let word = encode_upper_immediate(spec.id, rd, immediate)
                        .expect("family instruction should encode");
                    let decoded = decode_upper_immediate(word).expect("encoded word should decode");
                    assert_eq!(decoded.id, spec.id);
                    assert_eq!(
                        decoded.operands,
                        DecodedOperands::UpperImmediate {
                            rd,
                            immediate: (immediate.cast_unsigned() & 0xffff_f000).cast_signed(),
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn upper_immediate_family_rejects_other_instructions_and_opcodes() {
        let zero = IntReg::new(0).expect("valid register number");
        assert_eq!(
            encode_upper_immediate(InstructionId::Addi, zero, 0),
            Err(EncodingError::UnsupportedInstruction(InstructionId::Addi))
        );
        assert_eq!(
            decode_upper_immediate(0x0000_0013),
            Err(DecodeError::UnsupportedEncoding(0x0000_0013))
        );
    }

    #[test]
    fn register_register_descriptors_are_unambiguous_and_match_metadata() {
        assert_eq!(REGISTER_REGISTER_SPECS.len(), 28);
        for (index, spec) in REGISTER_REGISTER_SPECS.iter().enumerate() {
            assert!(matches!(
                spec.format,
                InstructionFormat::RegisterRegister | InstructionFormat::RegisterRegisterWord
            ));
            assert_eq!(spec.immediate, None);
            assert_eq!(spec.register_use, spec.id.register_use());
            assert_eq!(instruction_spec(spec.id), Some(spec));
            assert_eq!(
                register_register_instruction(
                    spec.format,
                    ((spec.match_bits >> 12) & 0x7) as u8,
                    ((spec.match_bits >> 25) & 0x7f) as u8,
                ),
                Some(spec.id)
            );
            assert!(
                !REGISTER_REGISTER_SPECS[..index]
                    .iter()
                    .any(|prior| prior.match_bits == spec.match_bits)
            );
            match spec.id.extension() {
                Extension::Rv64I => assert_eq!(spec.latency, LatencyClass::Integer),
                Extension::M => assert!(matches!(
                    spec.latency,
                    LatencyClass::Multiply | LatencyClass::Divide
                )),
                extension => panic!("unexpected extension {extension:?}"),
            }
        }
    }

    #[test]
    fn register_register_family_round_trips_every_register_combination() {
        for spec in REGISTER_REGISTER_SPECS {
            for rd_number in 0..32 {
                let rd = IntReg::new(rd_number).expect("valid destination register");
                for rs1_number in 0..32 {
                    let rs1 = IntReg::new(rs1_number).expect("valid first source register");
                    for rs2_number in 0..32 {
                        let rs2 = IntReg::new(rs2_number).expect("valid second source register");
                        let word = encode_register_register(spec.id, rd, rs1, rs2)
                            .expect("family instruction should encode");
                        assert_eq!(
                            decode_register_register(word),
                            Ok(super::DecodedInstruction {
                                id: spec.id,
                                operands: DecodedOperands::RegisterRegister { rd, rs1, rs2 },
                            })
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn register_register_family_rejects_other_instructions_and_selectors() {
        let zero = IntReg::new(0).expect("valid register number");
        assert_eq!(
            encode_register_register(InstructionId::Addi, zero, zero, zero),
            Err(EncodingError::UnsupportedInstruction(InstructionId::Addi))
        );
        assert_eq!(
            decode_register_register(0xfe00_0033),
            Err(DecodeError::UnsupportedEncoding(0xfe00_0033))
        );
        assert_eq!(
            register_register_instruction(InstructionFormat::RegisterRegisterWord, 2, 0),
            None
        );
    }

    #[test]
    fn instruction_metadata_distinguishes_mixed_register_files() {
        let load = InstructionId::Fld.register_use();
        assert_eq!(load.source1, Some(RegisterFile::Integer));
        assert_eq!(load.destination, Some(RegisterFile::FloatingPoint));

        let conversion = InstructionId::FcvtDW.register_use();
        assert_eq!(conversion.source1, Some(RegisterFile::Integer));
        assert_eq!(conversion.destination, Some(RegisterFile::FloatingPoint));

        let compare = InstructionId::FltD.register_use();
        assert_eq!(compare.source1, Some(RegisterFile::FloatingPoint));
        assert_eq!(compare.destination, Some(RegisterFile::Integer));
    }

    #[test]
    fn memory_classes_cover_integer_fp_and_atomic_loads() {
        assert!(InstructionId::Ld.is_load());
        assert!(InstructionId::Flw.is_load());
        assert!(!InstructionId::LrD.is_load());
        assert!(InstructionId::LrD.is_atomic());
        assert!(!InstructionId::Ld.is_atomic());
    }
}
