use crate::ast::{BinaryOp, UnaryOp};
use crate::ir::IrType;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckError {
    TypeMismatch {
        expected: String,
        found: String,
    },
    InvalidOperation {
        op: String,
        lhs: String,
        rhs: String,
    },
    InvalidUnaryOp {
        op: String,
        ty: String,
    },
}

impl std::fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, found } => {
                write!(f, "type mismatch: expected `{expected}`, found `{found}`")
            }
            Self::InvalidOperation { op, lhs, rhs } => {
                write!(
                    f,
                    "operator `{op}` cannot be applied to `{lhs}` and `{rhs}`"
                )
            }
            Self::InvalidUnaryOp { op, ty } => {
                write!(f, "operator `{op}` cannot be applied to `{ty}`")
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

    pub fn register_types(&mut self, types: &[(String, IrType)]) {
        for (name, ty) in types {
            self.register_type(name.clone(), ty.clone());
        }
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

        // Both operands must be the same type, with two exceptions: integer types
        // of different widths are compatible (e.g. i64 op i32 literal is valid),
        // and float types of different widths are compatible (e.g. an f64 var op a
        // bare float literal, which infers as f32, promotes to f64).
        if lhs_type != rhs_type
            && !lhs_unknown
            && !rhs_unknown
            && !lhs_placeholder
            && !rhs_placeholder
            && !(Self::is_integer_typename(lhs_type) && Self::is_integer_typename(rhs_type))
            && !(Self::is_float_typename(lhs_type) && Self::is_float_typename(rhs_type))
        {
            return Err(TypeCheckError::TypeMismatch {
                expected: lhs_type.to_owned(),
                found: rhs_type.to_owned(),
            });
        }

        match op {
            // Arithmetic operations require numeric types (include shifts)
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                let effective_type = if lhs_unknown {
                    rhs_type
                } else if rhs_unknown {
                    lhs_type
                } else if lhs_placeholder && !rhs_placeholder {
                    rhs_type
                } else if !lhs_placeholder && rhs_placeholder {
                    lhs_type
                } else if Self::is_integer_typename(lhs_type) && Self::is_integer_typename(rhs_type)
                {
                    // Mixed integer widths: promote to wider type
                    Self::promote_integer_types(lhs_type, rhs_type)
                } else if Self::is_float_typename(lhs_type) && Self::is_float_typename(rhs_type) {
                    // Mixed float widths: promote to the wider type so a bare float
                    // literal (f32) does not pin the result narrower than an f64 var.
                    Self::promote_float_types(lhs_type, rhs_type)
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

                // Modulo and shifts are not defined for floating-point operands.
                if matches!(op, BinaryOp::Mod | BinaryOp::Shl | BinaryOp::Shr)
                    && Self::is_float_typename(effective_type)
                {
                    return Err(TypeCheckError::InvalidOperation {
                        op: format!("{op:?}"),
                        lhs: lhs_type.to_owned(),
                        rhs: rhs_type.to_owned(),
                    });
                }

                Ok(effective_type.to_owned())
            }

            // Bitwise operations require integer types
            BinaryOp::BitwiseAnd | BinaryOp::BitwiseXor | BinaryOp::BitwiseOr => {
                let effective_type = if lhs_unknown {
                    rhs_type
                } else if rhs_unknown {
                    lhs_type
                } else if lhs_placeholder && !rhs_placeholder {
                    rhs_type
                } else if !lhs_placeholder && rhs_placeholder {
                    lhs_type
                } else if Self::is_integer_typename(lhs_type) && Self::is_integer_typename(rhs_type)
                {
                    // Mixed integer widths: promote to wider type
                    Self::promote_integer_types(lhs_type, rhs_type)
                } else {
                    lhs_type
                };
                if (!self.is_numeric(effective_type)
                    && !self.is_unknown_like(effective_type)
                    && !self.is_placeholder_like(effective_type))
                    || Self::is_float_typename(effective_type)
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

    fn is_integer_typename(ty: &str) -> bool {
        matches!(
            ty,
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        )
    }

    fn is_float_typename(ty: &str) -> bool {
        matches!(ty, "f32" | "f64")
    }

    /// Promote two integer types to the wider/higher-priority type for mixed-width operations.
    /// Priority (highest to lowest): u64, i64, u32, i32, u16, i16, u8, i8
    fn promote_integer_types<'a>(lhs: &'a str, rhs: &'a str) -> &'a str {
        if lhs == rhs {
            return lhs;
        }

        let width_priority = |ty: &str| match ty {
            "u64" => 8,
            "i64" => 7,
            "u32" => 6,
            "i32" => 5,
            "u16" => 4,
            "i16" => 3,
            "u8" => 2,
            "i8" => 1,
            _ => 0,
        };

        let lhs_pri = width_priority(lhs);
        let rhs_pri = width_priority(rhs);

        if lhs_pri >= rhs_pri { lhs } else { rhs }
    }

    /// Promote two float types to the wider one (f64 outranks f32).
    fn promote_float_types<'a>(lhs: &'a str, rhs: &'a str) -> &'a str {
        if lhs == "f64" || rhs == "f64" {
            "f64"
        } else {
            lhs
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
            IrType::Slice(element) => format!("{}[]", self.get_type_name(element)),
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

#[cfg(test)]
mod tests {
    use super::{TypeCheckError, TypeContext};
    use crate::ast::{BinaryOp, UnaryOp};
    use crate::ir::IrType;

    #[test]
    fn allows_placeholder_arithmetic() {
        let ctx = TypeContext::new();
        assert_eq!(ctx.check_binary_op(&BinaryOp::Add, "T", "T").unwrap(), "T");
    }

    #[test]
    fn still_rejects_non_numeric_named_types() {
        let ctx = TypeContext::new();
        assert!(matches!(
            ctx.check_binary_op(&BinaryOp::Add, "Point", "Point"),
            Err(TypeCheckError::InvalidOperation { .. })
        ));
    }

    #[test]
    fn unary_dereference_and_address_of_round_trip() {
        let ctx = TypeContext::new();
        assert_eq!(
            ctx.check_unary_op(&UnaryOp::AddressOf, "i32").unwrap(),
            "*i32"
        );
        assert_eq!(
            ctx.check_unary_op(&UnaryOp::Dereference, "*i32").unwrap(),
            "i32"
        );
    }

    #[test]
    fn get_type_name_formats_aggregates() {
        let ctx = TypeContext::new();
        let ty = IrType::Aggregate(vec![
            ("x".to_string(), IrType::Named("i32".to_string())),
            ("y".to_string(), IrType::Named("i32".to_string())),
        ]);
        assert_eq!(ctx.get_type_name(&ty), "{ x: i32, y: i32 }");
    }
}
