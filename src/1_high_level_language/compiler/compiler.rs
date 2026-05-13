#![allow(unused_imports)]

use crate::high_level_language::ast::{
    AssignTarget, BinaryOp, Block, DeclNode, Declaration, Expression, Literal, Program, ReturnType,
    Statement, Type, UnaryOp,
};
use crate::high_level_language::compiler::SemanticAnalyzer;
use crate::high_level_language::compiler::utility::LoweringContext;
use crate::intermediate_language::{
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
    pub params: Vec<crate::high_level_language::ast::Parameter>,
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

#[derive(Debug, Default)]
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
    /// Extern function return types from a pre-compiled stdlib, applied during compile_program.
    extern_fn_returns: std::collections::HashMap<String, IrType>,
    /// Extern type aliases from a pre-compiled stdlib, re-applied after reset_for_program().
    extern_ty_aliases: Vec<IrTypeAlias>,
}

impl HighLevelCompiler {
    pub fn new() -> Self {
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
            extern_fn_returns: std::collections::HashMap::new(),
            extern_ty_aliases: Vec::new(),
        }
    }

    pub fn compile_program(&mut self, program: &Program) -> Result<IrProgram, CompilerError> {
        log::info!(
            "Starting IR compilation for {} declarations",
            program.declarations.len()
        );

        let mut semantic_analyzer = SemanticAnalyzer::new();
        if !self.extern_fn_returns.is_empty() {
            semantic_analyzer.seed_extern_fn_returns(&self.extern_fn_returns);
        }
        if !self.extern_ty_aliases.is_empty() {
            semantic_analyzer.seed_extern_type_aliases(&self.extern_ty_aliases);
        }
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
        // Re-apply extern type aliases cleared by reset
        for alias in &self.extern_ty_aliases {
            self.context.types.register_type(alias.name.clone(), alias.ty.clone());
        }
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

    /// Like `compile_program`, but pre-seeded with stdlib function signatures and type aliases.
    /// Produces correct IR for user code that calls stdlib functions (correct return types, not Void).
    pub fn compile_program_with_externs(
        &mut self,
        program: &Program,
        fn_reg: &crate::high_level_language::stdlib::FunctionRegistry,
        ty_reg: &crate::high_level_language::stdlib::TypeRegistry,
    ) -> Result<IrProgram, CompilerError> {
        // Pre-seed function return types; these survive reset_for_program().
        // The first pass of compile_program will overwrite with user-defined types where names collide.
        for (name, sig) in &fn_reg.functions {
            self.function_return_types.insert(name.clone(), sig.return_type.clone());
        }
        self.extern_fn_returns = fn_reg
            .functions
            .iter()
            .map(|(k, v)| (k.clone(), v.return_type.clone()))
            .collect();
        self.extern_ty_aliases = ty_reg.aliases.clone();
        let result = self.compile_program(program);
        self.extern_fn_returns.clear();
        self.extern_ty_aliases.clear();
        result
    }
}

mod control_flow;
mod declarations;
mod expressions;
mod literals;
mod types;
mod utils;
