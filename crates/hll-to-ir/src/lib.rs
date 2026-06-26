#![expect(
    clippy::branches_sharing_code,
    clippy::cast_possible_wrap,
    clippy::collapsible_match,
    clippy::iter_over_hash_type,
    clippy::let_underscore_untyped,
    clippy::manual_let_else,
    clippy::map_err_ignore,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::module_inception,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::nonminimal_bool,
    clippy::self_only_used_in_recursion,
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::unwrap_used,
    reason = "legacy compiler structure; keep new lint categories enforced while refactoring incrementally"
)]
#![cfg_attr(
    test,
    expect(
        clippy::print_stdout,
        clippy::ref_patterns,
        clippy::single_char_pattern,
        clippy::str_to_string,
        reason = "compiler unit tests use direct diagnostics and compact fixture assertions"
    )
)]

pub(crate) mod ast;
pub(crate) mod compiler;
pub(crate) mod conv;
pub mod hll_compiler;
pub mod imports;
pub mod ir;
pub(crate) mod lexer;
pub(crate) mod monomorphize;
pub(crate) mod parser;
pub mod stdlib;
pub(crate) mod token;

/// Whether the compiled output targets a hosted OS process, a bare-metal program, or a kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    /// Linux userspace - `_start` calls `main`, Linux syscalls via ecall.
    #[default]
    Hosted,
    /// Bare-metal / freestanding - freestanding runtime, no Linux syscalls.
    Freestanding,
    /// Supervisor-mode kernel - kernel stdlib linked, entry point is `_kernel_start`,
    /// VM boots via ROM `_start` (PMP + medeleg + mret into S-mode).
    Kernel,
}

impl TargetMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hosted => "Hosted",
            Self::Freestanding => "Freestanding",
            Self::Kernel => "Kernel",
        }
    }
}

// Re-export public surface: HllCompiler, IR types, and diagnostics.
pub use compiler::{Diagnostic, DiagnosticLevel, OptOptions, optimize as optimize_ir};
pub use hll_compiler::{CompileConfig, HllCompiler, HllOutput};

// Re-export IR types so downstream crates (ir-to-asm, visualizer) can import them
// from hll_to_ir:: directly.
pub use ir::{
    FloatWidth, IntWidth, IrBlock, IrCastMode, IrCmpOp, IrFunction, IrGlobalString, IrGlobalVar,
    IrInstruction, IrLabel, IrMathOp, IrParam, IrProgram, IrRegister, IrTerminator, IrType,
    IrTypeAlias, IrUnaryOp, IrValue,
};
