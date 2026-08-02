use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    GlobalString(String), // Reference to a global string constant
}

impl fmt::Display for IrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Register(register) => write!(f, "{register}"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Float(value) if value.is_nan() => write!(f, "nan"),
            Self::Float(value) if value.is_infinite() && value.is_sign_negative() => {
                write!(f, "-inf")
            }
            Self::Float(value) if value.is_infinite() => write!(f, "inf"),
            Self::Float(value) => {
                let text = value.to_string();
                if text.contains(['.', 'e', 'E']) {
                    write!(f, "{text}")
                } else {
                    write!(f, "{text}.0")
                }
            }
            Self::Bool(value) => write!(f, "{}", if *value { "true" } else { "false" }),
            Self::Null => write!(f, "null"),
            Self::GlobalString(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IrLabel(pub String);

impl IrLabel {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for IrLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_remain_lexically_distinct_from_integers() {
        assert_eq!(IrValue::Float(1.0).to_string(), "1.0");
        assert_eq!(IrValue::Float(-0.0).to_string(), "-0.0");
        assert_eq!(IrValue::Float(1.25).to_string(), "1.25");
        assert_eq!(IrValue::Float(f64::INFINITY).to_string(), "inf");
        assert_eq!(IrValue::Float(f64::NEG_INFINITY).to_string(), "-inf");
        assert_eq!(IrValue::Float(f64::NAN).to_string(), "nan");
    }
}
