pub mod block;
pub mod instruction;
pub mod ops;
pub mod program;
pub mod types;
pub mod values;

pub use block::IrBlock;
pub use instruction::{IrInstruction, IrTerminator};
pub use ops::{IrCastMode, IrCmpOp, IrMathOp, IrUnaryOp};
pub use program::{
    IrExternFn, IrFunction, IrGlobalString, IrGlobalVar, IrParam, IrProgram, IrTypeAlias,
};
pub use types::{
    FloatWidth, IntWidth, IrType, TypeLayout, TypeLayoutError, type_layout, type_resolves_to_void,
};
pub use values::{IrLabel, IrRegister, IrValue};
