#![allow(unused_imports)]

use crate::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Declaration, Expression, Literal, Program, ReturnType,
    Statement, Type, UnaryOp,
};
use crate::compiler::SemanticAnalyzer;
use crate::compiler::utility::LoweringContext;
use crate::ir::{
    FloatWidth, IntWidth, IrBlock, IrCmpOp, IrFunction, IrGlobalString, IrGlobalVar, IrInstruction,
    IrLabel, IrMathOp, IrParam, IrProgram, IrRegister, IrTerminator, IrType, IrTypeAlias,
    IrUnaryOp, IrValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalMode {
    Value,
    Address,
}

#[derive(Debug, Clone)]
struct GenericTypeDef {
    params: Vec<String>,
    ty: Type,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<crate::ast::Parameter>,
    pub return_type: Option<ReturnType>,
    pub body: Option<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    UnsupportedDeclaration(String),
    UnsupportedFeature(&'static str),
}

#[derive(Debug, Clone)]
struct LoweredValue {
    value: IrValue,
    ty: IrType,
    is_unsigned: bool,
}

#[derive(Debug, Clone)]
enum DeferredAction {
    Call {
        function: String,
        args: Vec<IrValue>,
    },
    Expr(Expression),
}

#[derive(Debug)]
pub struct HighLevelCompiler {
    context: LoweringContext,
    next_temp: u32,
    next_label: u32,
    current_blocks: Vec<IrBlock>,
    current_block: Option<IrBlock>,
    defers: Vec<DeferredAction>,
    compile_time_consts: std::collections::HashMap<String, Literal>,
    /// Stack of (`continue_label`, `break_label`) for nested loops
    loop_labels: Vec<(IrLabel, IrLabel)>,
    /// Cache of specialized generic types: (`original_name`, `type_args`) -> `specialized_name`
    generic_type_cache: std::collections::HashMap<(String, Vec<IrType>), String>,
    generic_type_defs: std::collections::HashMap<String, GenericTypeDef>,
    function_return_types: std::collections::HashMap<String, IrType>,
    /// Store function declarations for compile-time evaluation
    function_declarations: std::collections::HashMap<String, FunctionDecl>,
    pending_global_strings: Vec<IrGlobalString>,
    /// Global variables declared at module scope: name -> IR type
    global_vars: std::collections::HashMap<String, IrType>,
    /// Prefix used when naming rodata string-literal labels (e.g. `"str_"` →
    /// `str_0`, `str_1`, ...).  Set per compilation unit so that two units linked
    /// together never produce duplicate label names.
    pub string_prefix: String,
}

impl HighLevelCompiler {
    pub fn new() -> Self {
        Self::with_string_prefix("str_")
    }

    /// Create a compiler that names string literals with a custom prefix.
    /// Use distinct prefixes for each compilation unit that will be linked
    /// together so the assembler never sees duplicate rodata labels.
    pub fn with_string_prefix(prefix: &str) -> Self {
        Self {
            context: LoweringContext::new(),
            next_temp: 0,
            next_label: 0,
            current_blocks: Vec::new(),
            current_block: None,
            defers: Vec::new(),
            compile_time_consts: std::collections::HashMap::new(),
            loop_labels: Vec::new(),
            generic_type_cache: std::collections::HashMap::new(),
            generic_type_defs: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            function_declarations: std::collections::HashMap::new(),
            pending_global_strings: Vec::new(),
            global_vars: std::collections::HashMap::new(),
            string_prefix: prefix.to_owned(),
        }
    }
}

impl Default for HighLevelCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl HighLevelCompiler {
    pub fn compile_program(&mut self, program: &Program) -> Result<IrProgram, CompilerError> {
        log::info!(
            "Starting IR compilation for {} declarations",
            program.declarations.len()
        );

        let mut semantic_analyzer = SemanticAnalyzer::new();
        if let Err(_) = semantic_analyzer.analyze_program(program) {
            // Collect semantic errors and emit them as diagnostics
            for diagnostic in semantic_analyzer.diagnostics() {
                self.context.diagnostics.error(diagnostic.message.clone()); // re-emitted from semantic analysis
            }
            log::warn!(
                "Semantic analysis found errors, continuing with compilation for diagnostics"
            );
        }

        self.context.reset_for_program();
        self.next_temp = 0;
        self.next_label = 0;
        self.pending_global_strings.clear();
        self.global_vars.clear();
        let mut ir_program = IrProgram::new("ir_program");

        for declaration in &program.declarations {
            if let DeclNode::Function {
                name,
                generics,
                return_type,
                ..
            } = &declaration.decl
            {
                let final_name = if generics.is_empty() {
                    name.clone()
                } else {
                    format!("{}<{}>", name, generics.join(", "))
                };
                let return_ty = self.lower_return_type(return_type.as_ref());
                self.function_return_types
                    .insert(final_name.clone(), return_ty.clone());
                if final_name != *name {
                    self.function_return_types.insert(name.clone(), return_ty);
                }
            }
        }

        for declaration in &program.declarations {
            self.lower_declaration(&mut ir_program, declaration)?;
        }

        // Add all pending global strings to the IR program
        for global_string in self.pending_global_strings.drain(..) {
            ir_program.push_global_string(global_string);
        }

        Ok(ir_program)
    }
}

mod control_flow;
mod declarations;
mod expressions;
mod literals;
mod types;
mod utils;
