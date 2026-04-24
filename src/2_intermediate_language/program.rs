use crate::intermediate_language::block::IrBlock;
use crate::intermediate_language::types::IrType;
use crate::intermediate_language::values::IrRegister;
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
        // Escape the string content for IR output
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
pub struct IrParam {
    pub ty: IrType,
    pub register: IrRegister,
}

impl fmt::Display for IrParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.ty, self.register)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrFunction {
    pub name: String,
    pub return_type: IrType,
    pub params: Vec<IrParam>,
    pub blocks: Vec<IrBlock>,
}

impl IrFunction {
    pub fn new(name: impl Into<String>, return_type: IrType) -> Self {
        Self {
            name: name.into(),
            return_type,
            params: Vec::new(),
            blocks: Vec::new(),
        }
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
        write!(f, "define {} {}(", self.return_type, self.name)?;
        for (index, param) in self.params.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        writeln!(f, ") {{")?;

        for block in &self.blocks {
            write!(f, "{block}")?;
        }

        writeln!(f, "}}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram {
    pub module_name: String,
    pub type_aliases: Vec<IrTypeAlias>,
    pub global_strings: Vec<IrGlobalString>,
    pub functions: Vec<IrFunction>,
}

impl IrProgram {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            type_aliases: Vec::new(),
            global_strings: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn push_type_alias(&mut self, alias: IrTypeAlias) {
        self.type_aliases.push(alias);
    }

    pub fn push_global_string(&mut self, global_string: IrGlobalString) {
        self.global_strings.push(global_string);
    }

    pub fn push_function(&mut self, function: IrFunction) {
        self.functions.push(function);
    }
}

impl fmt::Display for IrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for alias in &self.type_aliases {
            writeln!(f, "{alias}")?;
        }

        if !self.type_aliases.is_empty()
            && (!self.global_strings.is_empty() || !self.functions.is_empty())
        {
            writeln!(f)?;
        }

        for global_string in &self.global_strings {
            writeln!(f, "{global_string}")?;
        }

        if !self.global_strings.is_empty() && !self.functions.is_empty() {
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
