use crate::high_level_language::ast::{BinaryOp, UnaryOp};
use crate::intermediate_language::IrType;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
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
        let ty_str = format!("{:?}", ty);
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
        let lhs_unknown = self.is_unknown_like(lhs_type);
        let rhs_unknown = self.is_unknown_like(rhs_type);

        // Both operands must be same type
        if lhs_type != rhs_type && !lhs_unknown && !rhs_unknown {
            return Err(TypeCheckError::TypeMismatch {
                expected: lhs_type.to_string(),
                found: rhs_type.to_string(),
            });
        }

        match op {
            // Arithmetic operations require numeric types
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let effective_type = if lhs_unknown { rhs_type } else { lhs_type };
                if !self.is_numeric(effective_type) && !self.is_unknown_like(effective_type) {
                    return Err(TypeCheckError::InvalidOperation {
                        op: format!("{:?}", op),
                        lhs: lhs_type.to_string(),
                        rhs: rhs_type.to_string(),
                    });
                }
                Ok(effective_type.to_string())
            }

            // Logical operations work on bools
            BinaryOp::And | BinaryOp::Or => {
                let effective_type = if lhs_unknown { rhs_type } else { lhs_type };
                if effective_type != "i1"
                    && effective_type != "bool"
                    && !self.is_unknown_like(effective_type)
                {
                    return Err(TypeCheckError::InvalidOperation {
                        op: format!("{:?}", op),
                        lhs: lhs_type.to_string(),
                        rhs: rhs_type.to_string(),
                    });
                }
                Ok("i1".to_string())
            }

            // Comparisons return bool
            BinaryOp::Eq
            | BinaryOp::Neq
            | BinaryOp::Lt
            | BinaryOp::Lte
            | BinaryOp::Gt
            | BinaryOp::Gte => Ok("i1".to_string()),
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
                        op: "negate".to_string(),
                        ty: operand_type.to_string(),
                    });
                }
                Ok(operand_type.to_string())
            }
            UnaryOp::Not => {
                if operand_type != "i1" && operand_type != "bool" {
                    return Err(TypeCheckError::InvalidUnaryOp {
                        op: "not".to_string(),
                        ty: operand_type.to_string(),
                    });
                }
                Ok("i1".to_string())
            }
            UnaryOp::Dereference => Ok(operand_type.to_string()),
            UnaryOp::AddressOf => Ok(format!("*{}", operand_type)),
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

    pub fn get_type_name(&self, ty: &IrType) -> String {
        match ty {
            IrType::Void => "void".to_string(),
            IrType::Integer(width) => format!("{:?}", width).to_lowercase(),
            IrType::Float(width) => format!("{:?}", width).to_lowercase(),
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
