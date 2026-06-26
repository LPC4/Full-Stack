use hll_to_ir::{FloatWidth, IntWidth, IrType};
use log::warn;
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
    match resolve_ir_type(ty, type_aliases) {
        IrType::Void => 1,
        IrType::Integer(w) => match w {
            IntWidth::I1 | IntWidth::I8 => 1,
            IntWidth::I16 => 2,
            IntWidth::I32 => 4,
            IntWidth::I64 => 8,
        },
        IrType::Float(w) => match w {
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        IrType::Pointer(_)
        | IrType::FunctionPointer { .. }
        | IrType::Named(_)
        | IrType::Slice(_) => 8,
        IrType::Array { element, .. } => type_alignment(&element, type_aliases),
        IrType::Aggregate(fields) => fields
            .iter()
            .map(|(_, ft)| type_alignment(ft, type_aliases))
            .max()
            .unwrap_or(1),
    }
}

pub fn type_size(ty: &IrType, type_aliases: &HashMap<String, IrType>) -> usize {
    match resolve_ir_type(ty, type_aliases) {
        IrType::Void => 0,
        IrType::Integer(w) => match w {
            IntWidth::I1 | IntWidth::I8 => 1,
            IntWidth::I16 => 2,
            IntWidth::I32 => 4,
            IntWidth::I64 => 8,
        },
        IrType::Float(w) => match w {
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        IrType::Pointer(_) | IrType::FunctionPointer { .. } => 8,
        IrType::Slice(_) => 16,
        IrType::Array { len, element } => len * type_size(&element, type_aliases),
        IrType::Aggregate(fields) => {
            let mut offset: usize = 0;
            let mut max_align: usize = 1;
            for (_, field_ty) in &fields {
                let align = type_alignment(field_ty, type_aliases);
                max_align = max_align.max(align);
                offset = (offset + align - 1) & !(align - 1);
                offset += type_size(field_ty, type_aliases);
            }
            (offset + max_align - 1) & !(max_align - 1)
        }
        IrType::Named(_) => {
            warn!("Cannot compute size of unresolved named type; defaulting to 8");
            8
        }
    }
}
