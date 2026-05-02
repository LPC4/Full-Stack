use crate::high_level_language::ast::Program;
use crate::high_level_language::compiler::{
    CompilerError, Diagnostic, HighLevelCompiler, SemanticAnalyzer,
};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::{Parser, ParserError};
use crate::high_level_language::token::Token;
use crate::intermediate_language::IrProgram;
use crate::intermediate_language::asm_compiler::compiler_rv64::CompilerRv64;

#[derive(Debug, Clone)]
pub enum CompilationError {
    LexerError(String),
    ParseError(ParserError),
    CompilerError(CompilerError),
    SemanticErrors(Vec<String>),
}

impl std::fmt::Display for CompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LexerError(msg) => write!(f, "Lexer error: {msg}"),
            Self::ParseError(err) => write!(f, "Parse error: {err}"),
            Self::CompilerError(err) => write!(f, "Compiler error: {err:?}"),
            Self::SemanticErrors(errors) => {
                writeln!(f, "Semantic errors:")?;
                for error in errors {
                    writeln!(f, "  - {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CompilationError {}

#[derive(Debug)]
pub struct CompilationResult {
    pub ast: Program,
    pub ir_program: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct CompilationPipeline {
    pub run_semantic_analysis: bool,
    pub strict_semantics: bool,
}

impl Default for CompilationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilationPipeline {
    pub fn new() -> Self {
        Self {
            run_semantic_analysis: true,
            strict_semantics: false,
        }
    }

    /// source → tokens → AST → IR
    pub fn compile(&self, source: &str) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline");

        // Phase 1: Lexing
        log::info!("Phase 1: Lexing source code");
        let tokens = self.lex_internal(source)?;
        log::info!("Lexed {} tokens", tokens.len());

        // Phase 2: Parsing
        log::info!("Phase 2: Parsing tokens to AST");
        let ast = self.parse(tokens)?;
        log::info!(
            "Parsed program with {} declarations",
            ast.declarations.len()
        );

        // Phase 3: Semantic Analysis
        if self.run_semantic_analysis {
            log::info!("Phase 3: Running semantic analysis");
            self.semantic_analysis(&ast)?;
        }

        // Phase 4: Compilation to IR
        log::info!("Phase 4: Compiling to intermediate representation");
        let (ir_program, diagnostics) = self.compile_to_ir(&ast)?;
        log::info!("Compilation complete");

        Ok(CompilationResult {
            ast,
            ir_program,
            diagnostics,
        })
    }

    fn lex_internal<'a>(
        &self,
        source: &'a str,
    ) -> Result<Vec<(Token<'a>, crate::high_level_language::token::Span)>, CompilationError> {
        let token_spans = Lexer::tokenize(source);
        if let Some((Token::Error(msg), _)) = token_spans
            .iter()
            .find(|(t, _)| matches!(t, Token::Error(_)))
        {
            return Err(CompilationError::LexerError(msg.clone()));
        }
        Ok(token_spans)
    }

    pub fn parse(
        &self,
        token_spans: Vec<(Token<'_>, crate::high_level_language::token::Span)>,
    ) -> Result<Program, CompilationError> {
        let mut parser = Parser::new_with_spans(token_spans);
        parser.parse_program().map_err(CompilationError::ParseError)
    }

    pub fn semantic_analysis(&self, ast: &Program) -> Result<(), CompilationError> {
        let mut semantic_analyzer = SemanticAnalyzer::new();

        if let Ok(_) = semantic_analyzer.analyze_program(ast) {
            // Check if there are any errors in diagnostics
            let errors: Vec<_> = semantic_analyzer
                .diagnostics()
                .iter()
                .filter(|d| {
                    matches!(
                        d.level,
                        crate::high_level_language::compiler::DiagnosticLevel::Error
                    )
                })
                .map(|d| d.message.clone())
                .collect();

            if !errors.is_empty() {
                return Err(CompilationError::SemanticErrors(errors));
            }
            Ok(())
        } else {
            // Semantic analysis failed completely
            let errors: Vec<_> = semantic_analyzer
                .diagnostics()
                .iter()
                .map(|d| d.message.clone())
                .collect();
            Err(CompilationError::SemanticErrors(errors))
        }
    }

    pub fn compile_to_ir(
        &self,
        ast: &Program,
    ) -> Result<(IrProgram, Vec<Diagnostic>), CompilationError> {
        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(ast)
            .map_err(CompilationError::CompilerError)?;

        let diagnostics = compiler.diagnostics().to_vec();

        // Check for semantic errors in compiler diagnostics
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.level,
                    crate::high_level_language::compiler::DiagnosticLevel::Error
                )
            })
            .map(|d| d.message.clone())
            .collect();

        if !errors.is_empty() {
            return Err(CompilationError::SemanticErrors(errors));
        }

        Ok((ir_program, diagnostics))
    }

    /// Compile and return only the IR program
    pub fn compile_to_ir_only(&self, source: &str) -> Result<IrProgram, CompilationError> {
        let result = self.compile(source)?;
        Ok(result.ir_program)
    }

    /// Compile an IR program to RISC‑V assembly text.
    pub fn compile_ir_to_assembly(&self, ir: &IrProgram) -> String {
        let mut compiler = CompilerRv64::new();
        compiler.compile(ir)
    }

    /// Compile an IR program to assembly text and the structured token stream.
    pub fn compile_ir_to_assembly_with_tokens(
        &self,
        ir: &IrProgram,
    ) -> (
        String,
        Vec<crate::assembly_language::rv_instruction::RvInstruction>,
    ) {
        let mut compiler = CompilerRv64::new();
        compiler.compile_with_tokens(ir)
    }

    /// Assemble a token stream into machine code, producing one byte blob per section.
    ///
    /// Stubs for common libc symbols (putchar, printf, puts, malloc, free, exit) are
    /// appended automatically so external calls resolve without needing a real libc.
    pub fn assemble(
        &self,
        tokens: &[crate::assembly_language::rv_instruction::RvInstruction],
    ) -> Result<
        crate::assembly_language::assembler::output::AssembledOutput,
        crate::assembly_language::assembler::AssemblerError,
    > {
        let mut all: Vec<crate::assembly_language::rv_instruction::RvInstruction> =
            tokens.to_vec();
        all.extend(extern_stubs());
        crate::assembly_language::assembler::Assembler::assemble(&all)
    }
}

// ---------------------------------------------------------------------------
// Libc stub injection
// ---------------------------------------------------------------------------

/// Build the syscall-based stubs for common external symbols.
///
/// Each stub is three instructions:
///   addi a7, x0, <syscall_no>   // set syscall ID
///   ecall                       // handled by the VM
///   jalr x0, x1, 0              // ret — return to caller
///
/// The VM's ecall handler recognises these custom syscall numbers and
/// implements the corresponding behaviour (I/O, malloc, etc.).
fn extern_stubs() -> Vec<crate::assembly_language::rv_instruction::RvInstruction> {
    use crate::assembly_language::real::RealInstruction;
    use crate::assembly_language::rv_instruction::RvInstruction;
    use crate::assembly_language::riscv::rv64i::{Addi, Ecall, Jalr};

    // (symbol name, syscall number)
    const STUBS: &[(&str, i32)] = &[
        ("putchar", 1000),
        ("puts",    1001),
        ("printf",  1002),
        ("malloc",  1003),
        ("free",    1004),
        ("exit",    93),
    ];

    let mut tokens = Vec::new();
    // Switch back to .text so stubs land in the code section regardless of
    // what section the user's assembly ended in.
    tokens.push(RvInstruction::Directive(".text".to_owned()));
    for &(name, syscall_no) in STUBS {
        tokens.push(RvInstruction::Label(name.to_owned()));
        tokens.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(17, 0, syscall_no))));
        tokens.push(RvInstruction::Real(RealInstruction::Ecall(Ecall)));
        tokens.push(RvInstruction::Real(RealInstruction::Jalr(Jalr::new(0, 1, 0))));
    }
    tokens
}
