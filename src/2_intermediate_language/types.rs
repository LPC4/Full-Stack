use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntWidth {
    I1,
    I8,
    I16,
    I32,
    I64,
}

impl fmt::Display for IntWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::I1 => "i1",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatWidth {
    F32,
    F64,
}

impl fmt::Display for FloatWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrType {
    Void,
    Integer(IntWidth),
    Float(FloatWidth),
    Pointer(Box<Self>),
    Aggregate(Vec<(String, Self)>),
    Array { len: usize, element: Box<Self> },
    Named(String),
}

// AFTER:
impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::Integer(width) => write!(f, "{width}"),
            Self::Float(width) => write!(f, "{width}"),
            Self::Pointer(inner) => write!(f, "{inner}*"),
            Self::Aggregate(fields) => {
                write!(f, "{{")?;
                for (index, (name, field_ty)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    // Print named fields as "name: type", unnamed as just "type"
                    if !name.is_empty() {
                        write!(f, "{name}: {field_ty}")?;
                    } else {
                        write!(f, "{field_ty}")?;
                    }
                }
                write!(f, "}}")
            }
            Self::Array { len, element } => write!(f, "{element}[{len}]"),
            Self::Named(name) => write!(f, "{name}"),
        }
    }
}
