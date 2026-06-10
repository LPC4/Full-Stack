//! IR-level optimization passes (const_fold, dce). Off by default; see _LANG_SPECIFICATIONS.md.

mod const_fold;
mod dce;

use crate::ir::{IrFunction, IrProgram};

/// Which optimization passes to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OptOptions {
    pub const_fold: bool,
    pub dce: bool,
}

impl OptOptions {
    /// No passes (the default).
    pub fn none() -> Self {
        Self::default()
    }

    /// Every available pass.
    pub fn all() -> Self {
        Self {
            const_fold: true,
            dce: true,
        }
    }

    /// True if any pass is enabled.
    pub fn any(&self) -> bool {
        self.const_fold || self.dce
    }
}

/// Run the enabled passes over every function in `program`.
pub fn optimize(program: &mut IrProgram, opts: OptOptions) {
    if !opts.any() {
        return;
    }
    for func in &mut program.functions {
        optimize_function(func, opts);
    }
}

/// Run enabled passes over one function to a fixpoint (folding + DCE alternate).
fn optimize_function(func: &mut IrFunction, opts: OptOptions) {
    const MAX_ROUNDS: usize = 16;
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        if opts.const_fold {
            changed |= const_fold::run(func);
        }
        if opts.dce {
            changed |= dce::run(func);
        }
        if !changed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        IntWidth, IrBlock, IrInstruction, IrMathOp, IrRegister, IrTerminator, IrType, IrValue,
    };

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn i32_ty() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    #[test]
    fn fold_then_dce_collapses_dead_constant_chain() {
        // $a = 2 + 3; $b = $a * 4; ret $b  ->  ret 20
        let mut program = IrProgram::new("m");
        let mut func = IrFunction::new("f", i32_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(IrInstruction::Math {
            dest: reg("a"),
            op: IrMathOp::Add,
            ty: i32_ty(),
            lhs: IrValue::Integer(2),
            rhs: IrValue::Integer(3),
        });
        block.push_instruction(IrInstruction::Math {
            dest: reg("b"),
            op: IrMathOp::Mul,
            ty: i32_ty(),
            lhs: IrValue::Register(reg("a")),
            rhs: IrValue::Integer(4),
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("b")))));
        func.push_block(block);
        program.push_function(func);

        optimize(&mut program, OptOptions::all());

        let block = &program.functions[0].blocks[0];
        assert!(
            block.instructions.is_empty(),
            "dead constant chain should be eliminated, got {:?}",
            block.instructions
        );
        assert_eq!(
            block.terminator,
            Some(IrTerminator::Return(Some(IrValue::Integer(20))))
        );
    }

    #[test]
    fn none_is_a_no_op() {
        let mut program = IrProgram::new("m");
        let mut func = IrFunction::new("f", i32_ty());
        let mut block = IrBlock::new("entry");
        block.push_instruction(IrInstruction::Math {
            dest: reg("a"),
            op: IrMathOp::Add,
            ty: i32_ty(),
            lhs: IrValue::Integer(2),
            rhs: IrValue::Integer(3),
        });
        block.set_terminator(IrTerminator::Return(Some(IrValue::Register(reg("a")))));
        func.push_block(block);
        program.push_function(func);
        let before = format!("{program}");

        optimize(&mut program, OptOptions::none());

        assert_eq!(before, format!("{program}"), "none() must not change IR");
    }
}
