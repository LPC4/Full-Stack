use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMathOp {
    Add,
    Sub,
    Mul,
    Div,
    SDiv,
    Mod,
    Shl,
    Shr,
    And,
    Or,
    Xor,
}

impl fmt::Display for IrMathOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::SDiv => "sdiv",
            Self::Mod => "mod",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrUnaryOp {
    Neg,
    Not,
}

impl fmt::Display for IrUnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Neg => "neg",
            Self::Not => "not",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrCmpOp {
    Eq,
    Ne,
    Slt,
    Ult,
    Sle,
    Ule,
    Sgt,
    Ugt,
    Sge,
    Uge,
}

impl fmt::Display for IrCmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Slt => "slt",
            Self::Ult => "ult",
            Self::Sle => "sle",
            Self::Ule => "ule",
            Self::Sgt => "sgt",
            Self::Ugt => "ugt",
            Self::Sge => "sge",
            Self::Uge => "uge",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrCastMode {
    Trunc,
    Zext,
    Sext,
    Bitcast,
    F2i,
    I2f,
}

impl fmt::Display for IrCastMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Trunc => "trunc",
            Self::Zext => "zext",
            Self::Sext => "sext",
            Self::Bitcast => "bitcast",
            Self::F2i => "f2i",
            Self::I2f => "i2f",
        };
        write!(f, "{text}")
    }
}
