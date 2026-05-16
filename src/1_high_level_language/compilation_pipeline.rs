use crate::assembly_language::assembler::link_layout::LinkLayout;
use crate::high_level_language::ast::{Block, DeclNode, Program, Statement};
use crate::high_level_language::compiler::{
    CompilerError, Diagnostic, DiagnosticLevel, HighLevelCompiler, SemanticAnalyzer,
};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::{Parser, ParserError};
use crate::high_level_language::token::Token;
use crate::intermediate_language::IrProgram;
use crate::intermediate_language::asm_compiler::compiler_rv64::CompilerRv64;

// ---------------------------------------------------------------------------
// Target mode
// ---------------------------------------------------------------------------

/// Whether the output targets a hosted OS process or a bare-metal environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    /// Linux userspace, runtime.hll is linked, `_start` calls `main`.
    #[default]
    Hosted,
    /// Bare-metal / freestanding
    /// runtime_freestanding.hll is linked instead.
    Freestanding,
}

impl TargetMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hosted => "Hosted",
            Self::Freestanding => "Freestanding",
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CompilationError {
    LexerError(String),
    ParseError(ParserError),
    CompilerError(CompilerError),
    SemanticErrors(Vec<String>),
    /// Errors emitted by the freestanding validator.
    FreestandingErrors(Vec<String>),
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
            Self::FreestandingErrors(errors) => {
                writeln!(f, "Freestanding errors:")?;
                for error in errors {
                    writeln!(f, "  - {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CompilationError {}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CompilationResult {
    pub ast: Program,
    pub ir_program: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

pub struct CompilationPipeline {
    pub run_semantic_analysis: bool,
    pub strict_semantics: bool,
    /// Target environment.
    pub target_mode: TargetMode,
    /// Entry-point symbol to verify and use when building ELF images.
    /// `None` means "use the mode default" (`_start` for Hosted, `kmain` for
    /// Freestanding).
    pub entry_point: Option<String>,
    /// Memory layout for the output image.
    /// `None` uses the mode default (`LinkLayout::hosted()` or
    /// `LinkLayout::freestanding_kernel()`).
    pub link_layout: Option<LinkLayout>,
    /// Prefix for rodata string-literal labels produced by this compilation
    /// unit (e.g. `"str_"` → `str_0`, `str_1`, …).  When two units are linked
    /// together, give each a distinct prefix so the assembler never sees
    /// duplicate labels.  `None` uses the default `"str_"`.
    pub string_prefix: Option<String>,
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
            target_mode: TargetMode::Hosted,
            entry_point: None,
            link_layout: None,
            string_prefix: None,
        }
    }

    /// The effective link layout for the current configuration.
    pub fn effective_link_layout(&self) -> LinkLayout {
        if let Some(layout) = &self.link_layout {
            layout.clone()
        } else {
            match self.target_mode {
                TargetMode::Hosted => LinkLayout::hosted(),
                TargetMode::Freestanding => LinkLayout::freestanding_kernel(),
            }
        }
    }

    /// Effective load base address (shorthand for `effective_link_layout().load_base`).
    pub fn effective_load_base(&self) -> u64 {
        self.effective_link_layout().load_base
    }

    /// The effective entry-point symbol given the current configuration.
    pub fn effective_entry_point(&self) -> &str {
        if let Some(sym) = &self.entry_point {
            sym.as_str()
        } else {
            match self.target_mode {
                TargetMode::Hosted => "_start",
                TargetMode::Freestanding => "kmain",
            }
        }
    }

    /// source -> tokens -> AST -> IR, with mode-specific validation.
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
        let (ir_program, mut diagnostics) = self.compile_to_ir(&ast)?;
        log::info!("Compilation complete");

        // Phase 5: Freestanding validation
        if self.target_mode == TargetMode::Freestanding {
            log::info!("Phase 5: Running freestanding validation");
            let freestanding_diags = self.check_freestanding(&ast);

            let errors: Vec<String> = freestanding_diags
                .iter()
                .filter(|d| matches!(d.level, DiagnosticLevel::Error))
                .map(|d| d.format_full())
                .collect();

            if !errors.is_empty() {
                return Err(CompilationError::FreestandingErrors(errors));
            }

            // Warnings are surfaced as diagnostics in the result.
            diagnostics.extend(freestanding_diags);
        }

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
        let prefix = self.string_prefix.as_deref().unwrap_or("str_");
        let mut compiler = HighLevelCompiler::with_string_prefix(prefix);
        let ir_program = compiler
            .compile_program(ast)
            .map_err(CompilationError::CompilerError)?;

        let diagnostics = compiler.diagnostics().to_vec();

        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.level,
                    DiagnosticLevel::Error
                )
            })
            .map(|d| d.message.clone())
            .collect();

        if !errors.is_empty() {
            return Err(CompilationError::SemanticErrors(errors));
        }

        Ok((ir_program, diagnostics))
    }

    /// Compile and return only the IR program.
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

    /// Assemble a token stream into machine code.
    pub fn assemble(
        &self,
        tokens: &[crate::assembly_language::rv_instruction::RvInstruction],
    ) -> Result<
        crate::assembly_language::assembler::output::AssembledOutput,
        crate::assembly_language::assembler::AssemblerError,
    > {
        crate::assembly_language::assembler::Assembler::assemble(tokens)
    }

    /// Assemble and apply full link-time post-processing:
    /// - Inject layout boundary symbols (if `emit_layout_symbols` is set).
    /// - Mark the entry-point symbol as `.globl`.
    ///
    /// Use this instead of `assemble()` when you want ELF/binary images ready
    /// for boot or debugging.
    pub fn assemble_linked(
        &self,
        tokens: &[crate::assembly_language::rv_instruction::RvInstruction],
    ) -> Result<
        crate::assembly_language::assembler::output::AssembledOutput,
        crate::assembly_language::assembler::AssemblerError,
    > {
        let mut out = crate::assembly_language::assembler::Assembler::assemble(tokens)?;
        let layout = self.effective_link_layout();
        if layout.emit_layout_symbols {
            out.inject_layout_symbols(&layout);
        }
        out.mark_entry_global(self.effective_entry_point());
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Freestanding validator
// ---------------------------------------------------------------------------

/// Linux RV64 userspace syscall numbers that are invalid in freestanding mode.
/// These are the numbers from runtime.hll and close relatives.
const LINUX_USERSPACE_SYSCALLS: &[(u64, &str)] = &[
    (17, "sys_getcwd"),
    (23, "sys_dup"),
    (29, "sys_ioctl"),
    (34, "sys_mkdirat"),
    (35, "sys_unlinkat"),
    (48, "sys_faccessat"),
    (49, "sys_chdir"),
    (56, "sys_openat"),
    (57, "sys_close"),
    (62, "sys_lseek"),
    (63, "sys_read"),
    (64, "sys_write"),
    (65, "sys_readv"),
    (66, "sys_writev"),
    (80, "sys_fstat"),
    (93, "sys_exit"),
    (94, "sys_exit_group"),
    (96, "sys_set_tid_address"),
    (160, "sys_uname"),
    (172, "sys_getpid"),
    (214, "sys_brk"),
    (222, "sys_mmap"),
    (226, "sys_mprotect"),
];

impl CompilationPipeline {
    /// Walk the AST and emit diagnostics for hosted-only constructs that are
    /// unsafe or meaningless in freestanding mode.
    fn check_freestanding(&self, program: &Program) -> Vec<Diagnostic> {
        let mut diags: Vec<Diagnostic> = Vec::new();

        // Collect the names of all defined functions so we can validate the
        // configured entry point.
        let mut defined_fn_names: Vec<&str> = Vec::new();
        let mut has_external_main = false;

        for decl in &program.declarations {
            match &decl.decl {
                DeclNode::Function {
                    name,
                    is_extern,
                    body,
                    ..
                } => {
                    if *is_extern && name == "main" {
                        has_external_main = true;
                    }
                    if body.is_some() {
                        defined_fn_names.push(name.as_str());
                    }
                    if let Some(block) = body {
                        check_asm_in_block(block, &mut diags);
                    }
                }
                _ => {}
            }
        }

        // `external main` in freestanding mode signals an unconverted hosted
        // dependency (usually copied from a hosted program).
        if has_external_main {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    "`external main` declaration found in freestanding mode",
                )
                .with_note(
                    "freestanding builds do not call `main`; \
                     use a custom entry point (e.g., `kmain`) instead",
                ),
            );
        }

        // Verify that the configured entry point is actually defined, so the
        // user gets an early, actionable message instead of a silent wrong-entry ELF.
        let entry = self.effective_entry_point();
        let entry_defined = defined_fn_names.contains(&entry);

        // Also accept an asm-block defined label (we can't verify those here,
        // so only warn when no HLL function matches AND we are sure the entry
        // point matters).
        if !entry_defined && entry != "_start" && entry != "main" {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    format!(
                        "configured entry point `{entry}` is not defined as an HLL function"
                    ),
                )
                .with_note(
                    "if the entry point is defined via an `asm { }` label, \
                     this warning can be ignored",
                ),
            );
        }

        diags
    }
}

// ---------------------------------------------------------------------------
// ASM-block walker helpers (free functions to avoid borrow conflicts)
// ---------------------------------------------------------------------------

fn check_asm_in_block(block: &Block, diags: &mut Vec<Diagnostic>) {
    for stmt in &block.statements {
        check_asm_in_stmt(stmt, diags);
    }
}

fn check_asm_in_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match stmt {
        Statement::AsmBlock { lines } => check_asm_lines(lines, diags),
        Statement::Block(b) => check_asm_in_block(b, diags),
        Statement::If {
            then_block,
            else_branch,
            ..
        } => {
            check_asm_in_block(then_block, diags);
            if let Some(else_stmt) = else_branch {
                check_asm_in_stmt(else_stmt, diags);
            }
        }
        Statement::While { body, .. } => check_asm_in_block(body, diags),
        _ => {}
    }
}

/// Scan the lines of an `asm { }` block for Linux userspace syscall patterns.
///
/// We look for `li a7, <number>` (with optional whitespace / commas) where the
/// number matches a known Linux userspace syscall.  SBI ecalls use extension
/// IDs passed in `a7` as well, but those are typically large or negative values
/// that do not overlap the Linux syscall table checked here.
fn check_asm_lines(lines: &[String], diags: &mut Vec<Diagnostic>) {
    // Track whether this block will execute an ecall so we avoid false
    // positives on blocks that load a7 for other purposes.
    let has_ecall = lines
        .iter()
        .any(|l| l.split_whitespace().next() == Some("ecall"));

    if !has_ecall {
        return;
    }

    for line in lines {
        let trimmed = line.trim();
        // Match `li a7, <number>` or `li a7,<number>` (case-insensitive on mnemonic).
        if let Some(rest) = trimmed
            .to_lowercase()
            .strip_prefix("li")
            .and_then(|s| s.trim_start().strip_prefix("a7"))
            .and_then(|s| s.trim_start().strip_prefix(','))
        {
            let num_str = rest.trim();
            if let Ok(n) = num_str.parse::<u64>() {
                if let Some(&(_, name)) = LINUX_USERSPACE_SYSCALLS.iter().find(|&&(id, _)| id == n)
                {
                    diags.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            format!(
                                "asm block invokes Linux userspace syscall {n} ({name}) via ecall"
                            ),
                        )
                        .with_note(
                            "freestanding builds run without an OS; use MMIO or SBI ecalls \
                             (extension IDs ≥ 0x10) instead of Linux userspace syscall numbers",
                        ),
                    );
                }
            }
        }
    }
}
