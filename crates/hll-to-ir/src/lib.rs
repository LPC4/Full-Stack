pub(crate) mod ast;
pub(crate) mod compiler;
pub mod hll_compiler;
pub mod ir;
pub(crate) mod lexer;
pub(crate) mod parser;
pub mod stdlib;
pub(crate) mod token;

/// Whether the compiled output targets a hosted OS process or a bare-metal environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    /// Linux userspace - runtime.hll is linked, `_start` calls `main`.
    #[default]
    Hosted,
    /// Bare-metal / freestanding - freestanding runtime is linked instead.
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

// Re-export public surface: HllCompiler, IR types, and diagnostics.
pub use compiler::{Diagnostic, DiagnosticLevel};
pub use hll_compiler::{CompileConfig, HllCompiler, HllOutput};

// Re-export IR types so downstream crates (ir-to-asm, visualizer) can import them
// from hll_to_ir:: directly.
pub use ir::{
    FloatWidth, IntWidth, IrBlock, IrCastMode, IrCmpOp, IrFunction, IrGlobalString, IrGlobalVar,
    IrInstruction, IrLabel, IrMathOp, IrParam, IrProgram, IrRegister, IrTerminator, IrType,
    IrTypeAlias, IrUnaryOp, IrValue,
};
