use super::{
    AssembledOutput, AssemblerError, CodegenError, Diagnostic, DiagnosticLevel, IrProgram,
    LinkLayout, LinkerError, RvInstruction, TargetMode,
};

// --- Errors ---

#[derive(Debug, Clone)]
pub enum CompilationError {
    DiagnosticErrors(Vec<Diagnostic>),
    /// Errors emitted by the entry-point validator.
    FreestandingErrors(Vec<String>),
    Codegen(CodegenError),
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
            Self::Codegen(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CompilationError {}

// --- Result ---

#[derive(Debug)]
pub struct CompilationResult {
    pub ir_program: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
}

// --- Stage-typed pipeline output ---

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

#[derive(Debug, Clone)]
pub enum PipelineFailure {
    Codegen(CodegenError),
    Assembler(AssemblerError),
    Linker(LinkerError),
    Build(String),
}

impl std::fmt::Display for PipelineFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codegen(error) => write!(f, "{error}"),
            Self::Assembler(error) => write!(f, "{error}"),
            Self::Linker(error) => write!(f, "{error}"),
            Self::Build(error) => write!(f, "{error}"),
        }
    }
}

pub struct PipelineResult {
    pub diagnostics: Vec<Diagnostic>,
    pub lex: Option<LexOutput>,
    pub parse: Option<ParseOutput>,
    pub ir: Option<IrOutput>,
    pub asm: Option<AsmOutput>,
    pub binary: Option<BinaryOutput>,
    pub failure: Option<PipelineFailure>,
}

impl PipelineResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error)
    }

    pub fn has_failure(&self) -> bool {
        self.failure.is_some()
    }

    pub fn format_diagnostics(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.format_full())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// --- Pipeline config ---

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
