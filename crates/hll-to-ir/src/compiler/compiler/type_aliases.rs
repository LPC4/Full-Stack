use std::collections::{HashMap, HashSet};

use crate::ir::{IrInstruction, IrProgram, IrType, IrTypeAlias};

fn collect_named_types(ty: &IrType, names: &mut Vec<String>) {
    match ty {
        IrType::Pointer(inner) | IrType::Slice(inner) => collect_named_types(inner, names),
        IrType::FunctionPointer {
            params,
            return_type,
        } => {
            for param in params {
                collect_named_types(param, names);
            }
            collect_named_types(return_type, names);
        }
        IrType::Aggregate(fields) => {
            for (_, field_ty) in fields {
                collect_named_types(field_ty, names);
            }
        }
        IrType::Array { element, .. } => collect_named_types(element, names),
        IrType::Named(name) => names.push(name.clone()),
        IrType::Void | IrType::Integer(_) | IrType::Float(_) => {}
    }
}

fn collect_instruction_types(instruction: &IrInstruction, names: &mut Vec<String>) {
    match instruction {
        IrInstruction::Alloc { ty, .. }
        | IrInstruction::HeapAlloc { ty, .. }
        | IrInstruction::Load { ty, .. }
        | IrInstruction::Store { ty, .. }
        | IrInstruction::Offset { ty, .. }
        | IrInstruction::Index { ty, .. }
        | IrInstruction::Math { ty, .. }
        | IrInstruction::Unary { ty, .. }
        | IrInstruction::Cmp { ty, .. }
        | IrInstruction::Cast { ty, .. }
        | IrInstruction::Phi { ty, .. } => collect_named_types(ty, names),
        IrInstruction::IndirectCall { callee_ty, .. } => {
            collect_named_types(callee_ty, names);
        }
        IrInstruction::Comment(_)
        | IrInstruction::SourceLine { .. }
        | IrInstruction::HeapFree { .. }
        | IrInstruction::InlineAsm { .. }
        | IrInstruction::Call { .. }
        | IrInstruction::GlobalRef { .. }
        | IrInstruction::FunctionAddr { .. } => {}
    }
}

fn referenced_named_types(program: &IrProgram) -> Vec<String> {
    let mut names = Vec::new();
    for alias in &program.type_aliases {
        collect_named_types(&alias.ty, &mut names);
    }
    for external in &program.extern_fns {
        for param in &external.params {
            collect_named_types(param, &mut names);
        }
        collect_named_types(&external.return_type, &mut names);
    }
    for global in &program.global_vars {
        collect_named_types(&global.ty, &mut names);
    }
    for function in &program.functions {
        for param in &function.params {
            collect_named_types(&param.ty, &mut names);
        }
        collect_named_types(&function.return_type, &mut names);
        for block in &function.blocks {
            for instruction in &block.instructions {
                collect_instruction_types(instruction, &mut names);
            }
        }
    }
    names
}

pub(super) fn emit_referenced_prelude_types(
    program: &mut IrProgram,
    prelude_types: &[(String, IrType)],
) {
    let prelude: HashMap<&str, &IrType> = prelude_types
        .iter()
        .map(|(name, ty)| (name.as_str(), ty))
        .collect();
    let local_names: HashSet<&str> = program
        .type_aliases
        .iter()
        .map(|alias| alias.name.as_str())
        .collect();
    let mut pending = referenced_named_types(program);
    let mut required = HashSet::new();

    while let Some(name) = pending.pop() {
        if local_names.contains(name.as_str()) || !required.insert(name.clone()) {
            continue;
        }
        if let Some(ty) = prelude.get(name.as_str()) {
            collect_named_types(ty, &mut pending);
        }
    }

    let mut aliases: Vec<IrTypeAlias> = prelude_types
        .iter()
        .filter(|(name, _)| required.contains(name))
        .map(|(name, ty)| IrTypeAlias {
            name: name.clone(),
            ty: ty.clone(),
        })
        .collect();
    aliases.append(&mut program.type_aliases);
    program.type_aliases = aliases;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IntWidth, IrBlock, IrFunction, IrRegister};

    fn named(name: &str) -> IrType {
        IrType::Named(name.to_owned())
    }

    fn alias(name: &str, ty: IrType) -> IrTypeAlias {
        IrTypeAlias {
            name: name.to_owned(),
            ty,
        }
    }

    #[test]
    fn unused_prelude_types_are_omitted_but_authored_aliases_remain() {
        let mut program = IrProgram::new("test");
        program.push_type_alias(alias("UserWord", IrType::Integer(IntWidth::I64)));

        emit_referenced_prelude_types(
            &mut program,
            &[("HeapBlock".to_owned(), IrType::Aggregate(Vec::new()))],
        );

        assert_eq!(
            program.type_aliases,
            [alias("UserWord", IrType::Integer(IntWidth::I64))]
        );
    }

    #[test]
    fn an_instruction_type_retains_its_prelude_alias() {
        let mut program = IrProgram::new("test");
        let mut function = IrFunction::new("main", IrType::Void);
        let mut block = IrBlock::new("entry");
        block.push_instruction(IrInstruction::Alloc {
            dest: IrRegister::Named("block".to_owned()),
            ty: named("HeapBlock"),
            count: None,
        });
        function.push_block(block);
        program.push_function(function);

        emit_referenced_prelude_types(
            &mut program,
            &[("HeapBlock".to_owned(), IrType::Aggregate(Vec::new()))],
        );

        assert_eq!(program.type_aliases[0].name, "HeapBlock");
    }

    #[test]
    fn transitive_recursive_prelude_types_keep_their_original_order() {
        let mut program = IrProgram::new("test");
        program.push_global_var(crate::ir::IrGlobalVar {
            name: "state".to_owned(),
            ty: named("AllocatorState"),
            init: None,
            exported: false,
        });
        let prelude = [
            (
                "HeapBlock".to_owned(),
                IrType::Aggregate(vec![(
                    "next".to_owned(),
                    IrType::Pointer(Box::new(named("HeapBlock"))),
                )]),
            ),
            (
                "AllocatorState".to_owned(),
                IrType::Aggregate(vec![(
                    "head".to_owned(),
                    IrType::Pointer(Box::new(named("HeapBlock"))),
                )]),
            ),
        ];

        emit_referenced_prelude_types(&mut program, &prelude);

        assert_eq!(
            program
                .type_aliases
                .iter()
                .map(|alias| alias.name.as_str())
                .collect::<Vec<_>>(),
            ["HeapBlock", "AllocatorState"]
        );
    }

    #[test]
    fn a_local_alias_shadows_a_prelude_alias() {
        let mut program = IrProgram::new("test");
        program.push_type_alias(alias("HeapBlock", IrType::Integer(IntWidth::I32)));
        program.push_global_var(crate::ir::IrGlobalVar {
            name: "block".to_owned(),
            ty: named("HeapBlock"),
            init: None,
            exported: false,
        });

        emit_referenced_prelude_types(
            &mut program,
            &[("HeapBlock".to_owned(), IrType::Aggregate(Vec::new()))],
        );

        assert_eq!(
            program.type_aliases,
            [alias("HeapBlock", IrType::Integer(IntWidth::I32))]
        );
    }
}
