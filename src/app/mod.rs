mod catalog;
mod panels;
mod settings;

use crate::machine_window::MachineWindow;
use asm_to_binary::AssembledOutput;
use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::rv_instruction::RvInstruction;
use egui::Color32;
use egui_dock::{DockState, NodeIndex};
use full_stack::compilation_pipeline::{
    AsmOutput, BinaryOutput, CompilationPipeline, FsEntry, IrOutput, PipelineResult, TargetMode,
    assembled_to_exec_file, build_fs_image,
};
use full_stack::target_mode::infer_target_mode_for_source;
use full_stack::view::debug::DebugSession;
use full_stack::view::ide::vm_execution_view::VmExecutionResult;
use full_stack::view::{
    AssemblyView, AstView, BgPreset, CacheView, CfgView, CompilationState, CompilerView,
    CpuStateView, DisassemblyView, ExecutionView, FramebufferView, IoView, IrView, MemoryView,
    PipelineView, ProgramCatalog, SourceView, StackView, TokensView, UiTheme, VmExecutionView,
    apply_ui_theme,
};
use hll_to_ir::stdlib::{
    get_kernel_stdlib_source, get_stdlib_source_for_mode, get_stdlib_type_prelude,
};
use std::fmt;
use virtual_machine::cpu::StepOutcome;
use virtual_machine::virtual_machine::VirtualMachine;

// --- Enums ---

#[derive(Default, Clone, PartialEq, Eq)]
enum AppMode {
    #[default]
    Ide,
    Debug,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum CatalogExportKind {
    #[default]
    Hll,
    Asm,
    Elf,
    Bin,
}

impl CatalogExportKind {
    fn label(self) -> &'static str {
        match self {
            Self::Hll => ".hll",
            Self::Asm => ".s",
            Self::Elf => ".elf",
            Self::Bin => ".bin",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Hll => "Path to .hll file",
            Self::Asm => "Path to .s file",
            Self::Elf => "Path to .elf file",
            Self::Bin => "Path to flat binary (.bin)",
        }
    }
}

// --- Settings ---

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
enum AccentPreset {
    #[default]
    Blue,
    Purple,
    Teal,
    Green,
    Rose,
    Amber,
}

impl AccentPreset {
    const ALL: &'static [Self] = &[
        Self::Blue,
        Self::Purple,
        Self::Teal,
        Self::Green,
        Self::Rose,
        Self::Amber,
    ];

    fn colors(self) -> (Color32, Color32) {
        match self {
            Self::Blue => (
                Color32::from_rgb(80, 120, 220),
                Color32::from_rgb(126, 104, 240),
            ),
            Self::Purple => (
                Color32::from_rgb(140, 90, 220),
                Color32::from_rgb(170, 120, 240),
            ),
            Self::Teal => (
                Color32::from_rgb(40, 180, 160),
                Color32::from_rgb(70, 210, 190),
            ),
            Self::Green => (
                Color32::from_rgb(50, 185, 90),
                Color32::from_rgb(80, 215, 130),
            ),
            Self::Rose => (
                Color32::from_rgb(215, 75, 110),
                Color32::from_rgb(235, 105, 145),
            ),
            Self::Amber => (
                Color32::from_rgb(215, 158, 40),
                Color32::from_rgb(235, 185, 70),
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::Rose => "Rose",
            Self::Amber => "Amber",
        }
    }

    fn theme(self) -> UiTheme {
        let (accent, accent_alt) = self.colors();
        UiTheme::dark().with_accent(accent, accent_alt)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
enum FontScale {
    Small,
    #[default]
    Medium,
    Large,
    Larger,
}

impl FontScale {
    fn zoom(self) -> f32 {
        match self {
            Self::Small => 0.85,
            Self::Medium => 1.0,
            Self::Large => 1.2,
            Self::Larger => 1.5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Small => "S",
            Self::Medium => "M",
            Self::Large => "L",
            Self::Larger => "XL",
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, PartialEq, Debug)]
#[serde(default)]
struct AppSettings {
    accent: AccentPreset,
    bg: BgPreset,
    font_scale: FontScale,
    max_vm_steps: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            accent: AccentPreset::default(),
            bg: BgPreset::default(),
            font_scale: FontScale::default(),
            max_vm_steps: 5_000_000,
        }
    }
}

// --- View wrapper (gives every tab a unique identity) ---

struct ViewWrapper {
    id: u64,
    view: Box<dyn CompilerView>,
}

impl Clone for ViewWrapper {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            view: self.view.clone_box(),
        }
    }
}

impl ViewWrapper {
    fn new(view: Box<dyn CompilerView>, counter: &mut u64) -> Self {
        let id = *counter;
        *counter += 1;
        Self { id, view }
    }
}

impl PartialEq for ViewWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Display for ViewWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.view.title())
    }
}

// --- Application state ---

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct FullStackApp {
    catalog: ProgramCatalog,
    #[serde(skip)]
    dock: DockState<ViewWrapper>,
    #[serde(skip)]
    debug_dock: DockState<ViewWrapper>,
    #[serde(skip)]
    compilation_state: CompilationState,
    #[serde(skip)]
    pipeline: CompilationPipeline,
    #[serde(skip)]
    rename_id: Option<String>,
    #[serde(skip)]
    rename_buffer: String,
    #[serde(skip)]
    import_disk_path: String,
    #[serde(skip)]
    export_disk_path: String,
    #[serde(skip)]
    export_kind: CatalogExportKind,
    #[serde(skip)]
    show_import_controls: bool,
    #[serde(skip)]
    show_export_controls: bool,
    #[serde(skip)]
    catalog_message: Option<String>,
    #[serde(skip)]
    pending_new_view: Option<ViewWrapper>,
    #[serde(skip)]
    next_view_id: u64,
    #[serde(skip)]
    mode: AppMode,
    #[serde(skip)]
    step_n_input: String,
    #[serde(skip)]
    stdlib_tokens: Vec<RvInstruction>,
    #[serde(skip)]
    stdlib_asm: String,
    #[serde(skip)]
    stdlib_objects: Vec<(String, AssembledOutput)>,
    #[serde(skip)]
    target_mode: TargetMode,
    #[serde(skip)]
    entry_point: String,
    #[serde(skip)]
    load_base_input: String,
    #[serde(skip)]
    machine_window: MachineWindow,
    #[serde(skip)]
    selected_inject_program_id: String,
    #[serde(skip)]
    kernel_binary: Option<AssembledOutput>,
    #[serde(skip)]
    shell_binary: Option<AssembledOutput>,
    #[serde(skip)]
    edit_binary: Option<AssembledOutput>,
    #[serde(skip)]
    fbdemo_binary: Option<AssembledOutput>,
    #[serde(skip)]
    cube_binary: Option<AssembledOutput>,
    #[serde(skip)]
    life_binary: Option<AssembledOutput>,
    #[serde(skip)]
    as_binary: Option<AssembledOutput>,
    #[serde(skip)]
    vm_output_view_id: u64,
    settings: AppSettings,
    #[serde(skip)]
    show_settings: bool,
    #[serde(skip)]
    user_set_target_mode: bool,
    #[serde(skip)]
    last_compile_program_id: String,
}

impl Default for FullStackApp {
    fn default() -> Self {
        let catalog = ProgramCatalog::default();
        let mut app = Self {
            catalog,
            dock: DockState::new(vec![]),
            debug_dock: DockState::new(vec![]),
            compilation_state: CompilationState::default(),
            pipeline: CompilationPipeline::new(),
            rename_id: None,
            rename_buffer: String::new(),
            import_disk_path: String::new(),
            export_disk_path: String::new(),
            export_kind: CatalogExportKind::default(),
            show_import_controls: false,
            show_export_controls: false,
            catalog_message: None,
            pending_new_view: None,
            next_view_id: 0,
            mode: AppMode::Ide,
            step_n_input: "1".to_owned(),
            stdlib_tokens: Vec::new(),
            stdlib_asm: String::new(),
            stdlib_objects: Vec::new(),
            target_mode: TargetMode::Hosted,
            entry_point: "kmain".to_owned(),
            load_base_input: format!("{:#010x}", LinkLayout::freestanding_kernel().load_base),
            machine_window: MachineWindow::default(),
            selected_inject_program_id: String::new(),
            kernel_binary: None,
            shell_binary: None,
            edit_binary: None,
            fbdemo_binary: None,
            cube_binary: None,
            life_binary: None,
            as_binary: None,
            vm_output_view_id: 0,
            settings: AppSettings::default(),
            show_settings: false,
            user_set_target_mode: false,
            last_compile_program_id: String::new(),
        };
        app.reset_layout();
        app.reset_debug_layout();
        app
    }
}

impl FullStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_ui_theme(&cc.egui_ctx);
        let mut app: Self = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        app.catalog.ensure_consistency();
        app.reset_layout();
        app.apply_settings(&cc.egui_ctx);
        app.init_stdlib_cache();
        app.compile();
        app
    }

    fn init_stdlib_cache(&mut self) {
        let mode = self.pipeline.target_mode();
        self.stdlib_tokens.clear();
        self.stdlib_asm.clear();

        // Compile each stdlib HLL independently so we get per-module artifacts with no concatenation.
        let modules = hll_to_ir::stdlib::get_stdlib_modules_for_mode(mode);
        let mut std_pipeline = CompilationPipeline::new();
        std_pipeline.set_target_mode(mode);
        std_pipeline.set_type_prelude(get_stdlib_type_prelude());
        if mode == TargetMode::Kernel {
            std_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
        }

        match std_pipeline
            .compile_modules(&modules.iter().map(|(n, s)| (*n, *s)).collect::<Vec<_>>())
        {
            Ok(objs) => {
                self.stdlib_objects = modules
                    .iter()
                    .map(|(n, _)| n.to_string())
                    .zip(objs.into_iter())
                    .collect();
            }
            Err(e) => {
                log::error!("stdlib multi-module compilation failed: {e:?}");
                self.stdlib_objects.clear();
            }
        }

        // Also compile the concatenated stdlib source for the token-level run_full() path used by hosted programs.
        let full_source = get_stdlib_source_for_mode(mode);
        std_pipeline.set_artifact_stem(Some("stdlib_concat".to_owned()));
        match std_pipeline.compile(&full_source) {
            Ok(result) => {
                let (_, tokens) =
                    std_pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
                self.stdlib_tokens = tokens;
                self.stdlib_asm = result.ir_program.to_string();
            }
            Err(e) => {
                log::error!("stdlib concatenated compile failed: {e:?}");
                self.stdlib_tokens.clear();
                self.stdlib_asm.clear();
            }
        }
    }

    fn view<T: CompilerView + Default + 'static>(&mut self) -> ViewWrapper {
        ViewWrapper::new(Box::new(T::default()), &mut self.next_view_id)
    }

    fn reset_layout(&mut self) {
        let views = vec![
            self.view::<SourceView>(),      // 0
            self.view::<TokensView>(),      // 1
            self.view::<AstView>(),         // 2
            self.view::<IrView>(),          // 3
            self.view::<AssemblyView>(),    // 4
            self.view::<CfgView>(),         // 5
            self.view::<StackView>(),       // 6
            self.view::<ExecutionView>(),   // 7
            self.view::<VmExecutionView>(), // 8
        ];

        self.vm_output_view_id = views[8].id;

        let mut dock = DockState::new(vec![views[0].clone()]);
        let surface = dock.main_surface_mut();
        let [_left, right] = surface.split_right(
            NodeIndex::root(),
            0.5,
            vec![views[3].clone(), views[1].clone(), views[2].clone()],
        );
        surface.split_below(
            right,
            0.5,
            vec![
                views[8].clone(), // VM Output - first so it's the default visible tab
                views[4].clone(), // Assembly
                views[5].clone(), // CFG
                views[6].clone(), // Stack
                views[7].clone(), // Execution (QEMU)
            ],
        );
        self.dock = dock;
    }

    fn focus_vm_output_tab(&mut self) {
        let needle = ViewWrapper {
            id: self.vm_output_view_id,
            view: Box::new(VmExecutionView::default()),
        };
        if let Some(tab_path) = self.dock.find_tab(&needle) {
            let _ = self.dock.set_active_tab(tab_path);
        }
    }

    fn reset_debug_layout(&mut self) {
        let cpu = self.view::<CpuStateView>();
        let pipeline = self.view::<PipelineView>();
        let cache = self.view::<CacheView>();
        let disasm = self.view::<DisassemblyView>();
        let fb = self.view::<FramebufferView>();
        let mem = self.view::<MemoryView>();
        let io = self.view::<IoView>();

        let mut dock = DockState::new(vec![disasm, cpu]);
        let surface = dock.main_surface_mut();

        let [left, _right] =
            surface.split_right(NodeIndex::root(), 0.4, vec![mem, pipeline, cache]);

        surface.split_below(left, 0.5, vec![fb]);
        surface.push_to_focused_leaf(io);

        self.debug_dock = dock;
    }

    fn set_target_mode(&mut self, mode: TargetMode) {
        if self.target_mode == mode {
            return;
        }
        self.target_mode = mode;
        self.pipeline.set_target_mode(mode);
        match mode {
            TargetMode::Hosted => {
                self.pipeline.set_entry_point(None);
                self.pipeline.set_link_layout(Some(LinkLayout::hosted()));
            }
            TargetMode::Kernel => {
                self.pipeline
                    .set_entry_point(Some("_kernel_start".to_owned()));
                let layout = LinkLayout::freestanding_kernel();
                self.load_base_input = format!("{:#010x}", layout.load_base);
                self.pipeline.set_link_layout(Some(layout));
            }
            TargetMode::Freestanding => {
                self.pipeline.set_entry_point(Some("_start".to_owned()));
                let kernel_default = LinkLayout::freestanding_kernel();
                if self.load_base_input.is_empty()
                    || parse_hex_or_dec(&self.load_base_input)
                        == Some(LinkLayout::hosted().load_base)
                {
                    self.load_base_input = format!("{:#010x}", kernel_default.load_base);
                }
                self.pipeline.set_link_layout(Some(kernel_default));
            }
        }
        self.init_stdlib_cache();
        self.compile();
    }

    fn enter_debug_mode(&mut self) {
        if let Some(assembled) = self.compilation_state.assembled() {
            let assembled = assembled.clone();
            let session = if self.target_mode == TargetMode::Kernel {
                DebugSession::new_kernel(&assembled)
            } else {
                let entry = self.compilation_state.entry_symbol.clone();
                let base = self.compilation_state.load_base;
                DebugSession::new(&assembled, base, &entry)
            };
            self.compilation_state.debug_session = Some(session);
            self.compilation_state.disasm_follow_pc = true;
            self.reset_debug_layout();
            self.mode = AppMode::Debug;
        }
    }

    fn exit_debug_mode(&mut self) {
        self.compilation_state.debug_session = None;
        self.mode = AppMode::Ide;
    }

    fn compile_kernel_with_modules(&mut self) {
        // Reuse cached per-module stdlib objects; rebuild if the cache is empty.
        if self.stdlib_objects.is_empty() {
            self.init_stdlib_cache();
        }
        if self.stdlib_objects.is_empty() {
            self.compilation_state
                .set_error("kernel stdlib cache is empty after module compilation".to_owned());
            self.compilation_state.just_compiled = false;
            return;
        }

        // Get the user's kernel module source.
        let user_source = self.catalog.get_selected_source();
        let module_name = self
            .catalog
            .current_program()
            .map(|p| p.name.trim().to_owned())
            .unwrap_or_else(|| "kernel".to_owned());

        // Compile the user kernel module in its own pipeline without concatenation.
        let mut kernel_user_pipeline = CompilationPipeline::new();
        kernel_user_pipeline.set_target_mode(TargetMode::Kernel);
        kernel_user_pipeline.set_type_prelude(get_stdlib_type_prelude());
        kernel_user_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
        kernel_user_pipeline.set_link_layout(Some(LinkLayout::freestanding_kernel()));
        let kernel_modules = vec![(&module_name as &str, user_source.as_str())];

        let kernel_objects = match kernel_user_pipeline.compile_modules(&kernel_modules) {
            Ok(objs) => objs,
            Err(e) => {
                self.compilation_state
                    .set_error(format!("kernel module compile error: {e}"));
                self.compilation_state.just_compiled = false;
                return;
            }
        };

        // Link kernel modules with stdlib at object level (no source concatenation).
        let all_names: Vec<&str> = self
            .stdlib_objects
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(kernel_modules.iter().map(|(n, _)| *n))
            .collect();
        let mut object_refs: Vec<&AssembledOutput> = Vec::new();
        object_refs.extend(self.stdlib_objects.iter().map(|(_, obj)| obj));
        for obj in &kernel_objects {
            object_refs.push(obj);
        }

        let final_assembled = match kernel_user_pipeline.link_assembled_objects_named(
            &all_names.join("_"),
            &all_names
                .iter()
                .zip(object_refs.iter())
                .map(|(n, o)| (*n, *o))
                .collect::<Vec<_>>(),
        ) {
            Ok(asm) => asm,
            Err(e) => {
                self.compilation_state
                    .set_error(format!("kernel link error: {}", e.message));
                self.compilation_state.just_compiled = false;
                return;
            }
        };

        // Prepend ROM firmware assembly so the disassembly view can follow the PC through boot.
        self.compilation_state.linked_asm_text =
            format!("{}{}", os_runtime::ROM_SOURCE, self.stdlib_asm);

        // Compile user source again for IR/ASM display; module objects are already built above.
        let (user_ir_display, user_asm_display) = match self.pipeline.compile(&user_source) {
            Ok(compile_result) => {
                let ir_display = compile_result.ir_program.to_string();
                let asm_display = self
                    .pipeline
                    .compile_ir_to_assembly(&compile_result.ir_program);
                (Some(ir_display), Some(asm_display))
            }
            Err(_) => (None, None),
        };

        let binary_out = BinaryOutput {
            assembled: final_assembled,
        };
        self.compilation_state.pipeline = Some(PipelineResult {
            diagnostics: vec![],
            lex: None,
            parse: None,
            ir: user_ir_display.map(|display| IrOutput { display }),
            asm: user_asm_display.map(|display| AsmOutput {
                tokens: vec![],
                display,
            }),
            binary: Some(binary_out),
            assembler_error: None,
            exec: None,
        });

        self.compilation_state.clear_error();
        self.compilation_state.just_compiled = true;
        // Cache the kernel binary so it survives target-mode switches.
        // Userspace programs use this cached kernel for "Run in Kernel".
        self.kernel_binary = self.compilation_state.assembled().cloned();
    }

    fn compile(&mut self) {
        let user_source = self.catalog.get_selected_source();
        let is_stdlib = self
            .catalog
            .current_program()
            .map(|p| p.is_stdlib() || p.standalone)
            .unwrap_or(false);

        let is_os_program = self
            .catalog
            .current_program()
            .map(|p| p.is_os() && !p.standalone)
            .unwrap_or(false);

        // Reset user-set target mode when the selected program changes.
        let current_id = self.catalog.selected_program_id.clone();
        if self.last_compile_program_id != current_id {
            self.user_set_target_mode = false;
            self.last_compile_program_id = current_id;
        }

        // Only auto-infer target mode if the user has not manually changed it.
        if !self.user_set_target_mode {
            let desired_mode = if is_os_program {
                TargetMode::Kernel
            } else {
                infer_target_mode_for_source(&user_source, is_stdlib, self.target_mode)
            };
            if desired_mode != self.target_mode {
                self.set_target_mode(desired_mode);
                return;
            }
        }

        self.pipeline.set_target_mode(self.target_mode);
        let artifact_stem = self
            .catalog
            .current_program()
            .map(|program| {
                let name = program.name.trim();
                if name.is_empty() {
                    program.id.clone()
                } else {
                    name.to_owned()
                }
            })
            .unwrap_or_else(|| "program".to_owned());
        self.pipeline.set_artifact_stem(Some(artifact_stem));
        match self.target_mode {
            TargetMode::Hosted => {
                self.pipeline.set_entry_point(None);
                self.pipeline.set_link_layout(Some(LinkLayout::hosted()));
            }
            TargetMode::Kernel => {
                self.pipeline
                    .set_entry_point(Some("_kernel_start".to_owned()));
                self.pipeline
                    .set_link_layout(Some(LinkLayout::freestanding_kernel()));
            }
            TargetMode::Freestanding => {
                self.pipeline.set_entry_point(Some("_start".to_owned()));
                let load_base = parse_hex_or_dec(&self.load_base_input)
                    .unwrap_or_else(|| LinkLayout::freestanding_kernel().load_base);
                let mut layout = LinkLayout::freestanding_kernel();
                layout.load_base = load_base;
                self.pipeline.set_link_layout(Some(layout));
            }
        }

        self.compilation_state.entry_symbol = self.pipeline.effective_entry_point().to_owned();
        self.compilation_state.load_base = self.pipeline.effective_load_base();

        // Kernel OS programs use multi-module compilation.
        if is_os_program && self.target_mode == TargetMode::Kernel {
            self.compile_kernel_with_modules();
            return;
        }

        let stdlib_tokens = if is_stdlib {
            None
        } else {
            Some(self.stdlib_tokens.as_slice())
        };
        let result = self.pipeline.run_full(&user_source, stdlib_tokens);

        if result.has_errors() {
            self.compilation_state
                .set_error(result.format_diagnostics());
            self.compilation_state.pipeline = Some(result);
            self.compilation_state.linked_asm_text = String::new();
            self.compilation_state.just_compiled = false;
        } else if !is_stdlib && let Some(ref asm_err) = result.assembler_error.clone() {
            self.compilation_state.set_error(format!("- {asm_err}"));
            self.compilation_state.linked_asm_text = result
                .asm
                .as_ref()
                .map(|a| format!("{}\n{}", self.stdlib_asm, a.display))
                .unwrap_or_default();
            self.compilation_state.pipeline = Some(result);
            self.compilation_state.just_compiled = false;
        } else {
            self.compilation_state.linked_asm_text = if is_stdlib {
                result
                    .asm
                    .as_ref()
                    .map(|a| a.display.clone())
                    .unwrap_or_default()
            } else {
                result
                    .asm
                    .as_ref()
                    .map(|a| format!("{}\n{}", self.stdlib_asm, a.display))
                    .unwrap_or_default()
            };
            self.compilation_state.pipeline = Some(result);
            self.compilation_state.clear_error();
            self.compilation_state.just_compiled = true;
        }
    }

    /// Compile the program with the given id as Hosted and store the result in
    /// `compilation_state.last_hosted_binary` on success.
    fn compile_and_store_hosted(&mut self, program_id: &str) -> Result<(), String> {
        // Find program source by id
        let program_opt = self
            .catalog
            .all_programs()
            .iter()
            .find(|p| p.id == program_id);
        let program = match program_opt {
            Some(p) => p,
            None => return Err(format!("program id not found: {program_id}")),
        };

        // Build pipeline and compile concatenated stdlib + program source
        let mut user_pipeline = CompilationPipeline::new();
        user_pipeline.set_target_mode(TargetMode::Hosted);
        user_pipeline.set_write_artifacts(false);
        user_pipeline.set_type_prelude(get_stdlib_type_prelude());

        let user_source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            program.source
        );
        let result = match user_pipeline.run_full(&user_source, None) {
            r if r.has_errors() => return Err(r.format_diagnostics()),
            r => r,
        };

        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }

        if let Some(bin) = result.binary.as_ref() {
            self.compilation_state.last_hosted_binary = Some(bin.assembled.clone());
            return Ok(());
        }

        Err("no assembled binary produced".to_owned())
    }

    /// Compile the default kernel (kernel stdlib + `my_kernel`) into `kernel_binary`,
    /// caching it. Independent of the catalog selection and target mode, so a
    /// userspace program can boot the kernel without disturbing the editor state.
    fn ensure_kernel_binary(&mut self) -> Result<(), String> {
        if self.kernel_binary.is_some() {
            return Ok(());
        }

        // Kernel stdlib: single concatenated source with the kernel string prefix.
        let mut stdlib_pipeline = CompilationPipeline::new();
        stdlib_pipeline.set_target_mode(TargetMode::Kernel);
        stdlib_pipeline.set_write_artifacts(false);
        stdlib_pipeline.set_string_prefix(Some("__kern_str_".to_owned()));
        let stdlib = stdlib_pipeline
            .compile(&get_kernel_stdlib_source())
            .map_err(|e| format!("kernel stdlib compile error: {e}"))?;
        let (_, stdlib_tokens) =
            stdlib_pipeline.compile_ir_to_assembly_with_tokens(&stdlib.ir_program);
        let stdlib_obj = stdlib_pipeline
            .assemble(&stdlib_tokens)
            .map_err(|e| format!("kernel stdlib assemble error: {e}"))?;

        // Kernel module, linked against the stdlib object at S-mode entry.
        let mut kernel_pipeline = CompilationPipeline::new();
        kernel_pipeline.set_target_mode(TargetMode::Kernel);
        kernel_pipeline.set_write_artifacts(false);
        kernel_pipeline.set_entry_point(Some("_kernel_start".to_owned()));
        let kernel_objs = kernel_pipeline
            .compile_modules(&[("my_kernel", os_runtime::kernel::MY_KERNEL)])
            .map_err(|e| format!("kernel module compile error: {e}"))?;

        let assembled = kernel_pipeline
            .link_assembled_objects_named(
                "kernel_stdlib_my_kernel",
                &[
                    ("kernel_stdlib", &stdlib_obj),
                    ("my_kernel", &kernel_objs[0]),
                ],
            )
            .map_err(|e| format!("kernel link error: {}", e.message))?;
        self.kernel_binary = Some(assembled);
        Ok(())
    }

    /// Compile the bundled interactive shell as a hosted binary, caching the
    /// result so repeated boots reuse it.
    fn ensure_shell_binary(&mut self) -> Result<(), String> {
        if self.shell_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::SHELL
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.shell_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("shell produced no binary".to_owned()),
        }
    }

    /// Compile the bundled line editor as a hosted binary, caching the result
    /// so repeated boots reuse it. Installed at `/bin/edit.fexe` by the boot image
    /// builder so the shell's `edit` command can exec it.
    fn ensure_edit_binary(&mut self) -> Result<(), String> {
        if self.edit_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::EDIT
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.edit_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("editor produced no binary".to_owned()),
        }
    }

    /// Compile the bundled framebuffer demo as a hosted binary, caching it.
    fn ensure_fbdemo_binary(&mut self) -> Result<(), String> {
        if self.fbdemo_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::FBDEMO
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.fbdemo_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("fbdemo produced no binary".to_owned()),
        }
    }

    /// Compile the bundled spinning-cube demo as a hosted binary, caching it.
    fn ensure_cube_binary(&mut self) -> Result<(), String> {
        if self.cube_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::CUBE
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.cube_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("cube produced no binary".to_owned()),
        }
    }

    /// Compile the bundled Game of Life demo as a hosted binary, caching it.
    fn ensure_life_binary(&mut self) -> Result<(), String> {
        if self.life_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::LIFE
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.life_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("life produced no binary".to_owned()),
        }
    }

    /// Compile the bundled in-VM assembler as a hosted binary, caching it.
    /// Installed at `/bin/as.fexe` so the shell's `as <src> <out>` command works.
    fn ensure_as_binary(&mut self) -> Result<(), String> {
        if self.as_binary.is_some() {
            return Ok(());
        }
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_write_artifacts(false);
        pipeline.set_type_prelude(get_stdlib_type_prelude());

        let source = format!(
            "{}\n{}",
            get_stdlib_source_for_mode(TargetMode::Hosted),
            os_runtime::user::AS
        );
        let result = pipeline.run_full(&source, None);
        if result.has_errors() {
            return Err(result.format_diagnostics());
        }
        if let Some(ref asm_err) = result.assembler_error {
            return Err(format!("assembler error: {asm_err}"));
        }
        match result.binary.as_ref() {
            Some(bin) => {
                self.as_binary = Some(bin.assembled.clone());
                Ok(())
            }
            None => Err("assembler produced no binary".to_owned()),
        }
    }

    /// Build the filesystem image the shell boots with: a `/home` directory and,
    /// if a program is selected for injection, that program stored there as a
    /// runnable executable file. A short readme is always present so `ls` has
    /// something to show.
    fn build_boot_fs_image(&self) -> Vec<u8> {
        let mut entries = boot_fs_static_entries();

        // Install the line editor at /bin/edit.fexe so the shell's `edit` command
        // can exec it. Without this the editor is compiled but never reachable.
        let edit_holder;
        if let Some(asm) = self.edit_binary.as_ref() {
            edit_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: "/bin/edit.fexe",
                data: &edit_holder,
            });
        }

        // Install the Mandelbrot framebuffer demo at /home/demo/mandelbrot.fexe so
        // a bare `mandelbrot` paints the framebuffer device.
        let fbdemo_holder;
        if let Some(asm) = self.fbdemo_binary.as_ref() {
            fbdemo_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: "/home/demo/mandelbrot.fexe",
                data: &fbdemo_holder,
            });
        }

        // Install the spinning-cube demo at /home/demo/cube.fexe so a bare `cube`
        // animates a rotating wireframe on the framebuffer device.
        let cube_holder;
        if let Some(asm) = self.cube_binary.as_ref() {
            cube_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: "/home/demo/cube.fexe",
                data: &cube_holder,
            });
        }

        // Install Conway's Game of Life at /home/demo/life.fexe so a bare `life`
        // animates a cellular automaton on the framebuffer device.
        let life_holder;
        if let Some(asm) = self.life_binary.as_ref() {
            life_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: "/home/demo/life.fexe",
                data: &life_holder,
            });
        }

        // Install the in-VM assembler at /bin/as.fexe so `as <src> <out>` works.
        let as_holder;
        if let Some(asm) = self.as_binary.as_ref() {
            as_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: "/bin/as.fexe",
                data: &as_holder,
            });
        }

        // If a program is selected, write it into /home as an executable file.
        let exec_holder;
        let path_holder;
        if let Some(asm) = self.compilation_state.last_hosted_binary.as_ref() {
            let name = self
                .catalog
                .all_programs()
                .iter()
                .find(|p| p.id == self.selected_inject_program_id)
                .map(|p| sanitize_program_filename(&p.name))
                .unwrap_or_else(|| "program".to_owned());
            path_holder = format!("/home/{name}.fexe");
            exec_holder = assembled_to_exec_file(asm);
            entries.push(FsEntry::File {
                path: &path_holder,
                data: &exec_holder,
            });
        }

        build_fs_image(&entries)
    }

    fn save_state(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

// --- eframe::App ---

impl eframe::App for FullStackApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(view) = self.pending_new_view.take() {
            match self.mode {
                AppMode::Ide => self.dock.main_surface_mut().push_to_focused_leaf(view),
                AppMode::Debug => self
                    .debug_dock
                    .main_surface_mut()
                    .push_to_focused_leaf(view),
            }
        }

        match self.mode {
            AppMode::Ide => {
                egui::Panel::left("left_panel")
                    .resizable(true)
                    .default_size(220.0)
                    .show_inside(ui, |ui| self.catalog_ui(ui));

                egui::Panel::top("top_panel")
                    .resizable(false)
                    .exact_size(40.0)
                    .show_inside(ui, |ui| self.ide_top_bar(ui));

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui_dock::DockArea::new(&mut self.dock)
                        .show_add_buttons(false)
                        .show_close_buttons(true)
                        .show_inside(
                            ui,
                            &mut DockTabViewer {
                                state: &mut self.compilation_state,
                                catalog: &mut self.catalog,
                            },
                        );
                });
            }
            AppMode::Debug => {
                egui::Panel::top("debug_top_panel")
                    .resizable(false)
                    .exact_size(40.0)
                    .show_inside(ui, |ui| self.debug_top_bar(ui));

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui_dock::DockArea::new(&mut self.debug_dock)
                        .show_add_buttons(false)
                        .show_close_buttons(true)
                        .show_inside(
                            ui,
                            &mut DockTabViewer {
                                state: &mut self.compilation_state,
                                catalog: &mut self.catalog,
                            },
                        );
                });
            }
        }

        let has_kernel = self.target_mode == TargetMode::Kernel
            && self.compilation_state.assembled().is_some()
            && self.compilation_state.error_summary.is_none();
        let mut mw_open = self.machine_window.open;
        egui::Window::new("Machine")
            .open(&mut mw_open)
            .default_size([720.0, 480.0])
            .min_size([600.0, 350.0])
            .max_size([900.0, 700.0])
            .resizable(true)
            .show(ui.ctx(), |ui| {
                ui.set_max_width(900.0);
                // Booting drops into an interactive shell (ls / cd / cat / exit).
                // The selected program, if any, is placed in /home as a runnable
                // file you can launch by typing its name (bare-name execution).
                ui.horizontal(|ui| {
                    ui.label("Program in /home:");
                    // Program selector: list Example, Custom, and Userspace programs
                    // (all hosted -- they install into /home and run under the shell).
                    use full_stack::view::ProgramKind;
                    let programs: Vec<_> = self
                        .catalog
                        .all_programs()
                        .iter()
                        .filter(|p| {
                            p.kind == ProgramKind::Example
                                || p.kind == ProgramKind::Custom
                                || p.kind == ProgramKind::User
                        })
                        .cloned()
                        .collect();

                    let mut selected_label = "None".to_owned();
                    if !self.selected_inject_program_id.is_empty() {
                        if let Some(p) = programs
                            .iter()
                            .find(|p| p.id == self.selected_inject_program_id)
                        {
                            selected_label = p.name.clone();
                        }
                    }

                    egui::ComboBox::from_id_salt("inject_program_list")
                        .selected_text(selected_label.clone())
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.selected_inject_program_id.is_empty(),
                                    "None",
                                )
                                .clicked()
                            {
                                self.selected_inject_program_id.clear();
                            }
                            for p in &programs {
                                let is_selected = self.selected_inject_program_id == p.id;
                                if ui.selectable_label(is_selected, &p.name).clicked() {
                                    self.selected_inject_program_id = p.id.clone();
                                    // Compile the selected program into last_hosted_binary.
                                    match self.compile_and_store_hosted(&p.id) {
                                        Ok(()) => {
                                            // Enable the injection flag on the machine window.
                                            self.machine_window.selected_user_inject = true;
                                        }
                                        Err(e) => {
                                            self.compilation_state
                                                .set_error(format!("hosted compile failed: {e}"));
                                        }
                                    }
                                }
                            }
                        });

                    if self.selected_inject_program_id.is_empty() {
                        ui.colored_label(full_stack::view::ui_theme().text_dim, "(none selected)");
                    } else if let Some(ref asm) = self.compilation_state.last_hosted_binary {
                        let size = asm.to_flat_binary().len();
                        ui.label(format!("size: {size} bytes"));
                    }
                });

                self.machine_window.ui(ui, has_kernel);
            });
        self.machine_window.open = mw_open;

        if self.machine_window.boot_requested {
            self.machine_window.boot_requested = false;
            // When autorun is requested, use the cached kernel binary instead
            // of the currently-selected userspace program's assembled output.
            let kernel = if self.machine_window.autorun_requested {
                self.kernel_binary.clone()
            } else {
                self.compilation_state.assembled().cloned()
            };
            if let Some(assembled) = kernel {
                // Boot the interactive shell as pid 1, with a filesystem image
                // that holds any selected program as a runnable file.
                match self.ensure_shell_binary() {
                    Ok(()) => {
                        // Best-effort: install the editor so `edit` works. A
                        // failure here should not stop the shell from booting.
                        if let Err(e) = self.ensure_edit_binary() {
                            self.compilation_state
                                .set_error(format!("editor compile failed: {e}"));
                        }
                        // Best-effort: install the Mandelbrot demo for `mandelbrot`.
                        if let Err(e) = self.ensure_fbdemo_binary() {
                            self.compilation_state
                                .set_error(format!("fbdemo compile failed: {e}"));
                        }
                        // Best-effort: install the spinning-cube demo for `cube`.
                        if let Err(e) = self.ensure_cube_binary() {
                            self.compilation_state
                                .set_error(format!("cube compile failed: {e}"));
                        }
                        // Best-effort: install the Game of Life demo for `life`.
                        if let Err(e) = self.ensure_life_binary() {
                            self.compilation_state
                                .set_error(format!("life compile failed: {e}"));
                        }
                        // Best-effort: install the in-VM assembler for `as <src> <out>`.
                        if let Err(e) = self.ensure_as_binary() {
                            self.compilation_state
                                .set_error(format!("assembler compile failed: {e}"));
                        }
                        let fs_image = self.build_boot_fs_image();
                        let shell = self.shell_binary.clone();
                        // The shell idles waiting for keystrokes; give it a large
                        // step budget so the session does not time out between
                        // inputs (it ends when the user types `exit`).
                        let max_steps = self.settings.max_vm_steps.max(1).saturating_mul(1000);

                        // Build the autorun command if Boot & Run was requested.
                        let autorun = if self.machine_window.autorun_requested
                            && !self.selected_inject_program_id.is_empty()
                        {
                            let name = self
                                .catalog
                                .all_programs()
                                .iter()
                                .find(|p| p.id == self.selected_inject_program_id)
                                .map(|p| sanitize_program_filename(&p.name))
                                .unwrap_or_else(|| "program".to_owned());
                            Some(format!("run /home/{name}.fexe"))
                        } else {
                            None
                        };

                        self.machine_window.start_boot(
                            &assembled,
                            shell.as_ref(),
                            Some(&fs_image),
                            max_steps,
                            autorun.as_deref(),
                        );
                    }
                    Err(e) => {
                        self.compilation_state
                            .set_error(format!("shell compile failed: {e}"));
                    }
                }
            }
        }

        let mut show_settings = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut show_settings)
            .resizable(false)
            .default_width(280.0)
            .default_pos(egui::pos2(ui.ctx().content_rect().right() - 300.0, 54.0))
            .show(ui.ctx(), |ui| {
                self.settings_window_ui(ui);
            });
        self.show_settings = show_settings;
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
    }
}

// --- DockTabViewer ---

struct DockTabViewer<'a> {
    state: &'a mut CompilationState,
    catalog: &'a mut ProgramCatalog,
}

impl egui_dock::TabViewer for DockTabViewer<'_> {
    type Tab = ViewWrapper;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.view.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let ctx = ui.ctx().clone();
        tab.view.ui(ui, &ctx, self.state, self.catalog);
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }
}

// --- Helpers ---

fn parse_hex_or_dec(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// Turn a program display name into a lowercase, filesystem-safe stem.
/// Non-alphanumeric characters become underscores; empty results fall back
/// to "program".
fn sanitize_program_filename(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = mapped.trim_matches('_');
    if trimmed.is_empty() {
        "program".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The always-present part of the boot filesystem image: the Unix-shaped
/// directory tree (`/bin`, `/home`, `/home/demo`, `/home/src`), a readme, and the
/// example assembly sources. Compiled binaries (editor, demos, assembler) are
/// appended by [`FullStackApp::build_boot_fs_image`] when available. See PLAN 2.1.
fn boot_fs_static_entries() -> Vec<FsEntry<'static>> {
    // Bare-name execution finds demos on PATH; the readme points at the layout.
    let readme: &[u8] = b"Full-Stack OS. Type 'help' for the full command list.\n\
\n\
  ls [dir]      list a directory       cat <file>...  print files\n\
  echo <text>   print text             pwd            show cwd\n\
  cd <dir>      change directory       edit <file>    edit a file\n\
\n\
Run a program by typing its name (PATH is . then /bin then /home/demo):\n\
  cube          spinning wireframe     mandelbrot     fractal\n\
  life          game of life\n\
Add '&' to run in the background; jobs / fg <job> / kill <pid> manage jobs.\n\
\n\
Send output to a file:  echo hi > note.txt   cat a b >> log.txt\n\
Pipe programs:          cat a | filter      (up to 4 stages)\n\
Assemble:  as /home/src/fib.s /home/fib.fexe  then run it with  fib\n\
Examples in /home/src: sum.s fib.s array.s (array.s sums a stack array -> 42)\n";

    vec![
        FsEntry::Dir { path: "/bin" },
        FsEntry::Dir { path: "/home" },
        FsEntry::Dir { path: "/home/demo" },
        FsEntry::Dir { path: "/home/src" },
        FsEntry::File {
            path: "/readme.txt",
            data: readme,
        },
        // Example assembly sources so `as` can be tried immediately.
        FsEntry::File {
            path: "/home/src/sum.s",
            data: os_runtime::user::EXAMPLE_SUM_S.as_bytes(),
        },
        FsEntry::File {
            path: "/home/src/fib.s",
            data: os_runtime::user::EXAMPLE_FIB_S.as_bytes(),
        },
        FsEntry::File {
            path: "/home/src/array.s",
            data: os_runtime::user::EXAMPLE_ARRAY_S.as_bytes(),
        },
    ]
}

fn run_in_vm(
    assembled: &AssembledOutput,
    entry_symbol: &str,
    load_base: u64,
    max_steps: u64,
) -> VmExecutionResult {
    let elf = assembled.to_elf_with_entry(load_base, entry_symbol);
    let mut vm = match VirtualMachine::from_elf(&elf) {
        Ok(vm) => vm,
        Err(err) => {
            return VmExecutionResult {
                uart_output: format!("ELF load failed: {err}"),
                exit_code: None,
                steps: 0,
                max_steps_reached: false,
            };
        }
    };
    let result = vm.run(max_steps);

    VmExecutionResult {
        uart_output: result.uart_output,
        exit_code: match result.outcome {
            StepOutcome::Halted(code) => Some(code as i32),
            _ => None,
        },
        steps: result.steps,
        max_steps_reached: matches!(result.outcome, StepOutcome::Continue),
    }
}

#[cfg(test)]
mod boot_fs_tests {
    use super::*;

    // PLAN 2.1: the boot image is laid out Unix-style -- tools in /bin, demos in
    // /home/demo, example sources in /home/src -- not all dumped in /bin or /home.
    #[test]
    fn boot_fs_static_layout_is_unix_shaped() {
        let entries = boot_fs_static_entries();

        let dirs: Vec<&str> = entries
            .iter()
            .filter_map(|e| match e {
                FsEntry::Dir { path } => Some(*path),
                FsEntry::File { .. } => None,
            })
            .collect();
        let files: Vec<&str> = entries
            .iter()
            .filter_map(|e| match e {
                FsEntry::File { path, .. } => Some(*path),
                FsEntry::Dir { .. } => None,
            })
            .collect();

        for dir in ["/bin", "/home", "/home/demo", "/home/src"] {
            assert!(
                dirs.contains(&dir),
                "missing directory {dir}; dirs={dirs:?}"
            );
        }
        // Example sources moved under /home/src, no longer loose in /home.
        assert!(
            files.contains(&"/home/src/sum.s"),
            "sum.s not under /home/src; files={files:?}"
        );
        assert!(
            files.contains(&"/home/src/fib.s"),
            "fib.s not under /home/src; files={files:?}"
        );
        assert!(
            files.contains(&"/home/src/array.s"),
            "array.s not under /home/src; files={files:?}"
        );
        assert!(
            !files.contains(&"/home/sum.s"),
            "sum.s still loose in /home; files={files:?}"
        );
        assert!(
            !files.contains(&"/home/fib.s"),
            "fib.s still loose in /home; files={files:?}"
        );
        assert!(
            files.contains(&"/readme.txt"),
            "readme missing; files={files:?}"
        );
    }
}
