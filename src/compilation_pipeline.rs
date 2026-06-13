use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::assembler::{Assembler, AssemblerError};
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::{AssembledOutput, LinkerError, ObjectLinker};
pub use hll_to_ir::TargetMode;
use hll_to_ir::{
    CompileConfig, Diagnostic, DiagnosticLevel, HllCompiler, IrProgram, IrType, OptOptions,
    optimize_ir,
};
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
    peephole: bool,
    register_allocation: bool,
    omit_frame_pointer: bool,
    optimize: OptOptions,
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
            peephole: true,
            register_allocation: true,
            omit_frame_pointer: true,
            optimize: OptOptions::none(),
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
            peephole: true,
            register_allocation: true,
            omit_frame_pointer: true,
            optimize: OptOptions::none(),
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

    /// Enable the conservative assembly peephole pass. Off by default; when on,
    /// the emitted token stream (and the `.s` text rendered from it) is optimized
    /// before assembly.
    pub fn set_peephole(&mut self, enabled: bool) {
        self.peephole = enabled;
    }

    /// Enable or disable physical register allocation in the RV64 backend: hot
    /// scalar values are kept in callee-saved registers (s2-s11) instead of
    /// stack slots. On by default; turn it off to get the pure stack-slot
    /// lowering (e.g. for codegen-shape comparisons).
    pub fn set_register_allocation(&mut self, enabled: bool) {
        self.register_allocation = enabled;
    }

    /// Omit the redundant s0 frame pointer in the RV64 backend. On by default.
    pub fn set_omit_frame_pointer(&mut self, enabled: bool) {
        self.omit_frame_pointer = enabled;
    }

    /// Enable IR-level optimization passes (constant folding, dead-code
    /// elimination). Off by default so IR/assembly goldens stay stable; when on,
    /// the passes run on the lowered IR before backend lowering.
    pub fn set_optimize(&mut self, opts: OptOptions) {
        self.optimize = opts;
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

        let mut out = compiler
            .compile(source)
            .map_err(CompilationError::DiagnosticErrors)?;
        optimize_ir(&mut out.ir, self.optimize);

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
    /// `stdlib_tokens`: Pass `None` when compiling stdlib or kernel sources standalone.
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

        let mut out = match compiler.compile(source) {
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
        optimize_ir(&mut out.ir, self.optimize);

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
        compiler.set_peephole(self.peephole);
        compiler.set_register_allocation(self.register_allocation);
        compiler.set_omit_frame_pointer(self.omit_frame_pointer);
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

// --- Filesystem layout ---

/// Canonical on-disk filesystem layout constants.
///
/// Mirrors `FS_*` in `kernel/fs.hll`. See _`OS_SPECIFICATION.md` for filesystem layout.
///
///
///
pub mod fs_layout {
    /// Block size in bytes (`FS_BLOCK_SIZE`).
    pub const BLOCK_SIZE: usize = 4096;
    /// Inode size in bytes (`FS_INODE_SIZE`).
    pub const INODE_SIZE: usize = 128;
    /// Total inode count (`FS_MAX_INODES`).
    pub const INODE_COUNT: usize = 256;
    /// Blocks occupied by the inode table (`INODE_COUNT * INODE_SIZE / BLOCK_SIZE`).
    pub const INODE_TABLE_BLOCKS: usize = INODE_COUNT * INODE_SIZE / BLOCK_SIZE;
    /// Free-block bitmap block index (`FS_BITMAP_BLOCK`); follows the inode table.
    pub const BITMAP_BLOCK: usize = 1 + INODE_TABLE_BLOCKS;
    /// First data block index (`FS_DATA_BLOCK_START`); follows the bitmap.
    pub const DATA_BLOCK_START: usize = BITMAP_BLOCK + 1;
    /// Directory-entry size in bytes (`FS_DIRENT_SIZE`).
    pub const DIRENT_SIZE: usize = 36;
    /// Directory entries per block (`FS_DIRENTS_PER_BLOCK`).
    pub const DIRENTS_PER_BLOCK: usize = BLOCK_SIZE / DIRENT_SIZE;
    /// Direct block pointers per inode (`FS_INODE_BLOCKS`).
    pub const MAX_DIRECT_BLOCKS: usize = 44;
    /// Largest data-block count the layout supports (`FS_MAX_BLOCKS`).
    pub const MAX_DATA_BLOCKS: usize = 255;

    // Inode field offsets (FS_IN_*).
    pub const IN_TYPE: usize = 0;
    pub const IN_PARENT: usize = 2;
    pub const IN_SIZE: usize = 4;
    pub const IN_NAME: usize = 8;
    pub const IN_BLOCKS: usize = 40;

    // Directory-entry field offsets (FS_DE_*).
    pub const DE_NAME: usize = 0;
    pub const DE_INODE: usize = 32;

    // Superblock field offsets (FS_SB_*); MAGIC/VERSION/INODE_COUNT/ROOT_INODE are
    // host-only header fields the kernel reads positionally.
    pub const SB_MAGIC: usize = 0;
    pub const SB_VERSION: usize = 8;
    pub const SB_INODE_COUNT: usize = 10;
    pub const SB_BLOCK_COUNT: usize = 12;
    pub const SB_ROOT_INODE: usize = 14;
    pub const SB_FREE_INODES: usize = 16;
    pub const SB_FREE_BLOCKS: usize = 20;
    pub const SB_INODE_BITMAP: usize = 24;
}

// --- Filesystem image builder ---

/// An entry to include in a filesystem image built by [`build_fs_image`].
pub enum FsEntry<'a> {
    /// A directory at the given absolute path (e.g. `"/bin"`).
    Dir { path: &'a str },
    /// A file at the given absolute path with the given content.
    File { path: &'a str, data: &'a [u8] },
}

/// Serialise `entries` into the on-disk filesystem image format and return the raw bytes.
///
/// See _`OS_SPECIFICATION.md` for block layout.
///
///
///
///
/// Layout constants live in [`fs_layout`] and are checked against `fs.hll`.
pub fn build_fs_image(entries: &[FsEntry<'_>]) -> Vec<u8> {
    use fs_layout::{
        BITMAP_BLOCK, BLOCK_SIZE, DATA_BLOCK_START, DE_INODE, DE_NAME, DIRENT_SIZE, IN_BLOCKS,
        IN_NAME, IN_PARENT, IN_SIZE, IN_TYPE, INODE_COUNT, INODE_SIZE, MAX_DATA_BLOCKS,
        MAX_DIRECT_BLOCKS, SB_BLOCK_COUNT, SB_FREE_BLOCKS, SB_FREE_INODES, SB_INODE_BITMAP,
        SB_INODE_COUNT, SB_ROOT_INODE, SB_VERSION,
    };

    // One block per file (rounded up) plus one per directory and the root, then
    // a margin of free blocks so the running FS can create and grow files.
    let mut needed_data_blocks: usize = 1; // root directory block
    for entry in entries {
        match entry {
            FsEntry::File { data, .. } => {
                needed_data_blocks += data.len().div_ceil(BLOCK_SIZE).max(1);
            }
            FsEntry::Dir { .. } => needed_data_blocks += 1,
        }
    }
    needed_data_blocks += 56;
    let total_data_blocks = needed_data_blocks.min(MAX_DATA_BLOCKS);
    assert!(
        needed_data_blocks <= MAX_DATA_BLOCKS,
        "boot FS image needs {needed_data_blocks} data blocks but the layout caps at \
         {MAX_DATA_BLOCKS}; reduce the bundled file sizes"
    );
    let total_blocks = DATA_BLOCK_START + total_data_blocks;
    let image_size = total_blocks * BLOCK_SIZE;

    let mut image = vec![0u8; image_size];

    // --- Helpers ---

    let inode_offset = |idx: usize| -> usize { BLOCK_SIZE + idx * INODE_SIZE };
    let block_offset = |blk: usize| -> usize { blk * BLOCK_SIZE };

    let write_u16 = |buf: &mut Vec<u8>, off: usize, val: u16| {
        buf[off] = (val & 0xFF) as u8;
        buf[off + 1] = (val >> 8) as u8;
    };
    let write_u32 = |buf: &mut Vec<u8>, off: usize, val: u32| {
        buf[off] = (val & 0xFF) as u8;
        buf[off + 1] = ((val >> 8) & 0xFF) as u8;
        buf[off + 2] = ((val >> 16) & 0xFF) as u8;
        buf[off + 3] = ((val >> 24) & 0xFF) as u8;
    };
    let write_u64 = |buf: &mut Vec<u8>, off: usize, val: u64| {
        for i in 0..8usize {
            buf[off + i] = ((val >> (i * 8)) & 0xFF) as u8;
        }
    };

    // next_inode and next_data_block are mutable state we thread through.
    let mut next_inode: usize = 1; // 0 = root
    let mut next_data_block: usize = 0; // relative to DATA_BLOCK_START; absolute = + DATA_BLOCK_START

    // Allocate a data block and return its absolute block index.
    let alloc_block = |next: &mut usize| -> usize {
        let blk = DATA_BLOCK_START + *next;
        *next += 1;
        blk
    };

    // Allocate an inode index.
    let alloc_inode = |next: &mut usize| -> usize {
        let idx = *next;
        *next += 1;
        idx
    };

    // Write a name into a 32-byte field at buf[off..off+32].
    let write_name = |buf: &mut Vec<u8>, off: usize, name: &str| {
        let bytes = name.as_bytes();
        let len = bytes.len().min(31);
        buf[off..off + len].copy_from_slice(&bytes[..len]);
        buf[off + len] = 0;
    };

    // Root directory (inode 0)
    let root_data_blk = alloc_block(&mut next_data_block);
    {
        let off = inode_offset(0);
        image[off + IN_TYPE] = 2; // directory
        write_u16(&mut image, off + IN_PARENT, 0); // root's parent = self
        write_u32(&mut image, off + IN_SIZE, 0); // entry count updated later
        write_name(&mut image, off + IN_NAME, "/");
        write_u16(&mut image, off + IN_BLOCKS, root_data_blk as u16);
    }

    // Add a DirEntry to a directory inode's data block
    // Returns false if block is full (not handled for simplicity; 113 entries per block).
    let add_dirent = |image: &mut Vec<u8>, dir_inode: usize, child_inode: usize, name: &str| {
        let off = inode_offset(dir_inode);
        let count = u32::from_le_bytes([
            image[off + IN_SIZE],
            image[off + IN_SIZE + 1],
            image[off + IN_SIZE + 2],
            image[off + IN_SIZE + 3],
        ]) as usize;

        // Get first data block of directory.
        let blk_idx =
            u16::from_le_bytes([image[off + IN_BLOCKS], image[off + IN_BLOCKS + 1]]) as usize;
        let blk_off = block_offset(blk_idx);
        let de_off = blk_off + count * DIRENT_SIZE;

        // Write name field (DE_NAME, 32 bytes).
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(31);
        image[de_off + DE_NAME..de_off + DE_NAME + name_len]
            .copy_from_slice(&name_bytes[..name_len]);
        image[de_off + DE_NAME + name_len] = 0;
        // Write the child inode index (u16) at DE_INODE.
        image[de_off + DE_INODE] = (child_inode & 0xFF) as u8;
        image[de_off + DE_INODE + 1] = ((child_inode >> 8) & 0xFF) as u8;

        // Increment parent size.
        let new_count = (count + 1) as u32;
        image[off + IN_SIZE] = (new_count & 0xFF) as u8;
        image[off + IN_SIZE + 1] = ((new_count >> 8) & 0xFF) as u8;
        image[off + IN_SIZE + 2] = ((new_count >> 16) & 0xFF) as u8;
        image[off + IN_SIZE + 3] = ((new_count >> 24) & 0xFF) as u8;
    };

    // --- Process entries in order (dirs before files) ---

    // Map from absolute path -> inode index, seeded with root.
    let mut path_to_inode: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    path_to_inode.insert("/".to_owned(), 0);

    // Sort: directories first, then files, both sorted by path length so parents come first.
    let mut sorted: Vec<&FsEntry<'_>> = entries.iter().collect();
    sorted.sort_by_key(|e| match e {
        FsEntry::Dir { path } => (0usize, path.len(), *path),
        FsEntry::File { path, .. } => (1usize, path.len(), *path),
    });

    for entry in sorted {
        match entry {
            FsEntry::Dir { path } => {
                if *path == "/" {
                    continue; // root already created
                }
                let (parent_path, name) = split_last_component(path);
                let parent_inode = *path_to_inode.get(parent_path).unwrap_or(&0);

                let dir_inode = alloc_inode(&mut next_inode);
                let dir_data_blk = alloc_block(&mut next_data_block);
                {
                    let off = inode_offset(dir_inode);
                    image[off + IN_TYPE] = 2;
                    write_u16(&mut image, off + IN_PARENT, parent_inode as u16);
                    write_u32(&mut image, off + IN_SIZE, 0);
                    write_name(&mut image, off + IN_NAME, name);
                    write_u16(&mut image, off + IN_BLOCKS, dir_data_blk as u16);
                }
                add_dirent(&mut image, parent_inode, dir_inode, name);
                path_to_inode.insert(path.to_string(), dir_inode);
            }
            FsEntry::File { path, data } => {
                let (parent_path, name) = split_last_component(path);
                let parent_inode = *path_to_inode.get(parent_path).unwrap_or(&0);

                let file_inode = alloc_inode(&mut next_inode);
                let num_blocks = data.len().div_ceil(BLOCK_SIZE);
                let num_blocks = num_blocks.max(1); // at least one block even for empty files
                assert!(
                    num_blocks <= MAX_DIRECT_BLOCKS,
                    "file {path:?} needs {num_blocks} blocks but the inode holds at most \
                     {MAX_DIRECT_BLOCKS} direct blocks ({} bytes)",
                    MAX_DIRECT_BLOCKS * BLOCK_SIZE
                );

                {
                    let off = inode_offset(file_inode);
                    image[off + IN_TYPE] = 1;
                    write_u16(&mut image, off + IN_PARENT, parent_inode as u16);
                    write_u32(&mut image, off + IN_SIZE, data.len() as u32);
                    write_name(&mut image, off + IN_NAME, name);

                    for b in 0..num_blocks {
                        let blk = alloc_block(&mut next_data_block);
                        let blk_slot_off = off + IN_BLOCKS + b * 2;
                        image[blk_slot_off] = (blk & 0xFF) as u8;
                        image[blk_slot_off + 1] = ((blk >> 8) & 0xFF) as u8;

                        let src_start = b * BLOCK_SIZE;
                        let src_end = (src_start + BLOCK_SIZE).min(data.len());
                        let blk_off = block_offset(blk);
                        if src_start < data.len() {
                            let chunk = &data[src_start..src_end];
                            image[blk_off..blk_off + chunk.len()].copy_from_slice(chunk);
                        }
                    }
                }
                add_dirent(&mut image, parent_inode, file_inode, name);
                path_to_inode.insert(path.to_string(), file_inode);
            }
        }
    }

    // --- Superblock ---
    {
        let off = 0usize;
        // Magic: "HLLFS\0\1\0"
        let magic: [u8; 8] = [0x48, 0x4C, 0x4C, 0x46, 0x53, 0x00, 0x01, 0x00];
        image[off..off + 8].copy_from_slice(&magic);
        write_u16(&mut image, off + SB_VERSION, 1);
        write_u16(&mut image, off + SB_INODE_COUNT, INODE_COUNT as u16);
        write_u16(&mut image, off + SB_BLOCK_COUNT, total_data_blocks as u16);
        write_u16(&mut image, off + SB_ROOT_INODE, 0);

        let used_inodes = next_inode;
        let free_inodes = (INODE_COUNT - used_inodes) as u32;
        write_u32(&mut image, off + SB_FREE_INODES, free_inodes);

        let used_data_blocks = next_data_block;
        let free_blocks = (total_data_blocks.saturating_sub(used_data_blocks)) as u32;
        write_u32(&mut image, off + SB_FREE_BLOCKS, free_blocks);

        // Inode bitmap: 256 bits (32 bytes). 1 = free.
        for i in 0..INODE_COUNT {
            let byte = i / 8;
            let bit = i % 8;
            if i >= used_inodes {
                image[off + SB_INODE_BITMAP + byte] |= 1 << bit;
            }
        }
    }

    // --- Free-block bitmap (BITMAP_BLOCK) ---
    // One bit per data block starting at DATA_BLOCK_START; bit 0 of byte 0 = first data block.
    {
        let bmap_off = block_offset(BITMAP_BLOCK);
        for i in 0..total_data_blocks {
            let byte = i / 8;
            let bit = i % 8;
            if i >= next_data_block {
                image[bmap_off + byte] |= 1 << bit;
            }
        }
    }

    // Suppress unused-variable warnings from closures (write_u64 reserved for future use).
    let _ = write_u64;

    image
}

// --- Executable file format (FEXE) ---

/// Size of the executable header block. The payload starts at this offset so it
/// stays 4 KiB-aligned for the kernel's page-by-page load.
pub const EXEC_HEADER_SIZE: usize = 4096;

/// Magic number identifying a FEXE executable file ("FEXE", little-endian).
pub const FEXE_MAGIC: u32 = 0x4558_4546;

/// Wrap a position-independent flat binary in the FEXE executable-file format
/// understood by the kernel's `sys_exec`. The result is a header block (magic +
/// entry offset) followed by the payload, suitable for storing as a regular file
/// via [`build_fs_image`].
pub fn build_exec_file(entry_off: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; EXEC_HEADER_SIZE + payload.len()];
    out[0..4].copy_from_slice(&FEXE_MAGIC.to_le_bytes());
    out[8..16].copy_from_slice(&entry_off.to_le_bytes());
    out[EXEC_HEADER_SIZE..].copy_from_slice(payload);
    out
}

/// Build a FEXE executable file from an assembled hosted program. The entry
/// offset is taken from `_start`; the payload is the flat binary (which already
/// includes zero-filled BSS, so the program's globals are mapped on load).
pub fn assembled_to_exec_file(assembled: &AssembledOutput) -> Vec<u8> {
    let entry_off = assembled.symbol_address("_start").unwrap_or(0);
    let payload = assembled.to_flat_binary();
    build_exec_file(entry_off, &payload)
}

#[cfg(test)]
mod fs_image_tests {
    use super::*;

    // Parse a `const FS_NAME = <int>` declaration out of fs.hll. Handles decimal
    // and 0x-prefixed hex.
    fn fs_hll_const(src: &str, name: &str) -> i64 {
        for line in src.lines() {
            let line = line.trim();
            let rest = match line.strip_prefix("const ") {
                Some(r) => r,
                None => continue,
            };
            let (lhs, rhs) = match rest.split_once('=') {
                Some(p) => p,
                None => continue,
            };
            if lhs.trim() != name {
                continue;
            }
            let val = rhs.split(';').next().unwrap_or("").trim();
            return if let Some(hex) = val.strip_prefix("0x") {
                i64::from_str_radix(hex, 16).expect("hex fs.hll const")
            } else {
                val.parse().expect("decimal fs.hll const")
            };
        }
        panic!("const {name} not found in fs.hll");
    }

    // The host image builder and the running kernel must agree on the on-disk
    // layout byte for byte. fs.hll is the source of truth; this guards the copy
    // in fs_layout from drifting.
    #[test]
    fn fs_layout_matches_fs_hll() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/os-runtime/kernel/fs.hll"
        );
        let src = std::fs::read_to_string(path).expect("read fs.hll");

        let pairs: &[(&str, i64)] = &[
            ("FS_BLOCK_SIZE", fs_layout::BLOCK_SIZE as i64),
            ("FS_INODE_SIZE", fs_layout::INODE_SIZE as i64),
            ("FS_MAX_INODES", fs_layout::INODE_COUNT as i64),
            ("FS_BITMAP_BLOCK", fs_layout::BITMAP_BLOCK as i64),
            ("FS_DATA_BLOCK_START", fs_layout::DATA_BLOCK_START as i64),
            ("FS_DIRENT_SIZE", fs_layout::DIRENT_SIZE as i64),
            ("FS_DIRENTS_PER_BLOCK", fs_layout::DIRENTS_PER_BLOCK as i64),
            ("FS_INODE_BLOCKS", fs_layout::MAX_DIRECT_BLOCKS as i64),
            ("FS_IN_TYPE", fs_layout::IN_TYPE as i64),
            ("FS_IN_PARENT", fs_layout::IN_PARENT as i64),
            ("FS_IN_SIZE", fs_layout::IN_SIZE as i64),
            ("FS_IN_NAME", fs_layout::IN_NAME as i64),
            ("FS_IN_BLOCKS", fs_layout::IN_BLOCKS as i64),
            ("FS_DE_NAME", fs_layout::DE_NAME as i64),
            ("FS_DE_INODE", fs_layout::DE_INODE as i64),
            ("FS_SB_BLOCK_COUNT", fs_layout::SB_BLOCK_COUNT as i64),
            ("FS_SB_FREE_INODES", fs_layout::SB_FREE_INODES as i64),
            ("FS_SB_FREE_BLOCKS", fs_layout::SB_FREE_BLOCKS as i64),
            ("FS_SB_INODE_BITMAP", fs_layout::SB_INODE_BITMAP as i64),
        ];

        for (name, rust_val) in pairs {
            let hll_val = fs_hll_const(&src, name);
            assert_eq!(
                hll_val, *rust_val,
                "{name}: fs.hll has {hll_val} but fs_layout has {rust_val}"
            );
        }
    }

    // Regression: the image must be sized for the sum of all files' blocks, not
    // just the largest one, or the copy loop runs past the data region.
    #[test]
    fn build_fs_image_sizes_for_total_of_all_files() {
        const BLOCK: usize = 4096;
        // Four 30-block files sum to 120 blocks, well past any single file.
        let big = vec![7u8; 30 * BLOCK];
        let entries = vec![
            FsEntry::Dir { path: "/bin" },
            FsEntry::File {
                path: "/bin/a.fexe",
                data: &big,
            },
            FsEntry::File {
                path: "/bin/b.fexe",
                data: &big,
            },
            FsEntry::File {
                path: "/bin/c.fexe",
                data: &big,
            },
            FsEntry::Dir { path: "/home" },
            FsEntry::File {
                path: "/home/d.fexe",
                data: &big,
            },
        ];

        // Must not panic and must hold all 120 data blocks plus metadata.
        let image = build_fs_image(&entries);
        assert!(
            image.len() >= 120 * BLOCK,
            "image too small ({} bytes) for the summed file contents",
            image.len()
        );
    }
}

/// Split an absolute path into (`parent_path`, `last_component`).
/// "/bin/init" -> ("/bin", "init"), "/hello.txt" -> ("/", "hello.txt").
fn split_last_component(path: &str) -> (&str, &str) {
    if let Some(pos) = path.rfind('/') {
        let parent = if pos == 0 { "/" } else { &path[..pos] };
        let name = &path[pos + 1..];
        (parent, name)
    } else {
        ("/", path)
    }
}
