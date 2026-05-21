use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::assembler::{Assembler, AssemblerError};
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::AssembledOutput;
use hll_to_ir::{CompileConfig, Diagnostic, DiagnosticLevel, HllCompiler, IrProgram};
use ir_to_asm::compiler::compiler_rv64::CompilerRv64;

// ---------------------------------------------------------------------------
// Target mode (canonical definition lives in hll_to_ir)
// ---------------------------------------------------------------------------

pub use hll_to_ir::TargetMode;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CompilationError {
    DiagnosticErrors(Vec<Diagnostic>),
    /// Errors emitted by the entry-point validator.
    FreestandingErrors(Vec<String>),
}

impl std::fmt::Display for CompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DiagnosticErrors(diags) => {
                for d in diags {
                    writeln!(f, "{}", d.format_full())?;
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
    /// Debug-formatted token list (for the Tokens panel).
    pub tokens_display: String,
    /// Debug-formatted AST (for the AST panel).
    pub ast_display: String,
    pub ir_program: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// Stage-typed pipeline output
// ---------------------------------------------------------------------------

pub struct LexOutput {
    pub display: String,
}

pub struct ParseOutput {
    pub display: String,
}

pub struct IrOutput {
    pub display: String,
}

pub struct AsmOutput {
    pub tokens: Vec<RvInstruction>,
    pub display: String,
}

pub struct BinaryOutput {
    pub assembled: AssembledOutput,
}

pub struct ExecOutput {
    pub uart_output: String,
    pub exit_code: Option<i64>,
}

pub struct PipelineResult {
    pub diagnostics: Vec<Diagnostic>,
    pub lex: Option<LexOutput>,
    pub parse: Option<ParseOutput>,
    pub ir: Option<IrOutput>,
    pub asm: Option<AsmOutput>,
    pub binary: Option<BinaryOutput>,
    pub exec: Option<ExecOutput>,
}

impl PipelineResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.level == DiagnosticLevel::Error)
    }

    pub fn format_diagnostics(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.format_full())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Pipeline config
// ---------------------------------------------------------------------------

pub struct PipelineConfig {
    pub run_semantic_analysis: bool,
    pub strict_semantics: bool,
    pub target_mode: TargetMode,
    pub entry_point: Option<String>,
    pub link_layout: Option<LinkLayout>,
    pub string_prefix: Option<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            run_semantic_analysis: true,
            strict_semantics: false,
            target_mode: TargetMode::Hosted,
            entry_point: None,
            link_layout: None,
            string_prefix: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

pub struct CompilationPipeline {
    run_semantic_analysis: bool,
    strict_semantics: bool,
    target_mode: TargetMode,
    entry_point: Option<String>,
    link_layout: Option<LinkLayout>,
    string_prefix: Option<String>,
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

    pub fn from_config(config: PipelineConfig) -> Self {
        Self {
            run_semantic_analysis: config.run_semantic_analysis,
            strict_semantics: config.strict_semantics,
            target_mode: config.target_mode,
            entry_point: config.entry_point,
            link_layout: config.link_layout,
            string_prefix: config.string_prefix,
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

    pub fn target_mode(&self) -> TargetMode {
        self.target_mode
    }

    pub fn set_target_mode(&mut self, mode: TargetMode) {
        self.target_mode = mode;
    }

    pub fn set_entry_point(&mut self, entry: Option<String>) {
        self.entry_point = entry;
    }

    pub fn set_link_layout(&mut self, layout: Option<LinkLayout>) {
        self.link_layout = layout;
    }

    pub fn set_run_semantic_analysis(&mut self, enabled: bool) {
        self.run_semantic_analysis = enabled;
    }

    pub fn set_string_prefix(&mut self, prefix: Option<String>) {
        self.string_prefix = prefix;
    }

    /// source -> tokens -> AST -> IR, with mode-specific validation.
    pub fn compile(&self, source: &str) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline");

        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
        });

        let out = compiler
            .compile(source)
            .map_err(CompilationError::DiagnosticErrors)?;

        // Entry-point presence check for freestanding builds.
        if self.target_mode == TargetMode::Freestanding {
            let entry = self.effective_entry_point();
            let entry_defined = out.ir.functions.iter().any(|f| f.name == entry);
            if !entry_defined && entry != "_start" && entry != "main" {
                let mut warnings = out.diagnostics.clone();
                warnings.push(
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
                return Ok(CompilationResult {
                    tokens_display: out.tokens_display,
                    ast_display: out.ast_display,
                    ir_program: out.ir,
                    diagnostics: warnings,
                });
            }
        }

        log::info!("Compilation complete");
        Ok(CompilationResult {
            tokens_display: out.tokens_display,
            ast_display: out.ast_display,
            ir_program: out.ir,
            diagnostics: out.diagnostics,
        })
    }

    /// Run all pipeline stages, returning typed per-stage outputs.
    ///
    /// `stdlib_tokens` — when `Some`, prepended before the user's assembly tokens
    /// before assembling (the standard link mode for user programs).  Pass `None`
    /// when compiling stdlib or kernel sources standalone.
    pub fn run_full(
        &self,
        source: &str,
        stdlib_tokens: Option<&[RvInstruction]>,
    ) -> PipelineResult {
        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
        });

        let out = match compiler.compile(source) {
            Ok(out) => out,
            Err(diags) => {
                return PipelineResult {
                    diagnostics: diags,
                    lex: None,
                    parse: None,
                    ir: None,
                    asm: None,
                    binary: None,
                    exec: None,
                }
            }
        };

        let mut diagnostics = out.diagnostics;

        // Entry-point presence check for freestanding builds.
        if self.target_mode == TargetMode::Freestanding {
            let entry = self.effective_entry_point();
            if !out.ir.functions.iter().any(|f| f.name == entry)
                && entry != "_start"
                && entry != "main"
            {
                diagnostics.push(
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
        }

        let lex = Some(LexOutput { display: out.tokens_display });
        let parse = Some(ParseOutput { display: out.ast_display });
        let ir_display = out.ir.to_string();
        let ir = Some(IrOutput { display: ir_display });

        let (asm_text, user_tokens) = self.compile_ir_to_assembly_with_tokens(&out.ir);
        let asm = Some(AsmOutput { tokens: user_tokens.clone(), display: asm_text });

        let binary = {
            let mut all_tokens: Vec<RvInstruction> =
                stdlib_tokens.map(|s| s.to_vec()).unwrap_or_default();
            all_tokens.extend(user_tokens);
            self.assemble_linked(&all_tokens)
                .ok()
                .map(|assembled| BinaryOutput { assembled })
        };

        PipelineResult { diagnostics, lex, parse, ir, asm, binary, exec: None }
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
    ) -> (String, Vec<RvInstruction>) {
        let mut compiler = CompilerRv64::new();
        compiler.compile_with_tokens(ir)
    }

    /// Assemble a token stream into machine code.
    pub fn assemble(
        &self,
        tokens: &[RvInstruction],
    ) -> Result<AssembledOutput, AssemblerError> {
        Assembler::assemble(tokens)
    }

    /// Assemble and apply full link-time post-processing:
    /// - Inject layout boundary symbols (if `emit_layout_symbols` is set).
    /// - Mark the entry-point symbol as `.globl`.
    ///
    /// Use this instead of `assemble()` when you want ELF/binary images ready
    /// for boot or debugging.
    pub fn assemble_linked(
        &self,
        tokens: &[RvInstruction],
    ) -> Result<AssembledOutput, AssemblerError> {
        let mut out = Assembler::assemble(tokens)?;
        let layout = self.effective_link_layout();
        if layout.emit_layout_symbols {
            out.inject_layout_symbols(&layout);
        }
        out.mark_entry_global(self.effective_entry_point());
        Ok(out)
    }
}
