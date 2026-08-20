use crate::ir::ops::{IrCastMode, IrCmpOp, IrMathOp, IrUnaryOp};
use crate::ir::types::IrType;
use crate::ir::values::{IrLabel, IrRegister, IrValue};
use std::fmt;

/// A resolved inline-asm place with exactly one local slot or global symbol.
/// `offset` and `ty` describe the selected storage within that base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrAsmPlace {
    pub local: Option<IrRegister>,
    pub symbol: Option<String>,
    pub offset: i64,
    pub ty: IrType,
}

impl IrAsmPlace {
    fn fmt_place(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(local) = &self.local {
            write!(f, "{local}")?;
        } else if let Some(symbol) = &self.symbol {
            write!(f, "&{symbol}")?;
        }
        if self.offset != 0 {
            write!(f, " + {}", self.offset)?;
        }
        Ok(())
    }
}

/// A raw instruction or typed storage bridge at its authored position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrAsmOp {
    Raw(String),
    /// `mv reg, $place`
    Load {
        reg: String,
        place: IrAsmPlace,
    },
    /// `mv $place, reg`
    Store {
        place: IrAsmPlace,
        reg: String,
    },
    /// `la reg, $place`
    Addr {
        reg: String,
        place: IrAsmPlace,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrInstruction {
    Comment(String),
    /// Source-location marker for the pc-to-source line table. Emits no code;
    /// the backend turns it into a text-invisible assembler marker.
    SourceLine {
        file: String,
        line: u32,
    },
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
        ops: Vec<IrAsmOp>,
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
            Self::SourceLine { file, line } => write!(f, "; .loc {file}:{line}"),
            Self::Alloc { dest, ty, count } => {
                write!(f, "{dest} = stack_alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, ", {count}")?;
                }
                Ok(())
            }
            Self::HeapAlloc { dest, ty, count } => {
                write!(f, "{dest} = heap_alloc {ty}")?;
                if let Some(count) = count {
                    write!(f, ", {count}")?;
                }
                Ok(())
            }
            Self::HeapFree { ptr } => write!(f, "heap_free {ptr}"),
            Self::InlineAsm { ops } => {
                write!(f, "inline_asm {{")?;
                for op in ops {
                    write!(f, " ")?;
                    match op {
                        IrAsmOp::Raw(line) => {
                            let escaped = line.replace('\\', "\\\\").replace('"', "\\\"");
                            write!(f, "raw \"{escaped}\";")?;
                        }
                        IrAsmOp::Load { reg, place } => {
                            write!(f, "mv {reg}, ")?;
                            place.fmt_place(f)?;
                            write!(f, ";")?;
                        }
                        IrAsmOp::Store { place, reg } => {
                            write!(f, "mv ")?;
                            place.fmt_place(f)?;
                            write!(f, ", {reg};")?;
                        }
                        IrAsmOp::Addr { reg, place } => {
                            write!(f, "la {reg}, ")?;
                            place.fmt_place(f)?;
                            write!(f, ";")?;
                        }
                    }
                }
                write!(f, " }}")
            }
            Self::Load {
                dest,
                ty,
                ptr,
                offset,
            } => {
                write!(f, "{dest} = load {ty}, {ptr}")?;
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
                write!(f, "store {ty} {value}, {ptr}")?;
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
            } => write!(f, "{dest} = {op} {ty} {lhs}, {rhs}"),
            Self::Unary {
                dest,
                op,
                ty,
                value,
            } => write!(f, "{dest} = {op} {ty} {value}"),
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
            } => write!(f, "{dest} = {mode} {value} to {ty}"),
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
                write!(f, "indirect_call {callee}(")?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ") : {callee_ty}")
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
    use crate::ir::types::IntWidth;

    fn reg(name: &str) -> IrRegister {
        IrRegister::Named(name.to_owned())
    }

    fn value(name: &str) -> IrValue {
        IrValue::Register(reg(name))
    }

    fn i32_type() -> IrType {
        IrType::Integer(IntWidth::I32)
    }

    fn i64_type() -> IrType {
        IrType::Integer(IntWidth::I64)
    }

    #[test]
    fn instructions_use_ir_2_spelling() {
        let function_type = IrType::FunctionPointer {
            params: vec![i32_type()],
            return_type: Box::new(i32_type()),
        };
        let cases = [
            (
                IrInstruction::Comment("reason".to_owned()),
                "; reason".to_owned(),
            ),
            (
                IrInstruction::SourceLine {
                    file: "main.hll".to_owned(),
                    line: 7,
                },
                "; .loc main.hll:7".to_owned(),
            ),
            (
                IrInstruction::Alloc {
                    dest: reg("slot"),
                    ty: i32_type(),
                    count: Some(4),
                },
                "$slot = stack_alloc i32, 4".to_owned(),
            ),
            (
                IrInstruction::HeapAlloc {
                    dest: reg("items"),
                    ty: i32_type(),
                    count: Some(value("count")),
                },
                "$items = heap_alloc i32, $count".to_owned(),
            ),
            (
                IrInstruction::HeapFree { ptr: reg("items") },
                "heap_free $items".to_owned(),
            ),
            (
                IrInstruction::InlineAsm {
                    ops: vec![
                        IrAsmOp::Raw("ecall".to_owned()),
                        IrAsmOp::Load {
                            reg: "a0".to_owned(),
                            place: IrAsmPlace {
                                local: Some(reg("fd")),
                                symbol: None,
                                offset: 0,
                                ty: i32_type(),
                            },
                        },
                        IrAsmOp::Store {
                            place: IrAsmPlace {
                                local: None,
                                symbol: Some("_ret".to_owned()),
                                offset: 0,
                                ty: i64_type(),
                            },
                            reg: "a0".to_owned(),
                        },
                    ],
                },
                "inline_asm { raw \"ecall\"; mv a0, $fd; mv &_ret, a0; }".to_owned(),
            ),
            (
                IrInstruction::Load {
                    dest: reg("v"),
                    ty: i32_type(),
                    ptr: reg("ptr"),
                    offset: Some(4),
                },
                "$v = load i32, $ptr + 4".to_owned(),
            ),
            (
                IrInstruction::Store {
                    ty: i32_type(),
                    value: value("v"),
                    ptr: reg("ptr"),
                    offset: None,
                },
                "store i32 $v, $ptr".to_owned(),
            ),
            (
                IrInstruction::Offset {
                    dest: reg("field"),
                    ty: i32_type(),
                    ptr: reg("ptr"),
                    bytes: IrValue::Integer(4),
                },
                "$field = offset i32 $ptr, 4".to_owned(),
            ),
            (
                IrInstruction::Index {
                    dest: reg("element"),
                    ty: i32_type(),
                    base_ptr: reg("ptr"),
                    idx: value("index"),
                },
                "$element = index i32 $ptr, $index".to_owned(),
            ),
            (
                IrInstruction::Math {
                    dest: reg("sum"),
                    op: IrMathOp::Add,
                    ty: i32_type(),
                    lhs: value("a"),
                    rhs: value("b"),
                },
                "$sum = add i32 $a, $b".to_owned(),
            ),
            (
                IrInstruction::Unary {
                    dest: reg("negative"),
                    op: IrUnaryOp::Neg,
                    ty: i32_type(),
                    value: value("a"),
                },
                "$negative = neg i32 $a".to_owned(),
            ),
            (
                IrInstruction::Cmp {
                    dest: reg("less"),
                    op: IrCmpOp::Slt,
                    ty: i32_type(),
                    lhs: value("a"),
                    rhs: value("b"),
                },
                "$less = cmp slt i32 $a, $b".to_owned(),
            ),
            (
                IrInstruction::Cast {
                    dest: reg("wide"),
                    mode: IrCastMode::Sext,
                    value: value("a"),
                    ty: IrType::Integer(IntWidth::I64),
                },
                "$wide = sext $a to i64".to_owned(),
            ),
            (
                IrInstruction::Call {
                    dest: Some(reg("result")),
                    function: "sum".to_owned(),
                    args: vec![value("a"), value("b")],
                },
                "$result = call sum($a, $b)".to_owned(),
            ),
            (
                IrInstruction::IndirectCall {
                    dest: Some(reg("result")),
                    callee: value("function"),
                    callee_ty: function_type,
                    args: vec![value("a")],
                },
                "$result = indirect_call $function($a) : fn(i32) -> i32".to_owned(),
            ),
            (
                IrInstruction::Phi {
                    dest: reg("merged"),
                    ty: i32_type(),
                    incoming: vec![(value("a"), IrLabel::new("if_then"))],
                },
                "$merged = phi i32 [ $a, if_then ]".to_owned(),
            ),
            (
                IrInstruction::GlobalRef {
                    dest: reg("global"),
                    name: "counter".to_owned(),
                },
                "$global = global_ref counter".to_owned(),
            ),
            (
                IrInstruction::FunctionAddr {
                    dest: reg("function"),
                    name: "sum".to_owned(),
                },
                "$function = function_addr sum".to_owned(),
            ),
        ];

        for (instruction, expected) in cases {
            assert_eq!(instruction.to_string(), expected);
        }
    }

    #[test]
    fn terminators_use_ir_2_spelling() {
        assert_eq!(IrTerminator::Return(None).to_string(), "ret");
        assert_eq!(
            IrTerminator::Return(Some(value("result"))).to_string(),
            "ret $result"
        );
        assert_eq!(
            IrTerminator::Jump(IrLabel::new("done")).to_string(),
            "jump done"
        );
        assert_eq!(
            IrTerminator::Branch {
                cond: value("condition"),
                then_label: IrLabel::new("yes"),
                else_label: IrLabel::new("no"),
            }
            .to_string(),
            "branch $condition ? yes : no"
        );
        assert_eq!(IrTerminator::Trap { code: 134 }.to_string(), "trap 134");
    }
}
