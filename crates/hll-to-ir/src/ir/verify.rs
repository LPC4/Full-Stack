use crate::ir::{IrFunction, IrInstruction, IrProgram, IrRegister, IrTerminator, IrType};
use std::collections::{HashMap, HashSet};
use std::fmt;

const MAX_ERRORS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifierInvariant {
    InvalidTypeLayout,
    EmptyIdentity,
    DuplicateType,
    DuplicateExtern,
    DuplicateGlobal,
    DuplicateFunction,
    ConflictingSymbol,
    SignatureMismatch,
    MissingEntryBlock,
    DuplicateBlock,
    DuplicateValue,
    MissingTerminator,
    UnknownBlockTarget,
    EntryBlockPredecessor,
    ReturnValueRequired,
    ReturnValueForbidden,
}

impl VerifierInvariant {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidTypeLayout => "IR001",
            Self::EmptyIdentity => "IR002",
            Self::DuplicateType => "IR003",
            Self::DuplicateExtern => "IR004",
            Self::DuplicateGlobal => "IR005",
            Self::DuplicateFunction => "IR006",
            Self::ConflictingSymbol => "IR007",
            Self::SignatureMismatch => "IR008",
            Self::MissingEntryBlock => "IR009",
            Self::DuplicateBlock => "IR010",
            Self::DuplicateValue => "IR011",
            Self::MissingTerminator => "IR012",
            Self::UnknownBlockTarget => "IR013",
            Self::EntryBlockPredecessor => "IR014",
            Self::ReturnValueRequired => "IR015",
            Self::ReturnValueForbidden => "IR016",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierSource {
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierLocation {
    pub function: Option<String>,
    pub block: Option<String>,
    pub source: Option<VerifierSource>,
}

impl VerifierLocation {
    fn program() -> Self {
        Self {
            function: None,
            block: None,
            source: None,
        }
    }

    fn function(function: &IrFunction) -> Self {
        Self {
            function: Some(function.name.clone()),
            block: None,
            source: None,
        }
    }

    fn block(function: &IrFunction, block: &crate::ir::IrBlock) -> Self {
        Self {
            function: Some(function.name.clone()),
            block: Some(block.label.0.clone()),
            source: block_source(block),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierError {
    pub invariant: VerifierInvariant,
    pub location: VerifierLocation,
    pub related: Option<VerifierLocation>,
    pub message: String,
}

impl fmt::Display for VerifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.invariant.code())?;
        if let Some(function) = &self.location.function {
            write!(f, " in function `{function}`")?;
        }
        if let Some(block) = &self.location.block {
            write!(f, ", block `{block}`")?;
        }
        if let Some(source) = &self.location.source {
            write!(f, " at {}:{}", source.file, source.line)?;
        }
        write!(f, ": {}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierErrors {
    pub errors: Vec<VerifierError>,
    pub truncated: bool,
}

impl fmt::Display for VerifierErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.errors.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        if self.truncated {
            writeln!(f)?;
            write!(f, "additional IR verifier errors were truncated")?;
        }
        Ok(())
    }
}

impl std::error::Error for VerifierErrors {}

#[derive(Default)]
struct ErrorCollector {
    errors: Vec<VerifierError>,
    truncated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefinitionKind {
    Parameter,
    StackLocal,
    Value,
}

struct Definition {
    kind: DefinitionKind,
    location: VerifierLocation,
}

impl ErrorCollector {
    fn push(&mut self, error: VerifierError) {
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(error);
        } else {
            self.truncated = true;
        }
    }

    fn finish(self) -> Result<(), VerifierErrors> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(VerifierErrors {
                errors: self.errors,
                truncated: self.truncated,
            })
        }
    }
}

pub fn verify_program(program: &IrProgram) -> Result<(), VerifierErrors> {
    let mut errors = ErrorCollector::default();
    let types_valid = match program.validate_types() {
        Ok(()) => true,
        Err(error) => {
            errors.push(VerifierError {
                invariant: VerifierInvariant::InvalidTypeLayout,
                location: VerifierLocation::program(),
                related: None,
                message: error.to_string(),
            });
            false
        }
    };

    check_program_identities(program, &mut errors);

    let aliases = program
        .type_aliases
        .iter()
        .map(|alias| (alias.name.clone(), alias.ty.clone()))
        .collect::<HashMap<_, _>>();
    for function in &program.functions {
        let return_is_void = types_valid
            .then(|| crate::ir::type_resolves_to_void(&function.return_type, &aliases).ok())
            .flatten();
        check_function(function, return_is_void, &mut errors);
    }

    errors.finish()
}

pub fn verify_function(function: &IrFunction) -> Result<(), VerifierErrors> {
    let return_is_void = match function.return_type {
        IrType::Void => Some(true),
        IrType::Named(_) => None,
        _ => Some(false),
    };
    let mut errors = ErrorCollector::default();
    check_function(function, return_is_void, &mut errors);
    errors.finish()
}

fn check_program_identities(program: &IrProgram, errors: &mut ErrorCollector) {
    check_name(&program.module_name, "module", errors);
    check_unique_names(
        program.type_aliases.iter().map(|item| item.name.as_str()),
        VerifierInvariant::DuplicateType,
        "type alias",
        errors,
    );
    check_unique_names(
        program.extern_fns.iter().map(|item| item.name.as_str()),
        VerifierInvariant::DuplicateExtern,
        "extern function",
        errors,
    );
    check_unique_names(
        program.functions.iter().map(|item| item.name.as_str()),
        VerifierInvariant::DuplicateFunction,
        "function",
        errors,
    );

    let mut value_symbols = HashMap::<&str, &'static str>::new();
    for (name, kind) in program
        .global_strings
        .iter()
        .map(|item| (item.name.as_str(), "global string"))
        .chain(
            program
                .global_vars
                .iter()
                .map(|item| (item.name.as_str(), "global variable")),
        )
    {
        check_name(name, kind, errors);
        if let Some(previous) = value_symbols.insert(name, kind) {
            errors.push(program_error(
                VerifierInvariant::DuplicateGlobal,
                format!("{kind} `{name}` conflicts with {previous} `{name}`"),
            ));
        }
    }

    for external in &program.extern_fns {
        check_name(&external.name, "extern function", errors);
        if let Some(kind) = value_symbols.get(external.name.as_str()) {
            errors.push(program_error(
                VerifierInvariant::ConflictingSymbol,
                format!("extern function `{}` conflicts with {kind}", external.name),
            ));
        }
    }
    for function in &program.functions {
        check_name(&function.name, "function", errors);
        if let Some(kind) = value_symbols.get(function.name.as_str()) {
            errors.push(program_error(
                VerifierInvariant::ConflictingSymbol,
                format!("function `{}` conflicts with {kind}", function.name),
            ));
        }
        if let Some(external) = program
            .extern_fns
            .iter()
            .find(|external| external.name == function.name)
        {
            let params_match = external.params.len() == function.params.len()
                && external
                    .params
                    .iter()
                    .zip(&function.params)
                    .all(|(external, local)| external == &local.ty);
            if !params_match || external.return_type != function.return_type {
                errors.push(program_error(
                    VerifierInvariant::SignatureMismatch,
                    format!(
                        "definition of `{}` does not match its extern declaration",
                        function.name
                    ),
                ));
            }
        }
    }
}

fn check_unique_names<'a>(
    names: impl Iterator<Item = &'a str>,
    invariant: VerifierInvariant,
    kind: &str,
    errors: &mut ErrorCollector,
) {
    let mut seen = HashSet::new();
    for name in names {
        check_name(name, kind, errors);
        if !seen.insert(name) {
            errors.push(program_error(
                invariant,
                format!("duplicate {kind} `{name}`"),
            ));
        }
    }
}

fn check_name(name: &str, kind: &str, errors: &mut ErrorCollector) {
    if name.is_empty() {
        errors.push(program_error(
            VerifierInvariant::EmptyIdentity,
            format!("{kind} name must not be empty"),
        ));
    }
}

fn check_function(
    function: &IrFunction,
    return_is_void: Option<bool>,
    errors: &mut ErrorCollector,
) {
    if function.name.is_empty() {
        errors.push(VerifierError {
            invariant: VerifierInvariant::EmptyIdentity,
            location: VerifierLocation::function(function),
            related: None,
            message: "function name must not be empty".to_owned(),
        });
    }
    let Some(entry) = function.blocks.first() else {
        errors.push(VerifierError {
            invariant: VerifierInvariant::MissingEntryBlock,
            location: VerifierLocation::function(function),
            related: None,
            message: "function must contain an entry block".to_owned(),
        });
        return;
    };

    let mut blocks = HashMap::<&str, VerifierLocation>::new();
    for block in &function.blocks {
        if block.label.0.is_empty() {
            errors.push(block_error(
                function,
                block,
                VerifierInvariant::EmptyIdentity,
                "block label must not be empty".to_owned(),
            ));
        }
        let location = VerifierLocation::block(function, block);
        if let Some(previous) = blocks.insert(block.label.0.as_str(), location) {
            let mut error = block_error(
                function,
                block,
                VerifierInvariant::DuplicateBlock,
                format!("duplicate block label `{}`", block.label),
            );
            error.related = Some(previous);
            errors.push(error);
        }
    }

    let mut definitions = HashMap::<IrRegister, Definition>::new();
    for param in &function.params {
        let location = VerifierLocation::function(function);
        let definition = Definition {
            kind: DefinitionKind::Parameter,
            location: location.clone(),
        };
        if let Some(previous) = definitions.insert(param.register.clone(), definition) {
            errors.push(VerifierError {
                invariant: VerifierInvariant::DuplicateValue,
                location,
                related: Some(previous.location),
                message: format!("duplicate parameter definition `{}`", param.register),
            });
        }
    }

    for block in &function.blocks {
        check_block_definitions(function, block, &mut definitions, errors);
        let Some(terminator) = &block.terminator else {
            errors.push(block_error(
                function,
                block,
                VerifierInvariant::MissingTerminator,
                "block must end in a terminator".to_owned(),
            ));
            continue;
        };
        check_terminator(
            function,
            block,
            terminator,
            entry,
            &blocks,
            return_is_void,
            errors,
        );
    }
}

fn check_block_definitions(
    function: &IrFunction,
    block: &crate::ir::IrBlock,
    definitions: &mut HashMap<IrRegister, Definition>,
    errors: &mut ErrorCollector,
) {
    let mut source = None;
    for instruction in &block.instructions {
        if let IrInstruction::SourceLine { file, line } = instruction {
            source = Some(VerifierSource {
                file: file.clone(),
                line: *line,
            });
        }
        let Some((dest, kind)) = instruction_definition(instruction) else {
            continue;
        };
        let location = VerifierLocation {
            function: Some(function.name.clone()),
            block: Some(block.label.0.clone()),
            source: source.clone(),
        };
        let definition = Definition {
            kind,
            location: location.clone(),
        };
        if let Some(previous) = definitions.insert(dest.clone(), definition) {
            if matches!(dest, IrRegister::Named(_))
                && previous.kind == DefinitionKind::StackLocal
                && kind == DefinitionKind::StackLocal
            {
                continue;
            }
            errors.push(VerifierError {
                invariant: VerifierInvariant::DuplicateValue,
                location,
                related: Some(previous.location),
                message: format!("value `{dest}` is defined more than once"),
            });
        }
    }
}

fn check_terminator(
    function: &IrFunction,
    block: &crate::ir::IrBlock,
    terminator: &IrTerminator,
    entry: &crate::ir::IrBlock,
    blocks: &HashMap<&str, VerifierLocation>,
    return_is_void: Option<bool>,
    errors: &mut ErrorCollector,
) {
    match terminator {
        IrTerminator::Return(value) => match (return_is_void, value) {
            (Some(true), Some(_)) => errors.push(block_error(
                function,
                block,
                VerifierInvariant::ReturnValueForbidden,
                "void function must not return a value".to_owned(),
            )),
            (Some(false), None) => errors.push(block_error(
                function,
                block,
                VerifierInvariant::ReturnValueRequired,
                "non-void function must return a value".to_owned(),
            )),
            _ => {}
        },
        IrTerminator::Jump(target) => {
            check_target(function, block, target, entry, blocks, errors);
        }
        IrTerminator::Branch {
            then_label,
            else_label,
            ..
        } => {
            check_target(function, block, then_label, entry, blocks, errors);
            check_target(function, block, else_label, entry, blocks, errors);
        }
        IrTerminator::Trap { .. } => {}
    }
}

fn check_target(
    function: &IrFunction,
    block: &crate::ir::IrBlock,
    target: &crate::ir::IrLabel,
    entry: &crate::ir::IrBlock,
    blocks: &HashMap<&str, VerifierLocation>,
    errors: &mut ErrorCollector,
) {
    if !blocks.contains_key(target.0.as_str()) {
        errors.push(block_error(
            function,
            block,
            VerifierInvariant::UnknownBlockTarget,
            format!("branch target `{target}` does not exist"),
        ));
    } else if target == &entry.label {
        errors.push(block_error(
            function,
            block,
            VerifierInvariant::EntryBlockPredecessor,
            format!("entry block `{target}` must not have predecessors"),
        ));
    }
}

fn instruction_definition(instruction: &IrInstruction) -> Option<(&IrRegister, DefinitionKind)> {
    match instruction {
        IrInstruction::Alloc { dest, .. } => Some((dest, DefinitionKind::StackLocal)),
        IrInstruction::HeapAlloc { dest, .. }
        | IrInstruction::Load { dest, .. }
        | IrInstruction::Offset { dest, .. }
        | IrInstruction::Index { dest, .. }
        | IrInstruction::Math { dest, .. }
        | IrInstruction::Unary { dest, .. }
        | IrInstruction::Cmp { dest, .. }
        | IrInstruction::Cast { dest, .. }
        | IrInstruction::Phi { dest, .. }
        | IrInstruction::GlobalRef { dest, .. }
        | IrInstruction::FunctionAddr { dest, .. } => Some((dest, DefinitionKind::Value)),
        IrInstruction::Call { dest, .. } | IrInstruction::IndirectCall { dest, .. } => {
            dest.as_ref().map(|dest| (dest, DefinitionKind::Value))
        }
        IrInstruction::Comment(_)
        | IrInstruction::SourceLine { .. }
        | IrInstruction::HeapFree { .. }
        | IrInstruction::InlineAsm { .. }
        | IrInstruction::Store { .. } => None,
    }
}

fn block_source(block: &crate::ir::IrBlock) -> Option<VerifierSource> {
    block.instructions.iter().find_map(|instruction| {
        let IrInstruction::SourceLine { file, line } = instruction else {
            return None;
        };
        Some(VerifierSource {
            file: file.clone(),
            line: *line,
        })
    })
}

fn program_error(invariant: VerifierInvariant, message: String) -> VerifierError {
    VerifierError {
        invariant,
        location: VerifierLocation::program(),
        related: None,
        message,
    }
}

fn block_error(
    function: &IrFunction,
    block: &crate::ir::IrBlock,
    invariant: VerifierInvariant,
    message: String,
) -> VerifierError {
    VerifierError {
        invariant,
        location: VerifierLocation::block(function, block),
        related: None,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IntWidth, IrBlock, IrLabel, IrValue};

    fn i32_type() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    fn function_with_return(name: &str) -> IrFunction {
        let mut function = IrFunction::new(name, i32_type());
        let mut entry = IrBlock::new("entry");
        entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
        function.push_block(entry);
        function
    }

    fn codes(result: Result<(), VerifierErrors>) -> Vec<VerifierInvariant> {
        result
            .expect_err("fixture must be rejected")
            .errors
            .into_iter()
            .map(|error| error.invariant)
            .collect()
    }

    #[test]
    fn accepts_a_structurally_valid_program() {
        let mut program = IrProgram::new("valid");
        program.push_function(function_with_return("main"));
        assert_eq!(verify_program(&program), Ok(()));
    }

    #[test]
    fn invariant_codes_are_stable() {
        let invariants = [
            VerifierInvariant::InvalidTypeLayout,
            VerifierInvariant::EmptyIdentity,
            VerifierInvariant::DuplicateType,
            VerifierInvariant::DuplicateExtern,
            VerifierInvariant::DuplicateGlobal,
            VerifierInvariant::DuplicateFunction,
            VerifierInvariant::ConflictingSymbol,
            VerifierInvariant::SignatureMismatch,
            VerifierInvariant::MissingEntryBlock,
            VerifierInvariant::DuplicateBlock,
            VerifierInvariant::DuplicateValue,
            VerifierInvariant::MissingTerminator,
            VerifierInvariant::UnknownBlockTarget,
            VerifierInvariant::EntryBlockPredecessor,
            VerifierInvariant::ReturnValueRequired,
            VerifierInvariant::ReturnValueForbidden,
        ];
        assert_eq!(
            invariants.map(VerifierInvariant::code),
            [
                "IR001", "IR002", "IR003", "IR004", "IR005", "IR006", "IR007", "IR008", "IR009",
                "IR010", "IR011", "IR012", "IR013", "IR014", "IR015", "IR016",
            ]
        );
    }

    #[test]
    fn reports_invalid_layout() {
        let mut program = IrProgram::new("layout");
        program.push_function(IrFunction::new(
            "broken",
            IrType::Named("Missing".to_owned()),
        ));
        assert_eq!(
            codes(verify_program(&program))[0],
            VerifierInvariant::InvalidTypeLayout
        );
    }

    #[test]
    fn reports_program_identity_and_signature_errors() {
        let mut program = IrProgram::new("");
        for _ in 0..2 {
            program.push_type_alias(crate::ir::IrTypeAlias {
                name: "Word".to_owned(),
                ty: i32_type(),
            });
            program.extern_fns.push(crate::ir::IrExternFn {
                name: "external".to_owned(),
                params: Vec::new(),
                return_type: i32_type(),
            });
            program.push_function(function_with_return("duplicate"));
        }
        program.global_vars.push(crate::ir::IrGlobalVar {
            name: "collision".to_owned(),
            ty: i32_type(),
            init: None,
            exported: false,
        });
        program.push_function(function_with_return("collision"));
        program.extern_fns.push(crate::ir::IrExternFn {
            name: "declared".to_owned(),
            params: vec![i32_type()],
            return_type: i32_type(),
        });
        program.push_function(function_with_return("declared"));

        let actual = codes(verify_program(&program));
        for invariant in [
            VerifierInvariant::EmptyIdentity,
            VerifierInvariant::DuplicateType,
            VerifierInvariant::DuplicateExtern,
            VerifierInvariant::DuplicateFunction,
            VerifierInvariant::ConflictingSymbol,
            VerifierInvariant::SignatureMismatch,
        ] {
            assert!(
                actual.contains(&invariant),
                "missing {invariant:?}: {actual:?}"
            );
        }
    }

    #[test]
    fn reports_function_entry_block_and_return_errors() {
        assert_eq!(
            codes(verify_function(&IrFunction::new("empty", i32_type()))),
            vec![VerifierInvariant::MissingEntryBlock]
        );

        let mut function = IrFunction::new("broken", i32_type());
        let mut entry = IrBlock::new("entry");
        entry.set_terminator(IrTerminator::Jump(IrLabel::new("entry")));
        function.push_block(entry);
        let mut duplicate = IrBlock::new("entry");
        duplicate.set_terminator(IrTerminator::Return(None));
        function.push_block(duplicate);
        assert_eq!(
            codes(verify_function(&function)),
            vec![
                VerifierInvariant::DuplicateBlock,
                VerifierInvariant::EntryBlockPredecessor,
                VerifierInvariant::ReturnValueRequired,
            ]
        );

        let mut void_function = IrFunction::new("void", IrType::Void);
        let mut void_entry = IrBlock::new("entry");
        void_entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
        void_function.push_block(void_entry);
        assert_eq!(
            codes(verify_function(&void_function)),
            vec![VerifierInvariant::ReturnValueForbidden]
        );
    }

    #[test]
    fn reports_duplicate_symbols_in_source_order() {
        let mut program = IrProgram::new("duplicates");
        program.push_function(function_with_return("same"));
        program.push_function(function_with_return("same"));
        program.global_vars.push(crate::ir::IrGlobalVar {
            name: "data".to_owned(),
            ty: i32_type(),
            init: None,
            exported: false,
        });
        program.global_strings.push(crate::ir::IrGlobalString {
            name: "data".to_owned(),
            content: String::new(),
        });

        assert_eq!(
            codes(verify_program(&program)),
            vec![
                VerifierInvariant::DuplicateFunction,
                VerifierInvariant::DuplicateGlobal,
            ]
        );
    }

    #[test]
    fn reports_missing_terminators_and_targets_together() {
        let mut function = IrFunction::new("broken", i32_type());
        let mut entry = IrBlock::new("entry");
        entry.push_instruction(IrInstruction::SourceLine {
            file: "broken.hll".to_owned(),
            line: 9,
        });
        entry.set_terminator(IrTerminator::Jump(IrLabel::new("missing")));
        function.push_block(entry);
        function.push_block(IrBlock::new("tail"));

        let errors = verify_function(&function).expect_err("fixture must be rejected");
        assert_eq!(
            errors
                .errors
                .iter()
                .map(|error| error.invariant)
                .collect::<Vec<_>>(),
            vec![
                VerifierInvariant::UnknownBlockTarget,
                VerifierInvariant::MissingTerminator,
            ]
        );
        assert_eq!(
            errors.errors[0].location.source,
            Some(VerifierSource {
                file: "broken.hll".to_owned(),
                line: 9,
            })
        );
    }

    #[test]
    fn reports_duplicate_values_and_return_shape() {
        let register = IrRegister::Named("value".to_owned());
        let mut function = IrFunction::new("broken", IrType::Void);
        function.push_param(crate::ir::IrParam {
            ty: i32_type(),
            register: register.clone(),
        });
        let mut entry = IrBlock::new("entry");
        entry.push_instruction(IrInstruction::Math {
            dest: register,
            op: crate::ir::IrMathOp::Add,
            ty: i32_type(),
            lhs: IrValue::Integer(1),
            rhs: IrValue::Integer(2),
        });
        entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(3))));
        function.push_block(entry);

        assert_eq!(
            codes(verify_function(&function)),
            vec![
                VerifierInvariant::DuplicateValue,
                VerifierInvariant::ReturnValueForbidden,
            ]
        );
    }

    #[test]
    fn permits_reused_named_stack_locals_before_mem2reg() {
        let register = IrRegister::Named("local".to_owned());
        let mut function = IrFunction::new("scopes", i32_type());
        let mut entry = IrBlock::new("entry");
        for _ in 0..2 {
            entry.push_instruction(IrInstruction::Alloc {
                dest: register.clone(),
                ty: i32_type(),
                count: None,
            });
        }
        entry.set_terminator(IrTerminator::Return(Some(IrValue::Integer(0))));
        function.push_block(entry);

        assert_eq!(verify_function(&function), Ok(()));
    }

    #[test]
    fn caps_independent_errors() {
        let mut function = IrFunction::new("many", i32_type());
        for index in 0..(MAX_ERRORS + 3) {
            function.push_block(IrBlock::new(format!("block_{index}")));
        }
        let errors = verify_function(&function).expect_err("fixture must be rejected");
        assert_eq!(errors.errors.len(), MAX_ERRORS);
        assert!(errors.truncated);
    }
}
