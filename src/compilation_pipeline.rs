//! Compilation pipeline: HLL -> IR -> ASM -> OBJ -> ELF, with opt-in artifact writes.
//! Each HLL file compiles to its own object; objects link with full relocation support.

use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::assembler::{Assembler, AssemblerError};
use asm_to_binary::rv_instruction::RvInstruction;
use asm_to_binary::{AssembledOutput, LinkerError, ObjectLinker};
use hll_to_ir::{CompileConfig, HllCompiler, IrProgram, IrType, OptOptions, optimize_ir};
pub use hll_to_ir::{Diagnostic, DiagnosticLevel, TargetMode};
use ir_to_asm::{CodegenError, CompilerRv64};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::panic::Location;
use std::path::{Path, PathBuf};

mod artifacts;
mod executable;
mod fs_image;
mod imports;
#[cfg(test)]
mod imports_tests;
mod result;

use artifacts::sanitize_artifact_component;
pub use executable::*;
pub use fs_image::*;
pub use imports::bundled_module_source;
#[cfg(test)]
use imports::normalize_lexical;
use imports::{
    bundled_module_path, mark_all_defined_symbols_global, programs_module_resolver, source_identity,
};
pub use result::*;

pub const DEFAULT_ARTIFACT_DIRECTORY: &str = "out";

// --- Pipeline ---

/// Resolves an `import("name")` module binding to HLL source, over the host module
/// registry or filesystem. `None` means the name is unknown.
pub type ModuleResolver = Box<dyn Fn(&str) -> Option<String>>;

struct ResolvedModules {
    prelude: Option<String>,
    aliases: std::collections::HashMap<String, hll_to_ir::imports::ModuleAlias>,
}

struct ResolvedModuleSource {
    key: String,
    source: String,
    source_path: Option<PathBuf>,
    is_stdlib: bool,
}

pub struct CompilationPipeline {
    run_semantic_analysis: bool,
    target_mode: TargetMode,
    entry_point: Option<String>,
    link_layout: Option<LinkLayout>,
    string_prefix: Option<String>,
    type_prelude: Vec<(String, IrType)>,
    source_prelude: String,
    current_source_path: Option<PathBuf>,
    // Set during a closure build: mangle each module's own symbols by its `current_module_prefix`
    // so separately compiled modules in the link set do not collide.
    mangle_modules: bool,
    current_module_prefix: Option<String>,
    // Whether a closure build mangles per-module symbols.
    mangle_in_closure: bool,
    module_resolver: Option<ModuleResolver>,
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
            source_prelude: String::new(),
            current_source_path: None,
            mangle_modules: false,
            current_module_prefix: None,
            mangle_in_closure: true,
            module_resolver: Some(Box::new(programs_module_resolver)),
            artifact_root: PathBuf::from(DEFAULT_ARTIFACT_DIRECTORY),
            artifact_stem: RefCell::new(None),
            last_artifact_stem: RefCell::new(None),
            write_artifacts: false,
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
            source_prelude: String::new(),
            current_source_path: None,
            mangle_modules: false,
            current_module_prefix: None,
            mangle_in_closure: true,
            module_resolver: Some(Box::new(programs_module_resolver)),
            artifact_root: PathBuf::from(DEFAULT_ARTIFACT_DIRECTORY),
            artifact_stem: RefCell::new(None),
            last_artifact_stem: RefCell::new(None),
            write_artifacts: false,
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

    /// Set HLL source prepended to every compiled unit (e.g. the kernel ABI),
    /// so consts and structs stay defined once in HLL across separately-compiled TUs.
    pub fn set_source_prelude(&mut self, src: impl Into<String>) {
        self.source_prelude = src.into();
    }

    /// Set the filesystem path of the source currently being compiled. Relative
    /// `import("./x.hll")` and `import("../x.hll")` resolve against this file's directory.
    pub fn set_current_source_path(&mut self, path: Option<impl Into<PathBuf>>) {
        self.current_source_path = path.map(Into::into);
    }

    fn config_source_file(&self) -> Option<String> {
        self.current_source_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Set the resolver mapping `import("name")` to that module's source (default: the
    /// `PROGRAMS`-backed lookup). Pass `None` to disable import resolution entirely.
    pub fn set_module_resolver(&mut self, resolver: Option<ModuleResolver>) {
        self.module_resolver = resolver;
    }

    /// Enable or disable per-module symbol mangling in `compile_program_closure` (default on).
    pub fn set_module_mangling(&mut self, enabled: bool) {
        self.mangle_in_closure = enabled;
    }

    fn module_key(path: &str) -> &str {
        path.trim_start_matches("./")
    }

    // The mangling prefix: file stem for path imports, else the registry key,
    // sanitized to an identifier.
    fn module_prefix(key: &str, source_path: Option<&Path>) -> String {
        let raw = source_path
            .and_then(Path::file_stem)
            .and_then(|s| s.to_str())
            .unwrap_or(key);
        raw.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }

    /// Source -> tokens -> AST -> IR, with mode-specific validation.
    #[track_caller]
    pub fn compile(&self, source: &str) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline");

        let resolved = self
            .resolve_modules(source)
            .map_err(CompilationError::DiagnosticErrors)?;
        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
            type_prelude: self.type_prelude.clone(),
            source_prelude: resolved.prelude,
            module_aliases: resolved.aliases,
            module_mangle_prefix: self
                .mangle_modules
                .then(|| self.current_module_prefix.clone())
                .flatten(),
            source_file: self.config_source_file(),
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
        self.write_text_artifact("ir", &stem, ".ir", &out.ir.to_string());

        log::info!("Compilation complete");
        Ok(CompilationResult {
            ir_program: out.ir,
            diagnostics: out.diagnostics,
        })
    }

    /// Run all pipeline stages, returning typed per-stage outputs. `stdlib_objects`
    /// is empty when compiling stdlib or kernel sources standalone.
    #[track_caller]
    pub fn run_full(
        &self,
        source: &str,
        stdlib_objects: &[(&str, &AssembledOutput)],
    ) -> PipelineResult {
        let resolved = match self.resolve_modules(source) {
            Ok(resolved) => resolved,
            Err(diagnostics) => {
                return PipelineResult {
                    diagnostics,
                    lex: None,
                    parse: None,
                    ir: None,
                    asm: None,
                    binary: None,
                    failure: None,
                };
            }
        };
        let compiler = HllCompiler::new(CompileConfig {
            target: self.target_mode,
            strict: self.run_semantic_analysis,
            string_prefix: self.string_prefix.clone(),
            type_prelude: self.type_prelude.clone(),
            source_prelude: resolved.prelude,
            module_aliases: resolved.aliases,
            module_mangle_prefix: self
                .mangle_modules
                .then(|| self.current_module_prefix.clone())
                .flatten(),
            source_file: self.config_source_file(),
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
                    failure: None,
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
        self.write_text_artifact("ir", &ir_stem, ".ir", &ir_display);

        let ir = Some(IrOutput {
            display: ir_display,
        });

        let (asm_text, user_tokens) = match self.compile_ir_to_assembly_with_tokens(&out.ir) {
            Ok(output) => output,
            Err(error) => {
                return PipelineResult {
                    diagnostics,
                    lex,
                    parse,
                    ir,
                    asm: None,
                    binary: None,
                    failure: Some(PipelineFailure::Codegen(error)),
                };
            }
        };
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
                    failure: Some(PipelineFailure::Assembler(e)),
                };
            }
        };

        // Link the user object against the pre-built stdlib objects (compiled as
        // separate translation units; no source concatenation).
        let mut modules: Vec<(&str, &AssembledOutput)> = stdlib_objects.to_vec();
        modules.push(("user", &user_obj));

        let (binary, failure) = match self.link_assembled_objects_named(&user_stem, &modules) {
            Ok(assembled) => (Some(BinaryOutput { assembled }), None),
            Err(e) => (None, Some(PipelineFailure::Linker(e))),
        };

        PipelineResult {
            diagnostics,
            lex,
            parse,
            ir,
            asm,
            binary,
            failure,
        }
    }

    /// Compile an IR program to RISC-V assembly text.
    #[track_caller]
    pub fn compile_ir_to_assembly(&self, ir: &IrProgram) -> Result<String, CodegenError> {
        self.compile_ir_to_assembly_with_tokens(ir)
            .map(|(assembly, _)| assembly)
    }

    /// Compile an IR program to assembly text and the structured token stream.
    #[track_caller]
    pub fn compile_ir_to_assembly_with_tokens(
        &self,
        ir: &IrProgram,
    ) -> Result<(String, Vec<RvInstruction>), CodegenError> {
        let mut compiler = CompilerRv64::new();
        compiler.set_peephole(self.peephole);
        compiler.set_register_allocation(self.register_allocation);
        compiler.set_omit_frame_pointer(self.omit_frame_pointer);
        let (asm, tokens) = compiler.compile_with_tokens(ir)?;
        let stem = if let Some(existing) = self.current_artifact_stem() {
            existing
        } else {
            let stem = self.default_artifact_stem(Location::caller(), "asm", &asm);
            *self.last_artifact_stem.borrow_mut() = Some(stem.clone());
            stem
        };
        self.write_text_artifact("asm", &stem, ".s", &asm);
        Ok((asm, tokens))
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
        let mut assembled = Assembler::assemble(tokens)?;
        let entry = self.effective_entry_point();
        if assembled.has_symbol(entry) {
            assembled.mark_entry_global(entry);
        }
        if self.target_mode == TargetMode::Hosted && assembled.has_symbol("main") {
            assembled.mark_entry_global("main");
        }
        let stem = sanitize_artifact_component(stem);
        *self.last_artifact_stem.borrow_mut() = Some(stem.clone());
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
        self.write_bytes_artifact(
            "elf",
            &format!("total_{stem}"),
            ".elf",
            &out.to_elf_with_entry(self.effective_load_base(), self.effective_entry_point()),
        );
        Ok(out)
    }

    /// Compile named modules independently, one `.o` each, returned in input order.
    /// Fails with the diagnostics of the first module that does not compile.
    #[track_caller]
    pub fn compile_modules(
        &mut self,
        modules: &[(&str, &str)],
    ) -> Result<Vec<AssembledOutput>, CompilationError> {
        let mut result = Vec::with_capacity(modules.len());
        let saved_string_prefix = self.string_prefix.clone();

        for (module_name, source) in modules {
            self.set_artifact_stem(Some(module_name.to_string()));
            let namespace = sanitize_artifact_component(module_name);
            let base = saved_string_prefix.as_deref().unwrap_or("__module_");
            self.string_prefix = Some(format!("{base}{namespace}_str_"));
            let compiled = (|| {
                let compile_result = self.compile(source)?;
                let (_, tokens) = self
                    .compile_ir_to_assembly_with_tokens(&compile_result.ir_program)
                    .map_err(CompilationError::Codegen)?;
                self.assemble_named(module_name, &tokens)
                    .map_err(|e| CompilationError::FreestandingErrors(vec![e.message]))
            })();
            self.string_prefix = saved_string_prefix.clone();
            result.push(compiled?);
        }

        Ok(result)
    }

    /// Compile a primary unit plus every module reachable through its qualified `import("path")`
    /// declarations (the link closure) into per-module objects, deps first, cycles rejected.
    #[track_caller]
    pub fn compile_program_closure(
        &mut self,
        primary_name: &str,
        primary_src: &str,
    ) -> Result<Vec<(String, AssembledOutput)>, CompilationError> {
        let mut order: Vec<(String, String, Option<PathBuf>)> = Vec::new();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        let mut stack_keys: Vec<String> = Vec::new();
        let mut stack_sources: Vec<String> = Vec::new();
        self.collect_import_closure(
            primary_name,
            primary_src,
            self.current_source_path.clone(),
            &mut order,
            &mut visited,
            &mut stack,
            &mut stack_keys,
            &mut stack_sources,
        )?;

        let saved_source_path = self.current_source_path.clone();
        let saved_mangle = self.mangle_modules;
        let saved_prefix = self.current_module_prefix.clone();
        self.mangle_modules = self.mangle_in_closure;
        let mut objects = Vec::with_capacity(order.len());
        for (module_name, source, source_path) in &order {
            self.set_artifact_stem(Some(module_name.clone()));
            self.current_source_path = source_path.clone();
            self.current_module_prefix =
                Some(Self::module_prefix(module_name, source_path.as_deref()));
            let compile_result = self.compile(source)?;
            let (_, tokens) = self
                .compile_ir_to_assembly_with_tokens(&compile_result.ir_program)
                .map_err(CompilationError::Codegen)?;
            let assembled = self
                .assemble_named(module_name, &tokens)
                .map_err(|e| CompilationError::FreestandingErrors(vec![e.message]))?;
            objects.push(assembled);
        }
        self.current_source_path = saved_source_path;
        self.mangle_modules = saved_mangle;
        self.current_module_prefix = saved_prefix;
        Ok(order
            .into_iter()
            .map(|(name, _, _)| name)
            .zip(objects)
            .collect())
    }

    // Post-order DFS; a revisit of an in-progress module is a cycle.
    fn collect_import_closure(
        &self,
        name: &str,
        source: &str,
        source_path: Option<PathBuf>,
        order: &mut Vec<(String, String, Option<PathBuf>)>,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut Vec<String>,
        stack_keys: &mut Vec<String>,
        stack_sources: &mut Vec<String>,
    ) -> Result<(), CompilationError> {
        if visited.contains(name) {
            return Ok(());
        }
        let identity = source_identity(name, source_path.as_deref());
        if stack.iter().any(|m| m == name)
            || stack_keys.iter().any(|key| key == &identity)
            || stack_sources.iter().any(|seen| seen == source)
        {
            if self.mangle_in_closure || self.target_mode == TargetMode::Kernel {
                stack.push(name.to_owned());
                return Err(CompilationError::FreestandingErrors(vec![format!(
                    "cyclic import: {}",
                    stack.join(" -> ")
                )]));
            }
            return Ok(());
        }
        stack.push(name.to_owned());
        stack_keys.push(identity);
        stack_sources.push(source.to_owned());

        let module_imports = hll_to_ir::imports::collect_module_imports(source)
            .map_err(|msg| CompilationError::FreestandingErrors(vec![msg]))?;
        for (_alias, path) in module_imports {
            let resolved = self
                .resolve_module_source(&path, source_path.as_deref())
                .map_err(|msg| CompilationError::FreestandingErrors(vec![msg]))?
                .ok_or_else(|| {
                    let key = Self::module_key(&path);
                    CompilationError::FreestandingErrors(vec![format!(
                        "cannot resolve import(\"{path}\"): no module named `{key}`"
                    )])
                })?;
            // Stdlib modules are precompiled separately; header-only modules (e.g. `layout`)
            // produce no object and reach the importer through the prepended interface.
            if resolved.is_stdlib
                || !hll_to_ir::imports::defines_code(&resolved.source)
                    .map_err(|msg| CompilationError::FreestandingErrors(vec![msg]))?
            {
                continue;
            }
            self.collect_import_closure(
                &resolved.key,
                &resolved.source,
                resolved.source_path,
                order,
                visited,
                stack,
                stack_keys,
                stack_sources,
            )?;
        }

        stack.pop();
        stack_keys.pop();
        stack_sources.pop();
        visited.insert(name.to_owned());
        order.push((name.to_owned(), source.to_owned(), source_path));
        Ok(())
    }

    /// Compile the stdlib for `mode` as independent per-module objects (no source
    /// concatenation), to link alongside the user program. Kernel gets its prefix.
    pub fn compile_stdlib_objects(
        mode: TargetMode,
    ) -> Result<Vec<(String, AssembledOutput)>, CompilationError> {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(mode);
        pipeline.set_write_artifacts(false);
        pipeline.set_string_prefix(Some(
            if mode == TargetMode::Kernel {
                "__kern_"
            } else {
                "__stdlib_"
            }
            .to_owned(),
        ));
        let modules = hll_to_ir::stdlib::get_stdlib_modules_for_mode(mode);
        let mut objects = Vec::with_capacity(modules.len());
        for (module_name, source) in &modules {
            // Attribute each module to its real source file so stdlib code the
            // debugger steps through maps back to workspace lines.
            let path = hll_to_ir::stdlib::stdlib_module_source_path(mode, module_name);
            pipeline.set_current_source_path(path);
            objects.extend(pipeline.compile_modules(&[(*module_name, *source)])?);
        }
        for object in &mut objects {
            mark_all_defined_symbols_global(object);
        }
        Ok(modules
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .zip(objects)
            .collect())
    }

    /// Compile the kernel modules reachable from `my_kernel` via the qualified-import closure.
    pub fn compile_kernel_module_objects()
    -> Result<Vec<(String, AssembledOutput)>, CompilationError> {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Kernel);
        pipeline.set_write_artifacts(false);
        pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
        pipeline.set_module_mangling(true);
        pipeline.set_current_source_path(bundled_module_path("kernel/core/my_kernel.hll"));
        let source = bundled_module_source("kernel/core/my_kernel.hll")
            .expect("generated registry contains the kernel root");
        pipeline.compile_program_closure("my_kernel", source)
    }
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    #[test]
    fn artifact_writes_are_opt_in() {
        assert!(!CompilationPipeline::new().write_artifacts);
        assert!(!CompilationPipeline::from_config(PipelineConfig::default()).write_artifacts);
    }
}
