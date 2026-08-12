use std::collections::{HashMap, HashSet};
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
    FunctionPointer {
        params: Vec<Self>,
        return_type: Box<Self>,
    },
    Aggregate(Vec<(String, Self)>),
    Array {
        len: usize,
        element: Box<Self>,
    },
    // Slice fat pointer {ptr, len}, 16 bytes. Kept separate from Aggregate so the
    // front end can spot it for bounds checks, for-loops, and coercion.
    Slice(Box<Self>),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeLayout {
    pub size: usize,
    pub alignment: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeLayoutError {
    UnresolvedName(String),
    AliasCycle(String),
    Overflow,
}

impl fmt::Display for TypeLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedName(name) => write!(f, "unresolved IR type `{name}`"),
            Self::AliasCycle(name) => write!(f, "IR type alias cycle through `{name}`"),
            Self::Overflow => write!(f, "IR type layout arithmetic overflow"),
        }
    }
}

impl std::error::Error for TypeLayoutError {}

pub fn type_layout(
    ty: &IrType,
    aliases: &HashMap<String, IrType>,
) -> Result<TypeLayout, TypeLayoutError> {
    type_layout_inner(ty, aliases, &mut HashSet::new())
}

pub fn type_resolves_to_void(
    ty: &IrType,
    aliases: &HashMap<String, IrType>,
) -> Result<bool, TypeLayoutError> {
    let mut resolving = HashSet::new();
    let mut current = ty;
    loop {
        match current {
            IrType::Void => return Ok(true),
            IrType::Named(name) => {
                if !resolving.insert(name.clone()) {
                    return Err(TypeLayoutError::AliasCycle(name.clone()));
                }
                current = aliases
                    .get(name)
                    .ok_or_else(|| TypeLayoutError::UnresolvedName(name.clone()))?;
            }
            _ => return Ok(false),
        }
    }
}

fn type_layout_inner(
    ty: &IrType,
    aliases: &HashMap<String, IrType>,
    resolving: &mut HashSet<String>,
) -> Result<TypeLayout, TypeLayoutError> {
    let scalar = |size| TypeLayout {
        size,
        alignment: size.max(1),
    };
    match ty {
        IrType::Void => Ok(scalar(0)),
        IrType::Integer(width) => Ok(scalar(match width {
            IntWidth::I1 | IntWidth::I8 => 1,
            IntWidth::I16 => 2,
            IntWidth::I32 => 4,
            IntWidth::I64 => 8,
        })),
        IrType::Float(width) => Ok(scalar(match width {
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        })),
        IrType::Pointer(_) | IrType::FunctionPointer { .. } => Ok(scalar(8)),
        IrType::Slice(_) => Ok(TypeLayout {
            size: 16,
            alignment: 8,
        }),
        IrType::Array { len, element } => {
            let element = type_layout_inner(element, aliases, resolving)?;
            Ok(TypeLayout {
                size: element
                    .size
                    .checked_mul(*len)
                    .ok_or(TypeLayoutError::Overflow)?,
                alignment: element.alignment,
            })
        }
        IrType::Aggregate(fields) => {
            let mut size = 0usize;
            let mut alignment = 1usize;
            for (_, field) in fields {
                let field = type_layout_inner(field, aliases, resolving)?;
                alignment = alignment.max(field.alignment);
                size = checked_align(size, field.alignment)?;
                size = size
                    .checked_add(field.size)
                    .ok_or(TypeLayoutError::Overflow)?;
            }
            Ok(TypeLayout {
                size: checked_align(size, alignment)?,
                alignment,
            })
        }
        IrType::Named(name) => {
            let Some(resolved) = aliases.get(name) else {
                return Err(TypeLayoutError::UnresolvedName(name.clone()));
            };
            if !resolving.insert(name.clone()) {
                return Err(TypeLayoutError::AliasCycle(name.clone()));
            }
            let layout = type_layout_inner(resolved, aliases, resolving);
            resolving.remove(name);
            layout
        }
    }
}

fn checked_align(value: usize, alignment: usize) -> Result<usize, TypeLayoutError> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum & !(alignment - 1))
        .ok_or(TypeLayoutError::Overflow)
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::Integer(width) => write!(f, "{width}"),
            Self::Float(width) => write!(f, "{width}"),
            Self::Pointer(inner) => write!(f, "{inner}*"),
            Self::FunctionPointer {
                params,
                return_type,
            } => {
                write!(f, "fn(")?;
                for (index, param) in params.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                if matches!(return_type.as_ref(), Self::Void) {
                    write!(f, ")")
                } else {
                    write!(f, ") -> {return_type}")
                }
            }
            Self::Aggregate(fields) => {
                if fields.is_empty() {
                    return write!(f, "{{}}");
                }
                write!(f, "{{ ")?;
                for (index, (name, field_ty)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    if !name.is_empty() {
                        write!(f, "{name}: {field_ty}")?;
                    } else {
                        write!(f, "{field_ty}")?;
                    }
                }
                write!(f, " }}")
            }
            Self::Array { len, element } => write!(f, "{element}[{len}]"),
            Self::Slice(element) => write!(f, "{element}[]"),
            Self::Named(name) => write!(f, "{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_use_ir_2_spelling() {
        let aggregate = IrType::Aggregate(vec![
            ("x".to_owned(), IrType::Float(FloatWidth::F32)),
            (String::new(), IrType::Integer(IntWidth::I64)),
        ]);
        let function = IrType::FunctionPointer {
            params: vec![IrType::Pointer(Box::new(aggregate.clone()))],
            return_type: Box::new(IrType::Integer(IntWidth::I32)),
        };

        assert_eq!(aggregate.to_string(), "{ x: f32, i64 }");
        assert_eq!(IrType::Aggregate(Vec::new()).to_string(), "{}");
        assert_eq!(function.to_string(), "fn({ x: f32, i64 }*) -> i32");
        assert_eq!(
            IrType::FunctionPointer {
                params: Vec::new(),
                return_type: Box::new(IrType::Void),
            }
            .to_string(),
            "fn()"
        );
    }

    #[test]
    fn layouts_resolve_nested_aliases_and_padding() {
        let aliases = HashMap::from([
            ("Byte".to_owned(), IrType::Integer(IntWidth::I8)),
            ("Alias".to_owned(), IrType::Named("Byte".to_owned())),
        ]);
        let ty = IrType::Aggregate(vec![
            ("head".to_owned(), IrType::Named("Alias".to_owned())),
            (
                "items".to_owned(),
                IrType::Array {
                    len: 2,
                    element: Box::new(IrType::Integer(IntWidth::I64)),
                },
            ),
        ]);
        assert_eq!(
            type_layout(&ty, &aliases),
            Ok(TypeLayout {
                size: 24,
                alignment: 8
            })
        );
        assert_eq!(
            type_layout(&IrType::Void, &aliases),
            Ok(TypeLayout {
                size: 0,
                alignment: 1
            })
        );
    }

    #[test]
    fn layouts_report_unresolved_cycles_and_overflow() {
        let aliases = HashMap::from([
            ("A".to_owned(), IrType::Named("B".to_owned())),
            ("B".to_owned(), IrType::Named("A".to_owned())),
        ]);
        assert_eq!(
            type_layout(&IrType::Named("Missing".to_owned()), &aliases),
            Err(TypeLayoutError::UnresolvedName("Missing".to_owned()))
        );
        assert!(matches!(
            type_layout(&IrType::Named("A".to_owned()), &aliases),
            Err(TypeLayoutError::AliasCycle(_))
        ));
        assert_eq!(
            type_layout(
                &IrType::Array {
                    len: usize::MAX,
                    element: Box::new(IrType::Integer(IntWidth::I64)),
                },
                &HashMap::new()
            ),
            Err(TypeLayoutError::Overflow)
        );
    }

    #[test]
    fn void_detection_follows_aliases() {
        let aliases = HashMap::from([
            ("Nothing".to_owned(), IrType::Void),
            ("Alias".to_owned(), IrType::Named("Nothing".to_owned())),
        ]);
        assert_eq!(
            type_resolves_to_void(&IrType::Named("Alias".to_owned()), &aliases),
            Ok(true)
        );
    }
}
