use crate::high_level_language::ast::Program;
use crate::high_level_language::compiler::{
    CompilerError, Diagnostic, DiagnosticLevel, HighLevelCompiler, SemanticAnalyzer,
};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::{Parser, ParserError};
use crate::high_level_language::stdlib::{FunctionRegistry, TypeRegistry};
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

    /// source -> tokens -> AST -> IR
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

    /// Compile an IR program to RISC-V assembly text.
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

    /// Like `compile`, but pre-seeded with stdlib registries so user calls to stdlib
    /// functions produce correct IR (proper return types, not Void).
    pub fn compile_with_externs(
        &self,
        source: &str,
        fn_reg: &FunctionRegistry,
        ty_reg: &TypeRegistry,
    ) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline with externs");
        let tokens = self.lex_internal(source)?;
        let ast = self.parse(tokens)?;
        if self.run_semantic_analysis {
            self.semantic_analysis_with_externs(&ast, fn_reg, ty_reg)?;
        }
        let (ir_program, diagnostics) = self.compile_to_ir_with_externs(&ast, fn_reg, ty_reg)?;
        Ok(CompilationResult { ast, ir_program, diagnostics })
    }

    fn semantic_analysis_with_externs(
        &self,
        ast: &Program,
        fn_reg: &FunctionRegistry,
        ty_reg: &TypeRegistry,
    ) -> Result<(), CompilationError> {
        let mut analyzer = SemanticAnalyzer::new();
        let returns: std::collections::HashMap<String, crate::intermediate_language::IrType> =
            fn_reg.functions.iter().map(|(k, v)| (k.clone(), v.return_type.clone())).collect();
        analyzer.seed_extern_fn_returns(&returns);
        analyzer.seed_extern_type_aliases(&ty_reg.aliases);

        if let Ok(_) = analyzer.analyze_program(ast) {
            let errors: Vec<_> = analyzer
                .diagnostics()
                .iter()
                .filter(|d| matches!(d.level, DiagnosticLevel::Error))
                .map(|d| d.message.clone())
                .collect();
            if !errors.is_empty() {
                return Err(CompilationError::SemanticErrors(errors));
            }
            Ok(())
        } else {
            let errors: Vec<_> =
                analyzer.diagnostics().iter().map(|d| d.message.clone()).collect();
            Err(CompilationError::SemanticErrors(errors))
        }
    }

    fn compile_to_ir_with_externs(
        &self,
        ast: &Program,
        fn_reg: &FunctionRegistry,
        ty_reg: &TypeRegistry,
    ) -> Result<(IrProgram, Vec<Diagnostic>), CompilationError> {
        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program_with_externs(ast, fn_reg, ty_reg)
            .map_err(CompilationError::CompilerError)?;
        let diagnostics = compiler.diagnostics().to_vec();
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.level, DiagnosticLevel::Error))
            .map(|d| d.message.clone())
            .collect();
        if !errors.is_empty() {
            return Err(CompilationError::SemanticErrors(errors));
        }
        Ok((ir_program, diagnostics))
    }

    /// Assemble a token stream into machine code, producing one byte blob per section.
    ///
    /// Stubs for common runtime symbols are appended automatically so external
    /// calls resolve without needing a real libc. Heap primitives use internal
    /// raw names so the public stdlib can provide `malloc` and `free`.
    pub fn assemble(
        &self,
        tokens: &[crate::assembly_language::rv_instruction::RvInstruction],
    ) -> Result<
        crate::assembly_language::assembler::output::AssembledOutput,
        crate::assembly_language::assembler::AssemblerError,
    > {
        let mut all: Vec<crate::assembly_language::rv_instruction::RvInstruction> = tokens.to_vec();
        all.extend(extern_stubs());
        crate::assembly_language::assembler::Assembler::assemble(&all)
    }
}

// ---------------------------------------------------------------------------
// Runtime stub injection
// ---------------------------------------------------------------------------

/// Build real function implementations for common runtime symbols.
///
/// All I/O uses Linux syscall 64 (sys_write), so programs run unmodified
/// on both the internal VM and real Linux (via QEMU or native RISC-V).
///
/// Functions provided:
///   putchar(a0 = char) -> a0  — writes one byte to stdout
///   puts(a0 = ptr) -> 0       — writes null-terminated string + newline
///   printf(a0 = fmt) -> 0     — writes format string (no substitution)
///   exit(a0 = code)           — terminates via sys_exit (syscall 93)
fn extern_stubs() -> Vec<crate::assembly_language::rv_instruction::RvInstruction> {
    use crate::assembly_language::pseudo::PseudoInstruction;
    use crate::assembly_language::real::RealInstruction;
    use crate::assembly_language::riscv::rv64i::{Add, Addi, Ecall, Jalr, Lbu, Ld, Sb, Sd};
    use crate::assembly_language::riscv::rv64m::{Divu, Remu};
    use crate::assembly_language::rv_instruction::RvInstruction;

    // Register numbers (RISC-V ABI)
    const RA: u8 = 1;
    const SP: u8 = 2;
    const S0: u8 = 8;
    const S1: u8 = 9;
    const A0: u8 = 10;
    const A1: u8 = 11;
    const A2: u8 = 12;
    const A7: u8 = 17;

    let mut t: Vec<RvInstruction> = Vec::new();

    // Switch to .text so stubs land in the code section
    t.push(RvInstruction::Directive(".text".to_owned()));

    // ---- putchar(a0 = char) -> a0 ----
    // sys_write(fd=1, buf=&char_on_stack, len=1) via syscall 64
    t.push(RvInstruction::Label("putchar".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, -16,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(SP, RA, 8))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(SP, A0, 0))));
    t.push(RvInstruction::Real(RealInstruction::Sb(Sb::new(SP, A0, 7))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A0,
        imm: 1,
    }));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        A1, SP, 7,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A2,
        imm: 1,
    }));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A7,
        imm: 64,
    }));
    t.push(RvInstruction::Real(RealInstruction::Ecall(Ecall)));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(A0, SP, 0))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(RA, SP, 8))));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, 16,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Ret));

    // ---- puts(a0 = ptr) -> 0 ----
    // Calls putchar for each byte then putchar('\n')
    t.push(RvInstruction::Label("puts".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, -16,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(SP, RA, 8))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(SP, S0, 0))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Mv {
        rd: S0,
        rs: A0,
    }));
    t.push(RvInstruction::Label("__puts_loop".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Lbu(Lbu::new(
        A0, S0, 0,
    ))));
    t.push(RvInstruction::Directive(
        "\tbeq a0, x0, __puts_done".to_owned(),
    ));
    t.push(RvInstruction::Directive("\tcall putchar".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, 1,
    ))));
    t.push(RvInstruction::Directive("\tj __puts_loop".to_owned()));
    t.push(RvInstruction::Label("__puts_done".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A0,
        imm: 10,
    })); // '\n'
    t.push(RvInstruction::Directive("\tcall putchar".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A0,
        imm: 0,
    }));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(S0, SP, 0))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(RA, SP, 8))));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, 16,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Ret));

    // ---- print_int(a0 = i64) ----
    // Prints a0 as a signed decimal integer by collecting digits on the stack
    // and then emitting them in reverse order via putchar.
    //
    // Stack frame (-48):  40(sp)=ra  32(sp)=s0  24(sp)=s1
    //                     0..19(sp) = digit buffer (up to 20 digits for i64)
    t.push(RvInstruction::Label("print_int".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, -48,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(
        SP, RA, 40,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(
        SP, S0, 32,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(
        SP, S1, 24,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: S0,
        imm: 0,
    })); // digit count
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: S1,
        imm: 0,
    })); // sign flag
    // if a0 >= 0, skip negation
    t.push(RvInstruction::Directive(
        "\tbge a0, x0, __pi_pos".to_owned(),
    ));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: S1,
        imm: 1,
    }));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Neg {
        rd: A0,
        rs: A0,
    }));
    t.push(RvInstruction::Label("__pi_pos".to_owned()));
    t.push(RvInstruction::Directive(
        "\tbeq a0, x0, __pi_zero".to_owned(),
    ));
    // Digit extraction loop: collect a0 % 10, then a0 /= 10
    t.push(RvInstruction::Label("__pi_digit_loop".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: 5,
        imm: 10,
    })); // t0 = 10
    t.push(RvInstruction::Real(RealInstruction::Remu(Remu::new(
        6, A0, 5,
    )))); // t1 = a0 % 10
    t.push(RvInstruction::Real(RealInstruction::Divu(Divu::new(
        A0, A0, 5,
    )))); // a0 = a0 / 10
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        6, 6, 48,
    )))); // t1 += '0'
    t.push(RvInstruction::Real(RealInstruction::Add(Add::new(
        7, SP, S0,
    )))); // t2 = sp + count
    t.push(RvInstruction::Real(RealInstruction::Sb(Sb::new(7, 6, 0)))); // mem[t2] = t1
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, 1,
    ))));
    t.push(RvInstruction::Directive(
        "\tbne a0, x0, __pi_digit_loop".to_owned(),
    ));
    t.push(RvInstruction::Directive("\tj __pi_output".to_owned()));
    t.push(RvInstruction::Label("__pi_zero".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: 5,
        imm: 48,
    })); // '0'
    t.push(RvInstruction::Real(RealInstruction::Sb(Sb::new(SP, 5, 0))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: S0,
        imm: 1,
    }));
    t.push(RvInstruction::Label("__pi_output".to_owned()));
    // print '-' if negative
    t.push(RvInstruction::Directive(
        "\tbeq s1, x0, __pi_print".to_owned(),
    ));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A0,
        imm: 45,
    })); // '-'
    t.push(RvInstruction::Directive("\tcall putchar".to_owned()));
    // print digits in reverse (last stored is most significant)
    t.push(RvInstruction::Label("__pi_print".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, -1,
    ))));
    t.push(RvInstruction::Label("__pi_loop".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Add(Add::new(
        5, SP, S0,
    )))); // t0 = sp + s0
    t.push(RvInstruction::Real(RealInstruction::Lbu(Lbu::new(
        A0, 5, 0,
    ))));
    t.push(RvInstruction::Directive("\tcall putchar".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, -1,
    ))));
    t.push(RvInstruction::Directive(
        "\tbge s0, x0, __pi_loop".to_owned(),
    ));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(
        S1, SP, 24,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(
        S0, SP, 32,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(
        RA, SP, 40,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, 48,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Ret));

    // ---- printf(a0 = fmt, a1 = int_arg) -> 0 ----
    // Scans the format string; on encountering "%d" prints the integer in a1
    // via print_int; all other chars are passed through putchar.
    //
    // Stack frame (-32): 24(sp)=ra  16(sp)=s0(fmt ptr)  8(sp)=s1(int arg)
    t.push(RvInstruction::Label("printf".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, -32,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(
        SP, RA, 24,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(
        SP, S0, 16,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Sd(Sd::new(SP, S1, 8))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Mv {
        rd: S0,
        rs: A0,
    })); // s0 = fmt ptr
    t.push(RvInstruction::Pseudo(PseudoInstruction::Mv {
        rd: S1,
        rs: A1,
    })); // s1 = int arg
    t.push(RvInstruction::Label("__printf_loop".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Lbu(Lbu::new(
        A0, S0, 0,
    ))));
    t.push(RvInstruction::Directive(
        "\tbeq a0, x0, __printf_done".to_owned(),
    ));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: 5,
        imm: 37,
    })); // '%'
    t.push(RvInstruction::Directive(
        "\tbne a0, t0, __printf_char".to_owned(),
    ));
    // Peek at the character after '%'
    t.push(RvInstruction::Real(RealInstruction::Lbu(Lbu::new(
        6, S0, 1,
    )))); // t1 = s0[1]
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: 5,
        imm: 100,
    })); // 'd'
    t.push(RvInstruction::Directive(
        "\tbne t1, t0, __printf_char".to_owned(),
    ));
    // It's %d: advance past "%d", print integer
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, 2,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Mv {
        rd: A0,
        rs: S1,
    }));
    t.push(RvInstruction::Directive("\tcall print_int".to_owned()));
    t.push(RvInstruction::Directive("\tj __printf_loop".to_owned()));
    t.push(RvInstruction::Label("__printf_char".to_owned()));
    t.push(RvInstruction::Directive("\tcall putchar".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        S0, S0, 1,
    ))));
    t.push(RvInstruction::Directive("\tj __printf_loop".to_owned()));
    t.push(RvInstruction::Label("__printf_done".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A0,
        imm: 0,
    }));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(S1, SP, 8))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(
        S0, SP, 16,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Ld(Ld::new(
        RA, SP, 24,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        SP, SP, 32,
    ))));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Ret));

    // ---- exit(a0 = code) ----
    t.push(RvInstruction::Label("exit".to_owned()));
    t.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
        A7, 0, 93,
    ))));
    t.push(RvInstruction::Real(RealInstruction::Ecall(Ecall)));
    t.push(RvInstruction::Real(RealInstruction::Jalr(Jalr::new(
        0, RA, 0,
    ))));

    // ---- _start: kernel/QEMU entry point ----
    // Placed in .text so no separate PT_LOAD stub is needed (avoiding BSS-induced
    // p_vaddr/p_offset misalignment that breaks qemu-user segment mapping).
    t.push(RvInstruction::Directive(".globl _start".to_owned()));
    t.push(RvInstruction::Label("_start".to_owned()));
    t.push(RvInstruction::Directive("\tcall main".to_owned()));
    t.push(RvInstruction::Pseudo(PseudoInstruction::Li {
        rd: A7,
        imm: 93,
    }));
    t.push(RvInstruction::Real(RealInstruction::Ecall(Ecall)));

    t
}
