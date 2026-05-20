use crate::ir::instruction::{IrInstruction, IrTerminator};
use crate::ir::values::IrLabel;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct IrBlock {
    pub label: IrLabel,
    pub instructions: Vec<IrInstruction>,
    pub terminator: Option<IrTerminator>,
}

impl IrBlock {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: IrLabel::new(label),
            instructions: Vec::new(),
            terminator: None,
        }
    }

    pub fn push_instruction(&mut self, instruction: IrInstruction) {
        self.instructions.push(instruction);
    }

    pub fn set_terminator(&mut self, terminator: IrTerminator) {
        self.terminator = Some(terminator);
    }
}

impl fmt::Display for IrBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", self.label)?;

        for instruction in &self.instructions {
            writeln!(f, "\t{instruction}")?;
        }

        if let Some(terminator) = &self.terminator {
            writeln!(f, "\t{terminator}")?;
        }

        Ok(())
    }
}
