use crate::high_level_language::ast::{BinaryOp, UnaryOp};
use crate::intermediate_language::IrType;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckError {
    TypeMismatch {
        expected: String,
        found: String,
    },
    UndefinedType(String),
    InvalidOperation {
        op: String,
        lhs: String,
        rhs: String,
    },
    InvalidUnaryOp {
        op: String,
        ty: String,
    },
    InvalidCast {
        from: String,
        to: String,
    },
}

impl std::fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, found } => {
                write!(f, "type mismatch: expected `{expected}`, found `{found}`")
            }
            Self::UndefinedType(name) => write!(f, "undefined type `{name}`"),
            Self::InvalidOperation { op, lhs, rhs } => {
                write!(
                    f,
                    "operator `{op}` cannot be applied to `{lhs}` and `{rhs}`"
                )
            }
            Self::InvalidUnaryOp { op, ty } => {
                write!(f, "operator `{op}` cannot be applied to `{ty}`")
            }
            Self::InvalidCast { from, to } => {
                write!(f, "cannot cast `{from}` to `{to}`")
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct TypeContext {
    named_types: HashMap<String, IrType>,
    type_cache: HashMap<String, String>,
}

impl TypeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.named_types.clear();
        self.type_cache.clear();
    }

    pub fn register_type(&mut self, name: impl Into<String>, ty: IrType) {
        let name_str = name.into();
        let ty_str = format!("{ty:?}");
        self.type_cache.insert(name_str.clone(), ty_str);
        self.named_types.insert(name_str, ty);
    }

    pub fn resolve(&self, name: &str) -> Option<&IrType> {
        self.named_types.get(name)
    }

    /// Type check a binary operation and return the result type or error
    pub fn check_binary_op(
        &self,
        op: &BinaryOp,
        lhs_type: &str,
        rhs_type: &str,
    ) -> Result<String, TypeCheckError> {
        // Determine if either side is a pointer type.
        let lhs_is_ptr =
            lhs_type.ends_with('*') || lhs_type.starts_with('*') || lhs_type == "*unknown";
        let rhs_is_ptr =
            rhs_type.ends_with('*') || rhs_type.starts_with('*') || rhs_type == "*unknown";

        // One side pointer, other side integer/unknown => pointer arithmetic
        if (lhs_is_ptr != rhs_is_ptr) // exactly one side is a pointer
            && (self.is_numeric(rhs_type) || self.is_numeric(lhs_type)
            || rhs_type == "i32"
            || lhs_type == "i32"
            || rhs_type == "i64"
            || lhs_type == "i64"
            || rhs_type == "unknown"
            || lhs_type == "unknown")
        {
            return match op {
                BinaryOp::Add | BinaryOp::Sub => {
                    // Result is the pointer type
                    if lhs_is_ptr {
                        Ok(lhs_type.to_owned())
                    } else {
                        Ok(rhs_type.to_owned())
                    }
                }
                _ => Err(TypeCheckError::InvalidOperation {
                    op: format!("{op:?}"),
                    lhs: lhs_type.to_owned(),
                    rhs: rhs_type.to_owned(),
                }),
            };
        }

        let lhs_unknown = self.is_unknown_like(lhs_type);
        let rhs_unknown = self.is_unknown_like(rhs_type);
        let lhs_placeholder = self.is_placeholder_like(lhs_type);
        let rhs_placeholder = self.is_placeholder_like(rhs_type);

        // Both operands must be same type
        if lhs_type != rhs_type
            && !lhs_unknown
            && !rhs_unknown
            && !lhs_placeholder
            && !rhs_placeholder
        {
            return Err(TypeCheckError::TypeMismatch {
                expected: lhs_type.to_owned(),
                found: rhs_type.to_owned(),
            });
        }

        match op {
            // Arithmetic operations require numeric types
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let effective_type = if lhs_unknown {
                    rhs_type
                } else if rhs_unknown {
                    lhs_type
                } else if lhs_placeholder && !rhs_placeholder {
                    rhs_type
                } else {
                    lhs_type
                };
                if !self.is_numeric(effective_type)
                    && !self.is_unknown_like(effective_type)
                    && !self.is_placeholder_like(effective_type)
                {
                    return Err(TypeCheckError::InvalidOperation {
                        op: format!("{op:?}"),
                        lhs: lhs_type.to_owned(),
                        rhs: rhs_type.to_owned(),
                    });
                }

                Ok(effective_type.to_owned())
            }

            // Logical operations work on bools
            BinaryOp::And | BinaryOp::Or => {
                let effective_type = if lhs_unknown { rhs_type } else { lhs_type };
                if effective_type != "i1"
                    && effective_type != "bool"
                    && !self.is_unknown_like(effective_type)
                {
                    return Err(TypeCheckError::InvalidOperation {
                        op: format!("{op:?}"),
                        lhs: lhs_type.to_owned(),
                        rhs: rhs_type.to_owned(),
                    });
                }
                Ok("i1".to_owned())
            }

            // Comparisons return bool
            BinaryOp::Eq
            | BinaryOp::Neq
            | BinaryOp::Lt
            | BinaryOp::Lte
            | BinaryOp::Gt
            | BinaryOp::Gte => Ok("i1".to_owned()),
        }
    }

    /// Type check a unary operation
    pub fn check_unary_op(
        &self,
        op: &UnaryOp,
        operand_type: &str,
    ) -> Result<String, TypeCheckError> {
        match op {
            UnaryOp::Negate => {
                if !self.is_numeric(operand_type) {
                    return Err(TypeCheckError::InvalidUnaryOp {
                        op: "negate".to_owned(),
                        ty: operand_type.to_owned(),
                    });
                }
                Ok(operand_type.to_owned())
            }
            UnaryOp::Not => {
                if operand_type != "i1" && operand_type != "bool" {
                    return Err(TypeCheckError::InvalidUnaryOp {
                        op: "not".to_owned(),
                        ty: operand_type.to_owned(),
                    });
                }
                Ok("i1".to_owned())
            }
            UnaryOp::Dereference => {
                if let Some(inner) = operand_type.strip_prefix('*') {
                    Ok(inner.to_owned())
                } else if self.is_unknown_like(operand_type) {
                    Ok("unknown".to_owned())
                } else {
                    Err(TypeCheckError::InvalidUnaryOp {
                        op: "dereference".to_owned(),
                        ty: operand_type.to_owned(),
                    })
                }
            }
            UnaryOp::AddressOf => Ok(format!("*{operand_type}")),
        }
    }

    fn is_numeric(&self, ty: &str) -> bool {
        matches!(
            ty,
            "i1" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64"
        )
    }

    fn is_unknown_like(&self, ty: &str) -> bool {
        ty == "unknown" || ty == "*unknown"
    }

    fn is_placeholder_like(&self, ty: &str) -> bool {
        if self.is_unknown_like(ty) {
            return true;
        }

        let trimmed = ty.trim();
        let core = trimmed.strip_prefix('*').unwrap_or(trimmed);
        if core.is_empty() {
            return false;
        }

        core.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    }

    pub fn get_type_name(&self, ty: &IrType) -> String {
        match ty {
            IrType::Void => "void".to_owned(),
            IrType::Integer(width) => format!("{width}"),
            IrType::Float(width) => format!("{width}"),
            IrType::Pointer(inner) => format!("*{}", self.get_type_name(inner)),
            IrType::Array { element, len } => {
                format!("{}[{}]", self.get_type_name(element), len)
            }
            IrType::Aggregate(fields) => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(name, f)| format!("{}: {}", name, self.get_type_name(f)))
                    .collect();
                format!("{{ {} }}", field_strs.join(", "))
            }
            IrType::Named(name) => name.clone(),
        }
    }
}
