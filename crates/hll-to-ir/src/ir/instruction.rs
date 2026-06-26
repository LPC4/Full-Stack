use crate::ir::ops::{IrCastMode, IrCmpOp, IrMathOp, IrUnaryOp};
use crate::ir::types::IrType;
use crate::ir::values::{IrLabel, IrRegister, IrValue};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum IrInstruction {
    Comment(String),
    Alloc {
        dest: IrRegister,
        ty: IrType,
        count: Option<usize>,
    },
    HeapAlloc {
        dest: IrRegister,
        ty: IrType,
        // None = single element. Some = element count (may be a runtime register).
        count: Option<IrValue>,
    },
    HeapFree {
        ptr: IrRegister,
    },
    InlineAsm {
        lines: Vec<String>,
    },
    ReadReg {
        dest: IrRegister,
        reg: String,
    },
    Load {
        dest: IrRegister,
        ty: IrType,
        ptr: IrRegister,
        offset: Option<i64>,
    },
    Store {
        ty: IrType,
        value: IrValue,
        ptr: IrRegister,
        offset: Option<i64>,
    },
    Offset {
        dest: IrRegister,
        ty: IrType,
        ptr: IrRegister,
        bytes: IrValue,
    },
    Index {
        dest: IrRegister,
        ty: IrType,
        base_ptr: IrRegister,
        idx: IrValue,
    },
    Math {
        dest: IrRegister,
        op: IrMathOp,
        ty: IrType,
        lhs: IrValue,
        rhs: IrValue,
    },
    Unary {
        dest: IrRegister,
        op: IrUnaryOp,
        ty: IrType,
        value: IrValue,
    },
    Cmp {
        dest: IrRegister,
        op: IrCmpOp,
        ty: IrType,
        lhs: IrValue,
        rhs: IrValue,
    },
    Cast {
        dest: IrRegister,
        mode: IrCastMode,
        value: IrValue,
        ty: IrType,
    },
    Call {
        dest: Option<IrRegister>,
        function: String,
        args: Vec<IrValue>,
    },
    IndirectCall {
        dest: Option<IrRegister>,
        callee: IrValue,
        callee_ty: IrType,
        args: Vec<IrValue>,
    },
    Phi {
        dest: IrRegister,
        ty: IrType,
        incoming: Vec<(IrValue, IrLabel)>,
    },
    /// Load the address of a named global variable into `dest`.
    GlobalRef {
        dest: IrRegister,
        name: String,
    },
    FunctionAddr {
        dest: IrRegister,
        name: String,
    },
}

impl fmt::Display for IrInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Comment(message) => write!(f, "; {message}"),
            Self::Alloc { dest, ty, count } => {
                write!(f, "{dest} = stack_alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, " {count}")?;
                }
                Ok(())
            }
            Self::HeapAlloc { dest, ty, count } => {
                write!(f, "{dest} = heap_alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, " x{count}")?;
                }
                Ok(())
            }
            Self::HeapFree { ptr } => write!(f, "heap_free {ptr}"),
            Self::InlineAsm { lines } => {
                write!(f, "inline_asm {{")?;
                for line in lines {
                    write!(f, " \"{line}\";")?;
                }
                write!(f, " }}")
            }
            Self::ReadReg { dest, reg } => write!(f, "{dest} = read_reg {reg}"),
            Self::Load {
                dest,
                ty,
                ptr,
                offset,
            } => {
                write!(f, "{dest} = read {ty} @ {ptr}")?;
                if let Some(offset) = offset {
                    write!(f, " + {offset}")?;
                }
                Ok(())
            }
            Self::Store {
                ty,
                value,
                ptr,
                offset,
            } => {
                write!(f, "write {ty} {value} @ {ptr}")?;
                if let Some(offset) = offset {
                    write!(f, " + {offset}")?;
                }
                Ok(())
            }
            Self::Offset {
                dest,
                ty,
                ptr,
                bytes,
            } => write!(f, "{dest} = offset {ty} {ptr}, {bytes}"),
            Self::Index {
                dest,
                ty,
                base_ptr,
                idx,
            } => write!(f, "{dest} = index {ty} {base_ptr}, {idx}"),
            Self::Math {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => write!(f, "{dest} = math {op} {ty} {lhs}, {rhs}"),
            Self::Unary {
                dest,
                op,
                ty,
                value,
            } => write!(f, "{dest} = unary {op} {ty} {value}"),
            Self::Cmp {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => write!(f, "{dest} = cmp {op} {ty} {lhs}, {rhs}"),
            Self::Cast {
                dest,
                mode,
                value,
                ty,
            } => write!(f, "{dest} = cast {mode} {value} -> {ty}"),
            Self::Call {
                dest,
                function,
                args,
            } => {
                if let Some(dest) = dest {
                    write!(f, "{dest} = ")?;
                }
                write!(f, "call {function}(")?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            Self::IndirectCall {
                dest,
                callee,
                callee_ty,
                args,
            } => {
                if let Some(dest) = dest {
                    write!(f, "{dest} = ")?;
                }
                write!(f, "indirect_call {callee_ty} {callee}(")?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            Self::Phi { dest, ty, incoming } => {
                write!(f, "{dest} = phi {ty} ")?;
                for (index, (value, label)) in incoming.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "[ {value}, {label} ]")?;
                }
                Ok(())
            }
            Self::GlobalRef { dest, name } => write!(f, "{dest} = global_ref {name}"),
            Self::FunctionAddr { dest, name } => write!(f, "{dest} = function_addr {name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrTerminator {
    Return(Option<IrValue>),
    Jump(IrLabel),
    Branch {
        cond: IrValue,
        then_label: IrLabel,
        else_label: IrLabel,
    },
    // Abort with a diagnostic code and never return. Used by failed runtime
    // checks like slice bounds. The block has no successor.
    Trap {
        code: u32,
    },
}

impl fmt::Display for IrTerminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Return(Some(value)) => write!(f, "ret {value}"),
            Self::Return(None) => write!(f, "ret"),
            Self::Jump(label) => write!(f, "jump {label}"),
            Self::Branch {
                cond,
                then_label,
                else_label,
            } => write!(f, "branch {cond} ? {then_label} : {else_label}"),
            Self::Trap { code } => write!(f, "trap {code}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_terminator_displays_code() {
        assert_eq!(IrTerminator::Trap { code: 134 }.to_string(), "trap 134");
    }
}
