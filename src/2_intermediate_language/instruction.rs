use crate::intermediate_language::ops::{IrCastMode, IrCmpOp, IrMathOp, IrUnaryOp};
use crate::intermediate_language::types::IrType;
use crate::intermediate_language::values::{IrLabel, IrRegister, IrValue};
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
        count: Option<usize>,
    },
    HeapFree {
        ptr: IrRegister,
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
}

impl fmt::Display for IrInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Comment(message) => write!(f, "; {message}"),
            Self::Alloc { dest, ty, count } => {
                write!(f, "{dest} = alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, " {count}")?;
                }
                Ok(())
            }
            Self::HeapAlloc { dest, ty, count } => {
                write!(f, "{dest} = heap_alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, " {count}")?;
                }
                Ok(())
            }
            Self::HeapFree { ptr } => write!(f, "heap_free {ptr}"),
            Self::Load {
                dest,
                ty,
                ptr,
                offset,
            } => {
                write!(f, "{dest} = load {ty} {ptr}")?;
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
                write!(f, "store {ty} {value} -> {ptr}")?;
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
                write!(f, "call @{function}(")?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
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
        }
    }
}
