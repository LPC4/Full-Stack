use crate::intermediate_language::block::IrBlock;
use crate::intermediate_language::types::IrType;
use crate::intermediate_language::values::IrRegister;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct IrTypeAlias {
    pub name: String,
    pub ty: IrType,
}

impl fmt::Display for IrTypeAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type {} = {}", self.name, self.ty)
    }
}

#[derive(Debug, Clone, PartialEq)]
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
        write!(f, "define {} @{}(", self.return_type, self.name)?;
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
    pub functions: Vec<IrFunction>,
}

impl IrProgram {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            type_aliases: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn push_type_alias(&mut self, alias: IrTypeAlias) {
        self.type_aliases.push(alias);
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

        if !self.type_aliases.is_empty() && !self.functions.is_empty() {
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
    use crate::intermediate_language::instruction::{IrInstruction, IrTerminator};
    use crate::intermediate_language::ops::IrMathOp;
    use crate::intermediate_language::types::{FloatWidth, IntWidth};
    use crate::intermediate_language::values::IrValue;

    #[test]
    fn pretty_print_program_has_registers_labels_and_tabs() {
        let mut program = IrProgram::new("demo");
        program.push_type_alias(IrTypeAlias {
            name: "Point".to_owned(),
            ty: IrType::Aggregate(vec![
                IrType::Float(FloatWidth::F32),
                IrType::Float(FloatWidth::F32),
            ]),
        });

        let mut function = IrFunction::new("add_one", IrType::Integer(IntWidth::I32));
        function.push_param(IrParam {
            ty: IrType::Integer(IntWidth::I32),
            register: IrRegister::Named("value".to_owned()),
        });

        let mut entry = IrBlock::new("entry");
        entry.push_instruction(IrInstruction::Math {
            dest: IrRegister::Temp(0),
            op: IrMathOp::Add,
            ty: IrType::Integer(IntWidth::I32),
            lhs: IrValue::Register(IrRegister::Named("value".to_owned())),
            rhs: IrValue::Integer(1),
        });
        entry.set_terminator(IrTerminator::Return(Some(IrValue::Register(
            IrRegister::Temp(0),
        ))));

        function.push_block(entry);
        program.push_function(function);

        let output = format!("{program}");
        assert!(output.contains("type Point = {f32, f32}"));
        assert!(output.contains("define i32 @add_one(i32 $value) {"));
        assert!(output.contains("@entry:"));
        assert!(output.contains("\t$0 = math add i32 $value, 1"));
        assert!(output.contains("\tret $0"));
    }
}
