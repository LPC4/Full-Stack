use crate::ir::block::IrBlock;
use crate::ir::instruction::IrInstruction;
use crate::ir::types::IrType;
use crate::ir::types::{TypeLayoutError, type_layout};
use crate::ir::values::IrRegister;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrTypeAlias {
    pub name: String,
    pub ty: IrType,
}

impl fmt::Display for IrTypeAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type {} = {}", self.name, self.ty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrGlobalString {
    pub name: String,
    pub content: String,
}

impl fmt::Display for IrGlobalString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let escaped = self
            .content
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r");
        write!(f, "const {} = c\"{}\"", self.name, escaped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrExternFn {
    pub name: String,
    pub params: Vec<IrType>,
    pub return_type: IrType,
}

impl fmt::Display for IrExternFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "extern {}(", self.name)?;
        for (index, param) in self.params.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        write!(f, ") -> {}", self.return_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrParam {
    pub ty: IrType,
    pub register: IrRegister,
}

impl fmt::Display for IrParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.register, self.ty)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrFunction {
    pub name: String,
    pub return_type: IrType,
    pub params: Vec<IrParam>,
    pub blocks: Vec<IrBlock>,
    pub exported: bool,
}

impl IrFunction {
    pub fn new(name: impl Into<String>, return_type: IrType) -> Self {
        Self {
            name: name.into(),
            return_type,
            params: Vec::new(),
            blocks: Vec::new(),
            exported: true,
        }
    }

    pub fn set_exported(&mut self, exported: bool) {
        self.exported = exported;
    }

    pub fn push_block(&mut self, block: IrBlock) {
        self.blocks.push(block);
    }

    pub fn push_param(&mut self, param: IrParam) {
        self.params.push(param);
    }
}

impl fmt::Display for IrFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exported {
            write!(f, "export ")?;
        }
        write!(f, "define {}(", self.name)?;
        for (index, param) in self.params.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        writeln!(f, ") -> {} {{", self.return_type)?;

        for block in &self.blocks {
            write!(f, "{block}")?;
        }

        writeln!(f, "}}")
    }
}

/// A global variable emitted into the `.bss` / `.data` section.
#[derive(Debug, Clone)]
pub struct IrGlobalVar {
    pub name: String,
    pub ty: IrType,
    /// Raw initial bytes; `None` means zero-initialised (goes in `.bss`).
    pub init: Option<Vec<u8>>,
    pub exported: bool,
}

impl fmt::Display for IrGlobalVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exported {
            write!(f, "export ")?;
        }
        write!(f, "global {}: {} = ", self.name, self.ty)?;
        let Some(bytes) = &self.init else {
            return write!(f, "zero");
        };
        if bytes.len() > 1 && bytes.iter().all(|byte| byte == &bytes[0]) {
            return write!(f, "bytes[0x{:02x}; {}]", bytes[0], bytes.len());
        }
        write!(f, "bytes[")?;
        for (index, byte) in bytes.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "0x{byte:02x}")?;
        }
        write!(f, "]")
    }
}

#[derive(Debug)]
pub struct IrProgram {
    pub module_name: String,
    pub type_aliases: Vec<IrTypeAlias>,
    pub extern_fns: Vec<IrExternFn>,
    pub global_strings: Vec<IrGlobalString>,
    pub global_vars: Vec<IrGlobalVar>,
    pub functions: Vec<IrFunction>,
}

impl IrProgram {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            type_aliases: Vec::new(),
            extern_fns: Vec::new(),
            global_strings: Vec::new(),
            global_vars: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn push_type_alias(&mut self, alias: IrTypeAlias) {
        self.type_aliases.push(alias);
    }

    pub fn push_extern_fn(&mut self, ext: IrExternFn) {
        if !self.extern_fns.iter().any(|e| e.name == ext.name) {
            self.extern_fns.push(ext);
        }
    }

    pub fn push_global_string(&mut self, global_string: IrGlobalString) {
        self.global_strings.push(global_string);
    }

    pub fn push_global_var(&mut self, var: IrGlobalVar) {
        self.global_vars.push(var);
    }

    pub fn push_function(&mut self, function: IrFunction) {
        self.functions.push(function);
    }

    pub fn validate_types(&self) -> Result<(), TypeLayoutError> {
        let aliases = self
            .type_aliases
            .iter()
            .map(|alias| (alias.name.clone(), alias.ty.clone()))
            .collect::<HashMap<_, _>>();

        for alias in &self.type_aliases {
            validate_type_tree(&alias.ty, &aliases)?;
        }
        for external in &self.extern_fns {
            for param in &external.params {
                validate_type_tree(param, &aliases)?;
            }
            validate_type_tree(&external.return_type, &aliases)?;
        }
        for global in &self.global_vars {
            validate_type_tree(&global.ty, &aliases)?;
        }
        for function in &self.functions {
            validate_type_tree(&function.return_type, &aliases)?;
            for param in &function.params {
                validate_type_tree(&param.ty, &aliases)?;
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let Some(ty) = instruction_type(instruction) {
                        validate_type_tree(ty, &aliases)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn instruction_type(instruction: &IrInstruction) -> Option<&IrType> {
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
        | IrInstruction::Phi { ty, .. } => Some(ty),
        IrInstruction::IndirectCall { callee_ty, .. } => Some(callee_ty),
        IrInstruction::Comment(_)
        | IrInstruction::SourceLine { .. }
        | IrInstruction::HeapFree { .. }
        | IrInstruction::InlineAsm { .. }
        | IrInstruction::Call { .. }
        | IrInstruction::GlobalRef { .. }
        | IrInstruction::FunctionAddr { .. } => None,
    }
}

fn validate_type_tree(
    ty: &IrType,
    aliases: &HashMap<String, IrType>,
) -> Result<(), TypeLayoutError> {
    type_layout(ty, aliases)?;
    match ty {
        IrType::Pointer(inner) | IrType::Slice(inner) => validate_type_tree(inner, aliases),
        IrType::FunctionPointer {
            params,
            return_type,
        } => {
            for param in params {
                validate_type_tree(param, aliases)?;
            }
            validate_type_tree(return_type, aliases)
        }
        IrType::Aggregate(fields) => {
            for (_, field) in fields {
                validate_type_tree(field, aliases)?;
            }
            Ok(())
        }
        IrType::Array { element, .. } => validate_type_tree(element, aliases),
        IrType::Void | IrType::Integer(_) | IrType::Float(_) | IrType::Named(_) => Ok(()),
    }
}

impl fmt::Display for IrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for alias in &self.type_aliases {
            writeln!(f, "{alias}")?;
        }
        for ext in &self.extern_fns {
            writeln!(f, "{ext}")?;
        }

        let has_declarations = !self.type_aliases.is_empty() || !self.extern_fns.is_empty();
        let has_globals = !self.global_strings.is_empty() || !self.global_vars.is_empty();
        if has_declarations && (has_globals || !self.functions.is_empty()) {
            writeln!(f)?;
        }

        for global_string in &self.global_strings {
            writeln!(f, "{global_string}")?;
        }
        for global_var in &self.global_vars {
            writeln!(f, "{global_var}")?;
        }

        if has_globals && !self.functions.is_empty() {
            writeln!(f)?;
        }

        for (index, function) in self.functions.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{function}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::instruction::IrTerminator;
    use crate::ir::types::IntWidth;

    fn i32_type() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    #[test]
    fn declarations_and_globals_use_ir_2_spelling() {
        let ext = IrExternFn {
            name: "sum".to_owned(),
            params: vec![i32_type(), i32_type()],
            return_type: i32_type(),
        };
        assert_eq!(ext.to_string(), "extern sum(i32, i32) -> i32");

        let zero = IrGlobalVar {
            name: "counter".to_owned(),
            ty: i32_type(),
            init: None,
            exported: false,
        };
        assert_eq!(zero.to_string(), "global counter: i32 = zero");

        let repeated = IrGlobalVar {
            name: "padding".to_owned(),
            ty: IrType::Array {
                len: 4,
                element: Box::new(IrType::Integer(IntWidth::I8)),
            },
            init: Some(vec![0; 4]),
            exported: true,
        };
        assert_eq!(
            repeated.to_string(),
            "export global padding: i8[4] = bytes[0x00; 4]"
        );

        let bytes = IrGlobalVar {
            name: "pair".to_owned(),
            ty: IrType::Array {
                len: 2,
                element: Box::new(IrType::Integer(IntWidth::I8)),
            },
            init: Some(vec![1, 2]),
            exported: false,
        };
        assert_eq!(bytes.to_string(), "global pair: i8[2] = bytes[0x01, 0x02]");
    }

    #[test]
    fn program_prints_every_top_level_group() {
        let mut program = IrProgram::new("demo");
        program.push_type_alias(IrTypeAlias {
            name: "Word".to_owned(),
            ty: i32_type(),
        });
        program.push_extern_fn(IrExternFn {
            name: "sink".to_owned(),
            params: vec![i32_type()],
            return_type: IrType::Void,
        });
        program.push_global_string(IrGlobalString {
            name: "message".to_owned(),
            content: "hi\n\"there\"".to_owned(),
        });
        program.push_global_var(IrGlobalVar {
            name: "counter".to_owned(),
            ty: i32_type(),
            init: None,
            exported: false,
        });

        let mut function = IrFunction::new("main", i32_type());
        function.set_exported(false);
        function.push_param(IrParam {
            ty: i32_type(),
            register: IrRegister::Named("value".to_owned()),
        });
        let mut entry = IrBlock::new("entry");
        entry.set_terminator(IrTerminator::Return(Some(crate::ir::IrValue::Integer(0))));
        function.push_block(entry);
        program.push_function(function);

        assert_eq!(
            program.to_string(),
            concat!(
                "type Word = i32\n",
                "extern sink(i32) -> void\n",
                "\n",
                "const message = c\"hi\\n\\\"there\\\"\"\n",
                "global counter: i32 = zero\n",
                "\n",
                "define main($value: i32) -> i32 {\n",
                "entry:\n",
                "    ret 0\n",
                "}\n",
            )
        );
    }
}
