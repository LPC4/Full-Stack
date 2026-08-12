use hll_to_ir::{IrType, type_layout};
use std::collections::{HashMap, HashSet};

pub fn resolve_ir_type(ty: &IrType, type_aliases: &HashMap<String, IrType>) -> IrType {
    resolve_ir_type_inner(ty, type_aliases, &mut HashSet::new())
}

fn resolve_ir_type_inner(
    ty: &IrType,
    type_aliases: &HashMap<String, IrType>,
    seen: &mut HashSet<String>,
) -> IrType {
    match ty {
        IrType::Named(name) => type_aliases
            .get(name)
            .cloned()
            .map(|resolved| {
                if !seen.insert(name.clone()) {
                    IrType::Named(name.clone())
                } else {
                    let out = resolve_ir_type_inner(&resolved, type_aliases, seen);
                    seen.remove(name);
                    out
                }
            })
            .unwrap_or_else(|| IrType::Named(name.clone())),
        IrType::Pointer(inner) => {
            IrType::Pointer(Box::new(resolve_ir_type_inner(inner, type_aliases, seen)))
        }
        IrType::FunctionPointer {
            params,
            return_type,
        } => IrType::FunctionPointer {
            params: params
                .iter()
                .map(|param| resolve_ir_type_inner(param, type_aliases, seen))
                .collect(),
            return_type: Box::new(resolve_ir_type_inner(return_type, type_aliases, seen)),
        },
        IrType::Array { len, element } => IrType::Array {
            len: *len,
            element: Box::new(resolve_ir_type_inner(element, type_aliases, seen)),
        },
        IrType::Aggregate(fields) => IrType::Aggregate(
            fields
                .iter()
                .map(|(name, field_ty)| {
                    (
                        name.clone(),
                        resolve_ir_type_inner(field_ty, type_aliases, seen),
                    )
                })
                .collect(),
        ),
        IrType::Slice(element) => {
            IrType::Slice(Box::new(resolve_ir_type_inner(element, type_aliases, seen)))
        }
        other => other.clone(),
    }
}

pub fn type_alignment(ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
    type_layout(ty, type_aliases)
        .expect("IR types are validated before code generation")
        .alignment
}

pub fn type_size(ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
    type_layout(ty, type_aliases)
        .expect("IR types are validated before code generation")
        .size
}
