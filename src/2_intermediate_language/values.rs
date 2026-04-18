use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrRegister {
    Temp(u32),
    Named(String),
}

impl fmt::Display for IrRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temp(index) => write!(f, "${index}"),
            Self::Named(name) => write!(f, "${name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrValue {
    Register(IrRegister),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl fmt::Display for IrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Register(register) => write!(f, "{register}"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{}", if *value { "true" } else { "false" }),
            Self::Null => write!(f, "null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrLabel(pub String);

impl IrLabel {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for IrLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}
