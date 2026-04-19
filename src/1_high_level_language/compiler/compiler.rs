#![allow(unused_imports)]

use crate::high_level_language::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Declaration, Expression, Literal, Program, ReturnType,
    Statement, Type, UnaryOp,
};
use crate::high_level_language::compiler::SemanticAnalyzer;
use crate::high_level_language::compiler::utility::LoweringContext;
use crate::intermediate_language::{
    FloatWidth, IntWidth, IrBlock, IrCmpOp, IrFunction, IrInstruction, IrLabel, IrMathOp, IrParam,
    IrProgram, IrRegister, IrTerminator, IrType, IrTypeAlias, IrUnaryOp, IrValue,
};

#[derive(Debug, Clone)]
struct GenericTypeDef {
    params: Vec<String>,
    ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompilerError {
    UnsupportedDeclaration(String),
    UnsupportedFeature(&'static str),
}

#[derive(Debug, Clone)]
struct LoweredValue {
    value: IrValue,
    ty: IrType,
}

#[derive(Debug, Clone)]
enum DeferredAction {
    Call {
        function: String,
        args: Vec<IrValue>,
    },
    Expr(Expression),
}

#[derive(Debug, Default)]
pub struct HighLevelCompiler {
    context: LoweringContext,
    next_temp: u32,
    next_label: u32,
    current_blocks: Vec<IrBlock>,
    current_block: Option<IrBlock>,
    /// Stack of deferred actions to execute at the end of the current function/block
    defers: Vec<DeferredAction>,
    /// Map of compile-time constant names to their evaluated literal values
    compile_time_consts: std::collections::HashMap<String, Literal>,
    /// Stack of (continue_label, break_label) for nested loops
    loop_labels: Vec<(IrLabel, IrLabel)>,
    /// Cache of specialized generic types: (original_name, type_args) -> specialized_name
    generic_type_cache: std::collections::HashMap<(String, Vec<IrType>), String>,
    /// Store generic type definitions for later specialization
    generic_type_defs: std::collections::HashMap<String, GenericTypeDef>,
    /// Map of function names to their return types
    function_return_types: std::collections::HashMap<String, IrType>,
}

#[path = "assignments.rs"]
mod assignments;
#[path = "control_flow.rs"]
mod control_flow;
#[path = "declarations.rs"]
mod declarations;
#[path = "expressions.rs"]
mod expressions;
#[path = "types.rs"]
mod types;
#[path = "utils.rs"]
mod utils;
