use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::assembler::{Assembler, AssemblerError};
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::{AssembledOutput, LinkerError, ObjectLinker};
pub use hll_to_ir::TargetMode;
use hll_to_ir::{CompileConfig, Diagnostic, DiagnosticLevel, HllCompiler, IrProgram, IrType};
use ir_to_asm::compiler::compiler_rv64::CompilerRv64;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash as _, Hasher as _};
use std::panic::Location;
use std::path::{Path, PathBuf};

// Compilation pipeline: HLL source -> IR -> ASM -> OBJ -> ELF.
//
// Stages:
//   1. HLL -> IR   (compile / compile_to_ir_only / run_full)   -> out/ir/*.ir
//   2. IR  -> ASM  (compile_ir_to_assembly)                    -> out/asm/*.s
//   3. ASM -> OBJ  (assemble / assemble_named)                 -> out/obj/*.o
//   4. OBJ -> ELF  (link_assembled_objects)                    -> out/elf/total_*.elf
//
// Each HLL file compiles to its own .o, so no source concatenation happens before
// assembly.  Object files are linked together with full relocation support.

// --- Errors ---

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

// --- Result ---

#[derive(Debug)]
pub struct CompilationResult {
    pub tokens_display: String,
    pub ast_display: String,
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
    pub assembler_error: Option<String>,
    pub exec: Option<ExecOutput>,
}

impl PipelineResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error)
    }

    pub fn has_assembler_error(&self) -> bool {
        self.assembler_error.is_some()
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

// --- Pipeline ---

pub struct CompilationPipeline {
    run_semantic_analysis: bool,
    target_mode: TargetMode,
    entry_point: Option<String>,
    link_layout: Option<LinkLayout>,
    string_prefix: Option<String>,
    type_prelude: Vec<(String, IrType)>,
    artifact_root: PathBuf,
    artifact_stem: RefCell<Option<String>>,
    last_artifact_stem: RefCell<Option<String>>,
    write_artifacts: bool,
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
            target_mode: TargetMode::Hosted,
            entry_point: None,
            link_layout: None,
            string_prefix: None,
            type_prelude: Vec::new(),
            artifact_root: PathBuf::from("out"),
            artifact_stem: RefCell::new(None),
            last_artifact_stem: RefCell::new(None),
            write_artifacts: true,
        }
    }

    pub fn from_config(config: PipelineConfig) -> Self {
        Self {
            run_semantic_analysis: config.run_semantic_analysis,
            target_mode: config.target_mode,
            entry_point: config.entry_point,
            link_layout: config.link_layout,
            string_prefix: config.string_prefix,
            type_prelude: Vec::new(),
            artifact_root: PathBuf::from("out"),
            artifact_stem: RefCell::new(None),
            last_artifact_stem: RefCell::new(None),
            write_artifacts: true,
        }
    }

    /// The effective link layout for the current configuration.
    pub fn effective_link_layout(&self) -> LinkLayout {
        if let Some(layout) = &self.link_layout {
            layout.clone()
        } else {
            match self.target_mode {
                TargetMode::Hosted => LinkLayout::hosted(),
                TargetMode::Freestanding | TargetMode::Kernel => LinkLayout::freestanding_kernel(),
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
                TargetMode::Kernel => "_kernel_start",
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

    pub fn set_type_prelude(&mut self, types: Vec<(String, IrType)>) {
        self.type_prelude = types;
    }

    pub fn set_artifact_root(&mut self, root: impl Into<PathBuf>) {
        self.artifact_root = root.into();
    }

    pub fn set_write_artifacts(&mut self, enabled: bool) {
        self.write_artifacts = enabled;
    }

    pub fn set_artifact_stem(&mut self, stem: Option<String>) {
        *self.artifact_stem.get_mut() = stem.map(|s| sanitize_artifact_component(&s));
    }

    fn current_artifact_stem(&self) -> Option<String> {
        self.artifact_stem
            .borrow()
            .clone()
            .or_else(|| self.last_artifact_stem.borrow().clone())
    }

    fn default_artifact_stem(
        &self,
        caller: &'static Location<'static>,
        hint: &str,
        seed: &str,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        caller.file().hash(&mut hasher);
        caller.line().hash(&mut hasher);
        caller.column().hash(&mut hasher);
        hint.hash(&mut hasher);
        seed.hash(&mut hasher);
        let file_stem = Path::new(caller.file())
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("artifact");
        format!("{file_stem}_{}_{}", caller.line(), hasher.finish())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_text_artifact(&self, subdir: &str, stem: &str, extension: &str, content: &str) {
        if !self.write_artifacts {
            return;
        }
        let path = self
            .artifact_root
            .join(subdir)
            .join(format!("{stem}{extension}"));
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!("failed to create artifact directory `{parent:?}`: {err}");
            return;
        }
        if let Err(err) = fs::write(&path, content) {
            log::warn!("failed to write artifact `{path:?}`: {err}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_bytes_artifact(&self, subdir: &str, stem: &str, extension: &str, bytes: &[u8]) {
        if !self.write_artifacts {
            return;
        }
        let path = self
            .artifact_root
            .join(subdir)
            .join(format!("{stem}{extension}"));
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!("failed to create artifact directory `{parent:?}`: {err}");
            return;
        }
        if let Err(err) = fs::write(&path, bytes) {
            log::warn!("failed to write artifact `{path:?}`: {err}");
        }
    }

    /// Source -> tokens -> AST -> IR, with mode-specific validation.
    #[track_caller]
    pub fn compile(&self, source: &str) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline");

        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
            type_prelude: self.type_prelude.clone(),
        });

        let out = compiler
            .compile(source)
            .map_err(CompilationError::DiagnosticErrors)?;

        // Entry-point presence check for freestanding builds.
        // Kernel mode skips this: `_kernel_start` is provided by the kernel stdlib, not user code.
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

        let stem = self
            .artifact_stem
            .borrow()
            .clone()
            .unwrap_or_else(|| self.default_artifact_stem(Location::caller(), "ir", source));
        *self.last_artifact_stem.borrow_mut() = Some(sanitize_artifact_component(&stem));
        #[cfg(not(target_arch = "wasm32"))]
        self.write_text_artifact("ir", &stem, ".ir", &out.ir.to_string());

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
    /// `stdlib_tokens` - when `Some`, prepended before the user's assembly tokens
    /// before assembling (the standard link mode for user programs).  Pass `None`
    /// when compiling stdlib or kernel sources standalone.
    #[track_caller]
    pub fn run_full(
        &self,
        source: &str,
        stdlib_tokens: Option<&[RvInstruction]>,
    ) -> PipelineResult {
        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
            type_prelude: self.type_prelude.clone(),
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
                    assembler_error: None,
                    exec: None,
                };
            }
        };

        let mut diagnostics = out.diagnostics;

        // Entry-point presence check for freestanding builds.
        // Kernel mode skips this: `_kernel_start` is provided by the kernel stdlib, not user code.
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

        let lex = Some(LexOutput {
            display: out.tokens_display,
        });
        let parse = Some(ParseOutput {
            display: out.ast_display,
        });
        let ir_display = out.ir.to_string();

        // Write IR to disk
        let ir_stem =
            self.artifact_stem.borrow().clone().unwrap_or_else(|| {
                self.default_artifact_stem(Location::caller(), "ir", &ir_display)
            });
        *self.last_artifact_stem.borrow_mut() = Some(sanitize_artifact_component(&ir_stem));
        #[cfg(not(target_arch = "wasm32"))]
        self.write_text_artifact("ir", &ir_stem, ".ir", &ir_display);

        let ir = Some(IrOutput {
            display: ir_display,
        });

        let (asm_text, user_tokens) = self.compile_ir_to_assembly_with_tokens(&out.ir);
        let asm = Some(AsmOutput {
            tokens: user_tokens.clone(),
            display: asm_text,
        });

        let user_stem = self.current_artifact_stem().unwrap_or_else(|| {
            self.default_artifact_stem(Location::caller(), "user", &out.ir.to_string())
        });

        let user_obj = match self.assemble_named(&user_stem, &user_tokens) {
            Ok(obj) => obj,
            Err(e) => {
                return PipelineResult {
                    diagnostics,
                    lex,
                    parse,
                    ir,
                    asm,
                    binary: None,
                    assembler_error: Some(format!("assembler error: {}", e.message)),
                    exec: None,
                };
            }
        };

        let stdlib_obj = match stdlib_tokens {
            Some(tokens) => {
                let stdlib_stem = match self.target_mode {
                    TargetMode::Kernel => "kernel_stdlib".to_owned(),
                    _ => "stdlib".to_owned(),
                };
                match self.assemble_named(&stdlib_stem, tokens) {
                    Ok(obj) => Some(obj),
                    Err(e) => {
                        return PipelineResult {
                            diagnostics,
                            lex,
                            parse,
                            ir,
                            asm,
                            binary: None,
                            assembler_error: Some(format!("assembler error: {}", e.message)),
                            exec: None,
                        };
                    }
                }
            }
            None => None,
        };

        let mut modules: Vec<(&str, &AssembledOutput)> = vec![("user", &user_obj)];
        if let Some(ref stdlib) = stdlib_obj {
            modules.insert(0, ("stdlib", stdlib));
        }

        let (binary, assembler_error) =
            match self.link_assembled_objects_named(&user_stem, &modules) {
                Ok(assembled) => (Some(BinaryOutput { assembled }), None),
                Err(e) => (None, Some(format!("linker error: {}", e.message))),
            };

        PipelineResult {
            diagnostics,
            lex,
            parse,
            ir,
            asm,
            binary,
            assembler_error,
            exec: None,
        }
    }

    /// Compile and return only the IR program.
    pub fn compile_to_ir_only(&self, source: &str) -> Result<IrProgram, CompilationError> {
        let result = self.compile(source)?;
        Ok(result.ir_program)
    }

    /// Compile an IR program to RISC-V assembly text.
    #[track_caller]
    pub fn compile_ir_to_assembly(&self, ir: &IrProgram) -> String {
        let (asm, _) = self.compile_ir_to_assembly_with_tokens(ir);
        asm
    }

    /// Compile an IR program to assembly text and the structured token stream.
    #[track_caller]
    pub fn compile_ir_to_assembly_with_tokens(
        &self,
        ir: &IrProgram,
    ) -> (String, Vec<RvInstruction>) {
        let mut compiler = CompilerRv64::new();
        let (asm, tokens) = compiler.compile_with_tokens(ir);
        let stem = if let Some(existing) = self.current_artifact_stem() {
            existing
        } else {
            let stem = self.default_artifact_stem(Location::caller(), "asm", &asm);
            *self.last_artifact_stem.borrow_mut() = Some(stem.clone());
            stem
        };
        #[cfg(not(target_arch = "wasm32"))]
        self.write_text_artifact("asm", &stem, ".s", &asm);
        (asm, tokens)
    }

    /// Assemble a token stream into machine code.
    #[track_caller]
    pub fn assemble(&self, tokens: &[RvInstruction]) -> Result<AssembledOutput, AssemblerError> {
        let stem = self.current_artifact_stem().unwrap_or_else(|| {
            self.default_artifact_stem(Location::caller(), "obj", &format!("{}", tokens.len()))
        });
        self.assemble_named(&stem, tokens)
    }

    #[track_caller]
    pub fn assemble_named(
        &self,
        stem: &str,
        tokens: &[RvInstruction],
    ) -> Result<AssembledOutput, AssemblerError> {
        let assembled = Assembler::assemble(tokens)?;
        let stem = sanitize_artifact_component(stem);
        *self.last_artifact_stem.borrow_mut() = Some(stem.clone());
        #[cfg(not(target_arch = "wasm32"))]
        self.write_bytes_artifact(
            "obj",
            &stem,
            ".o",
            &assembled.to_object(&format!("{stem}.o")),
        );
        Ok(assembled)
    }

    /// Link already-assembled objects into a single executable image and apply
    /// layout post-processing (boundary symbols + global entry export).
    #[track_caller]
    pub fn link_assembled_objects(
        &self,
        modules: &[(&str, &AssembledOutput)],
    ) -> Result<AssembledOutput, LinkerError> {
        let stem = self.current_artifact_stem().unwrap_or_else(|| {
            self.default_artifact_stem(
                Location::caller(),
                "elf",
                &modules
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join("+"),
            )
        });
        self.link_assembled_objects_named(&stem, modules)
    }

    #[track_caller]
    pub fn link_assembled_objects_named(
        &self,
        stem: &str,
        modules: &[(&str, &AssembledOutput)],
    ) -> Result<AssembledOutput, LinkerError> {
        let mut out = ObjectLinker::link(modules)?;
        let layout = self.effective_link_layout();
        if layout.emit_layout_symbols {
            out.inject_layout_symbols(&layout);
        }
        out.mark_entry_global(self.effective_entry_point());
        let stem = sanitize_artifact_component(stem);
        *self.last_artifact_stem.borrow_mut() = Some(stem.clone());
        #[cfg(not(target_arch = "wasm32"))]
        self.write_bytes_artifact(
            "elf",
            &format!("total_{stem}"),
            ".elf",
            &out.to_elf_with_entry(self.effective_load_base(), self.effective_entry_point()),
        );
        Ok(out)
    }

    /// Compile multiple named modules independently, producing one `.o` per module.
    ///
    /// Returns assembled objects in input order, or the diagnostics from the first failed module.
    #[track_caller]
    pub fn compile_modules(
        &mut self,
        modules: &[(&str, &str)],
    ) -> Result<Vec<AssembledOutput>, CompilationError> {
        let mut result = Vec::with_capacity(modules.len());

        for (module_name, source) in modules {
            self.set_artifact_stem(Some(module_name.to_string()));

            let compile_result = self.compile(source)?;

            let (_, tokens) = self.compile_ir_to_assembly_with_tokens(&compile_result.ir_program);

            let assembled = self
                .assemble_named(module_name, &tokens)
                .map_err(|e| CompilationError::FreestandingErrors(vec![e.message]))?;

            result.push(assembled);
        }

        Ok(result)
    }

    /// Link multiple compiled objects and apply layout post-processing.
    ///
    /// Returns the linked output ready to load into a VM.
    #[track_caller]
    pub fn link_modules(
        &self,
        module_names: &[&str],
        objects: &[&AssembledOutput],
    ) -> Result<AssembledOutput, LinkerError> {
        let modules: Vec<(&str, &AssembledOutput)> = module_names
            .iter()
            .zip(objects.iter())
            .map(|(n, o)| (*n, *o))
            .collect();
        let combined_stem = module_names.join("_");
        self.link_assembled_objects_named(&combined_stem, &modules)
    }

    /// Compile kernel modules and link them with the provided stdlib object.
    ///
    /// Returns a fully linked kernel image ready to load into a VM.
    #[track_caller]
    pub fn compile_kernel_modules_with_stdlib(
        &mut self,
        kernel_modules: &[(&str, &str)],
        stdlib_object: &AssembledOutput,
    ) -> Result<AssembledOutput, CompilationError> {
        let kernel_objects = self.compile_modules(kernel_modules)?;

        let mut module_names = vec!["kernel_stdlib"];
        let mut object_refs = vec![stdlib_object];
        for (module_name, _) in kernel_modules {
            module_names.push(*module_name);
        }
        let kernel_refs: Vec<&AssembledOutput> = kernel_objects.iter().collect();
        object_refs.extend(kernel_refs);

        let combined_stem = module_names.join("_");
        self.link_assembled_objects_named(
            &combined_stem,
            &module_names
                .iter()
                .zip(object_refs.iter())
                .map(|(n, o)| (*n, *o))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| {
            CompilationError::FreestandingErrors(vec![format!("linker error: {}", e.message)])
        })
    }
}

fn sanitize_artifact_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "artifact".to_owned()
    } else {
        trimmed.to_owned()
    }
}
