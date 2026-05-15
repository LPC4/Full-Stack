use crate::assembly_language::assembler::link_layout::LinkLayout;
use crate::high_level_language::compilation_pipeline::{CompilationPipeline, TargetMode};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::stdlib::get_stdlib_source_for_mode;
use crate::high_level_language::token::Token;
use crate::view::debug::{DebugSession, SessionStatus};
use crate::view::ide::vm_execution_view::VmExecutionResult;
use crate::view::{
    AssemblyView, AstView, CacheView, CfgView, CompilationState, CompilerView, CpuStateView,
    DisassemblyView, ExecutionView, FramebufferView, IoView, IrView, MemoryMapView, MemoryView,
    PipelineView, ProgramCatalog, ProgramKind, SourceView, StackView, TokensView, VmExecutionView,
    apply_ui_theme, blank_custom_program_source, ui_theme,
};
use egui::{Color32, Layout, RichText};
use egui_dock::{DockState, NodeIndex};
use std::fmt;
use std::fs;
use std::path::Path;

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

// ------------------------------------------------------------
// Unique wrapper so every tab has its own identity
// ------------------------------------------------------------
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

// ------------------------------------------------------------
// Application state
// ------------------------------------------------------------
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
    stdlib_tokens: Vec<crate::assembly_language::rv_instruction::RvInstruction>,
    #[serde(skip)]
    stdlib_asm: String,
    #[serde(skip)]
    target_mode: TargetMode,
    #[serde(skip)]
    entry_point: String,
    #[serde(skip)]
    load_base_input: String,
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
            target_mode: TargetMode::Hosted,
            entry_point: "kmain".to_owned(),
            load_base_input: format!("{:#010x}", LinkLayout::freestanding_kernel().load_base),
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
        app.init_stdlib_cache();
        app.compile();
        app
    }

    fn init_stdlib_cache(&mut self) {
        // Compile the stdlib appropriate for the current target mode.
        // The pipeline's target_mode is set before this is called.
        let stdlib_src = get_stdlib_source_for_mode(self.pipeline.target_mode);
        match self.pipeline.compile(&stdlib_src) {
            Ok(result) => {
                let (asm_text, tokens) = self
                    .pipeline
                    .compile_ir_to_assembly_with_tokens(&result.ir_program);
                self.stdlib_tokens = tokens;
                self.stdlib_asm = asm_text;
            }
            Err(e) => {
                log::error!("stdlib compilation failed: {e}");
            }
        }
    }

    fn view<T: CompilerView + Default + 'static>(&mut self) -> ViewWrapper {
        ViewWrapper::new(Box::new(T::default()), &mut self.next_view_id)
    }

    fn reset_layout(&mut self) {
        let views = vec![
            self.view::<SourceView>(),
            self.view::<TokensView>(),
            self.view::<AstView>(),
            self.view::<IrView>(),
            self.view::<AssemblyView>(),
            self.view::<CfgView>(),
            self.view::<StackView>(),
            self.view::<MemoryMapView>(),
            self.view::<ExecutionView>(),
            self.view::<VmExecutionView>(),
        ];

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
                views[4].clone(),
                views[5].clone(),
                views[6].clone(),
                views[8].clone(),
                views[9].clone(),
            ],
        );
        self.dock = dock;
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

    /// Switch target mode, re-cache the stdlib for the new mode, and recompile.
    fn set_target_mode(&mut self, mode: TargetMode) {
        if self.target_mode == mode {
            return;
        }
        self.target_mode = mode;
        self.pipeline.target_mode = mode;
        // Update the entry point field to the mode default when switching,
        // unless the user has already overridden it for that mode.
        if mode == TargetMode::Hosted {
            self.pipeline.entry_point = None; // use "_start"
            self.pipeline.link_layout = Some(LinkLayout::hosted());
        } else {
            // Keep whatever the user typed; default is "kmain".
            let ep = if self.entry_point.trim().is_empty() {
                "kmain".to_owned()
            } else {
                self.entry_point.clone()
            };
            self.pipeline.entry_point = Some(ep);
            // Reset load base to kernel default if the user hasn't customised it.
            let kernel_default = LinkLayout::freestanding_kernel();
            if self.load_base_input.is_empty()
                || parse_hex_or_dec(&self.load_base_input)
                    == Some(LinkLayout::hosted().load_base)
            {
                self.load_base_input = format!("{:#010x}", kernel_default.load_base);
            }
            self.pipeline.link_layout = Some(kernel_default);
        }
        self.init_stdlib_cache();
        self.compile();
    }

    fn enter_debug_mode(&mut self) {
        if let Some(assembled) = &self.compilation_state.assembled {
            let entry = self.compilation_state.entry_symbol.clone();
            let base = self.compilation_state.load_base;
            self.compilation_state.debug_session =
                Some(DebugSession::new(assembled, base, &entry));
            self.compilation_state.disasm_follow_pc = true;
            self.reset_debug_layout();
            self.mode = AppMode::Debug;
        }
    }

    fn exit_debug_mode(&mut self) {
        self.compilation_state.debug_session = None;
        self.mode = AppMode::Ide;
    }

    fn compile(&mut self) {
        // Sync pipeline config with current UI state.
        self.pipeline.target_mode = self.target_mode;
        if self.target_mode == TargetMode::Freestanding {
            let ep = self.entry_point.trim().to_owned();
            self.pipeline.entry_point = Some(if ep.is_empty() {
                "kmain".to_owned()
            } else {
                ep
            });
        } else {
            self.pipeline.entry_point = None;
        }

        // Sync link layout from UI state.
        if self.target_mode == TargetMode::Freestanding {
            let load_base = parse_hex_or_dec(&self.load_base_input)
                .unwrap_or_else(|| LinkLayout::freestanding_kernel().load_base);
            let mut layout = LinkLayout::freestanding_kernel();
            layout.load_base = load_base;
            self.pipeline.link_layout = Some(layout);
        } else {
            self.pipeline.link_layout = Some(LinkLayout::hosted());
        }

        // Keep entry_symbol and load_base in sync so VM and ELF export always use the right values.
        self.compilation_state.entry_symbol =
            self.pipeline.effective_entry_point().to_owned();
        self.compilation_state.load_base = self.pipeline.effective_load_base();

        let user_source = &self.catalog.get_selected_source();
        let is_stdlib = self
            .catalog
            .current_program()
            .map(|p| p.is_stdlib())
            .unwrap_or(false);

        // For stdlib programs, just compile and display them
        if is_stdlib {
            let mut lexer = Lexer::new(user_source);
            let mut tokens = Vec::new();

            loop {
                let token = lexer.next_token();
                if let Token::Error(message) = &token {
                    self.compilation_state.tokens = format!("LEXER ERROR: {message}");
                    self.compilation_state
                        .set_error(format!("Lexer error: {message}"));
                    self.compilation_state.just_compiled = false;
                    return;
                }
                let is_eof = matches!(token, Token::Eof);
                tokens.push(token);
                if is_eof {
                    break;
                }
            }

            self.compilation_state.tokens = format!("{tokens:#?}");

            match self.pipeline.compile(user_source) {
                Ok(result) => {
                    self.compilation_state.ast = format!("{:#?}", result.ast);
                    self.compilation_state.ir = result.ir_program.to_string();
                    let (asm_text, asm_tokens) = self
                        .pipeline
                        .compile_ir_to_assembly_with_tokens(&result.ir_program);
                    self.compilation_state.linked_asm = asm_text.clone();
                    self.compilation_state.asm = asm_text;
                    self.compilation_state.assembled = self.pipeline.assemble(&asm_tokens).ok();
                    self.compilation_state.assembly_tokens = asm_tokens;
                    self.compilation_state.clear_error();
                    self.compilation_state.just_compiled = true;
                }
                Err(error) => {
                    self.compilation_state.set_error(error.to_string());
                    self.compilation_state.just_compiled = false;
                }
            }
            return;
        }

        // For user programs: compile WITHOUT stdlib for IR/ASM views
        let mut lexer = Lexer::new(user_source);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token();
            if let Token::Error(message) = &token {
                self.compilation_state.tokens = format!("LEXER ERROR: {message}");
                self.compilation_state
                    .set_error(format!("Lexer error: {message}"));
                self.compilation_state.just_compiled = false;
                return;
            }
            let is_eof = matches!(token, Token::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        self.compilation_state.tokens = format!("{tokens:#?}");

        // Compile user code; IR/ASM panels show user-only code.
        // Execution uses token-level linking with cached stdlib tokens.
        match self.pipeline.compile(user_source) {
            Ok(result) => {
                self.compilation_state.ast = format!("{:#?}", result.ast);
                self.compilation_state.ir = result.ir_program.to_string();
                let (asm_text, user_tokens) = self
                    .pipeline
                    .compile_ir_to_assembly_with_tokens(&result.ir_program);
                self.compilation_state.asm = asm_text.clone();
                self.compilation_state.linked_asm =
                    format!("{}\n{}", self.stdlib_asm, asm_text);
                self.compilation_state.assembly_tokens = user_tokens.clone();

                // Token-level link: prepend cached stdlib tokens, then assemble once.
                let mut linked = self.stdlib_tokens.clone();
                linked.extend(user_tokens);
                self.compilation_state.assembled =
                    self.pipeline.assemble_linked(&linked).ok();

                self.compilation_state.clear_error();
                self.compilation_state.just_compiled = true;
            }
            Err(error) => {
                self.compilation_state.set_error(error.to_string());
                self.compilation_state.just_compiled = false;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_selected_output_to_disk(&mut self) {
        let path = self.export_disk_path.trim().to_owned();
        if path.is_empty() {
            self.catalog_message = Some("enter a file path to export the selected file".to_owned());
            return;
        }

        let result = match self.export_kind {
            CatalogExportKind::Hll => {
                let Some(program) = self.catalog.current_program() else {
                    self.catalog_message = Some("no program selected".to_owned());
                    return;
                };

                fs::write(&path, &program.source)
                    .map(|_| format!("exported `{}` to `{path}`", program.name))
            }
            CatalogExportKind::Asm => {
                if self.compilation_state.assembled.is_none() {
                    self.catalog_message =
                        Some("compile successfully before exporting assembly".to_owned());
                    return;
                }

                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting assembly".to_owned());
                    return;
                }

                fs::write(&path, self.compilation_state.asm.as_bytes())
                    .map(|_| format!("exported assembly to `{path}`"))
            }
            CatalogExportKind::Elf => {
                let Some(assembled) = self.compilation_state.assembled.as_ref() else {
                    self.catalog_message =
                        Some("compile successfully before exporting an ELF image".to_owned());
                    return;
                };

                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting an ELF image".to_owned());
                    return;
                }

                let entry = &self.compilation_state.entry_symbol;
                let base = self.compilation_state.load_base;
                fs::write(&path, assembled.to_elf_with_entry(base, entry))
                    .map(|_| format!("exported ELF image to `{path}` (load base {base:#010x})"))
            }

            CatalogExportKind::Bin => {
                let Some(assembled) = self.compilation_state.assembled.as_ref() else {
                    self.catalog_message =
                        Some("compile successfully before exporting a flat binary".to_owned());
                    return;
                };

                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting a flat binary".to_owned());
                    return;
                }

                fs::write(&path, assembled.to_flat_binary())
                    .map(|_| format!("exported flat binary to `{path}`"))
            }
        };

        match result {
            Ok(message) => self.catalog_message = Some(message),
            Err(err) => {
                self.catalog_message = Some(format!("failed to export to `{path}`: {err}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_program_from_disk(&mut self) {
        let path = self.import_disk_path.trim().to_owned();
        if path.is_empty() {
            self.catalog_message = Some("enter a file path to import a program".to_owned());
            return;
        }

        match fs::read_to_string(&path) {
            Ok(source) => {
                let name = Path::new(&path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .filter(|stem| !stem.trim().is_empty())
                    .map(|stem| stem.to_owned())
                    .unwrap_or_else(|| String::from("Imported Program"));
                self.catalog.create_custom_program(source, name.clone());
                self.rename_id = None;
                self.catalog_message = Some(format!("imported `{name}` from `{path}`"));
                self.compile();
            }
            Err(err) => {
                self.catalog_message = Some(format!("failed to import from `{path}`: {err}"));
            }
        }
    }

    fn catalog_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Files");
            ui.add_space(6.0);
            ui.small("Examples are embedded; your files stay in memory.");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("New File").clicked() {
                    self.catalog.create_blank_program();
                    self.rename_id = None;
                    self.compile();
                }
                if ui.button("Duplicate").clicked() {
                    self.catalog.duplicate_current_program();
                    self.rename_id = None;
                    self.compile();
                }
            });

            #[cfg(not(target_arch = "wasm32"))]
            ui.horizontal(|ui| {
                if ui
                    .button("Import")
                    .on_hover_text("Import a .hll file from disk")
                    .clicked()
                {
                    self.show_import_controls = !self.show_import_controls;
                    if self.show_import_controls {
                        self.show_export_controls = false;
                    }
                }
                if ui
                    .button("Export")
                    .on_hover_text("Export the current program, assembly, or ELF image")
                    .clicked()
                {
                    self.show_export_controls = !self.show_export_controls;
                    if self.show_export_controls {
                        self.show_import_controls = false;
                    }
                }
            });

            #[cfg(not(target_arch = "wasm32"))]
            {
                if self.show_import_controls {
                    ui.separator();
                    ui.small("Import a .hll file from disk:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_disk_path)
                            .hint_text("Path to .hll file")
                            .desired_width(f32::INFINITY),
                    );
                    let path_ready = !self.import_disk_path.trim().is_empty();
                    if ui
                        .add_enabled(path_ready, egui::Button::new("Import .hll"))
                        .clicked()
                    {
                        self.import_program_from_disk();
                    }
                }

                if self.show_export_controls {
                    ui.separator();
                    ui.small("Export the current program, assembly, or ELF image:");
                    ui.horizontal(|ui| {
                        ui.label("Format:");
                        egui::ComboBox::from_id_salt("catalog_export_format")
                            .selected_text(self.export_kind.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Hll,
                                    ".hll",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Asm,
                                    ".s",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Elf,
                                    ".elf",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Bin,
                                    ".bin (flat binary)",
                                );
                            });
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_disk_path)
                            .hint_text(self.export_kind.hint())
                            .desired_width(f32::INFINITY),
                    );
                    let path_ready = !self.export_disk_path.trim().is_empty();
                    let can_export = path_ready
                        && match self.export_kind {
                            CatalogExportKind::Hll => self.catalog.current_program().is_some(),
                            CatalogExportKind::Asm
                            | CatalogExportKind::Elf
                            | CatalogExportKind::Bin => {
                                self.compilation_state.just_compiled
                                    && self.compilation_state.assembled.is_some()
                            }
                        };
                    let export_label = match self.export_kind {
                        CatalogExportKind::Hll => "Export .hll",
                        CatalogExportKind::Asm => "Export .s",
                        CatalogExportKind::Elf => "Export .elf",
                        CatalogExportKind::Bin => "Export .bin",
                    };
                    if ui
                        .add_enabled(can_export, egui::Button::new(export_label))
                        .clicked()
                    {
                        self.export_selected_output_to_disk();
                    }
                }

                if let Some(message) = &self.catalog_message {
                    ui.small(message);
                }
            }

            ui.add_space(8.0);
            self.render_program_section(ui, ProgramKind::Stdlib, "Standard Library");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Example, "Examples");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Custom, "Your programs");

            let is_custom = self
                .catalog
                .current_program()
                .map(|p| p.is_custom())
                .unwrap_or(false);
            if is_custom && ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.catalog.delete_current_custom_program();
                self.rename_id = None;
                self.compile();
            }
        });
    }

    fn render_program_section(&mut self, ui: &mut egui::Ui, kind: ProgramKind, title: &str) {
        let entries: Vec<(String, String)> = self
            .catalog
            .get_programs_by_kind(kind)
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();

        if entries.is_empty() {
            return;
        }

        egui::CollapsingHeader::new(title)
            .default_open(true)
            .show(ui, |ui| {
                for (id, name) in &entries {
                    let is_rename_active = self.rename_id.as_deref() == Some(id.as_str());
                    if is_rename_active {
                        let response = ui.text_edit_singleline(&mut self.rename_buffer);
                        response.request_focus();
                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if response.lost_focus() || enter_pressed {
                            if let Some(program) = self.catalog.current_program_mut() {
                                if program.id == *id {
                                    program.name = self.rename_buffer.trim().to_string();
                                }
                            }
                            self.rename_id = None;
                            ui.ctx().request_repaint();
                        }
                    } else {
                        let selected = *id == self.catalog.selected_program_id;
                        let response = ui.selectable_label(selected, name);
                        if response.clicked() {
                            self.catalog.select_program(id);
                            self.compile();
                        }
                        if response.double_clicked() && kind == ProgramKind::Custom {
                            self.rename_buffer = name.clone();
                            self.rename_id = Some(id.clone());
                            ui.ctx().request_repaint();
                        }
                    }
                }
            });
    }

    fn save_state(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

// ------------------------------------------------------------
// eframe::App
// ------------------------------------------------------------

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
                // Full-screen debugger
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
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
    }
}

// ------------------------------------------------------------
// Top bars
// ------------------------------------------------------------

impl FullStackApp {
    fn ide_top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = ui_theme();
        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            // -- Left: Compile and Run actions --------------------------------
            if ui
                .add(egui::Button::new("Compile").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                self.compile();
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("Run in VM").strong())
                        .fill(Color32::from_rgb(30, 110, 60))
                        .min_size(egui::vec2(100.0, 35.0)),
                )
                .clicked()
            {
                if let Some(assembled) = &self.compilation_state.assembled {
                    let entry = self.compilation_state.entry_symbol.clone();
                    let base = self.compilation_state.load_base;
                    self.compilation_state.vm_result = Some(run_in_vm(assembled, &entry, base));
                }
            }

            // Target mode selector
            ui.separator();
            ui.label("Target:");
            let prev_mode = self.target_mode;
            egui::ComboBox::from_id_salt("target_mode_combo")
                .selected_text(self.target_mode.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.target_mode,
                        TargetMode::Hosted,
                        TargetMode::Hosted.label(),
                    );
                    ui.selectable_value(
                        &mut self.target_mode,
                        TargetMode::Freestanding,
                        TargetMode::Freestanding.label(),
                    );
                });
            if self.target_mode != prev_mode {
                let new_mode = self.target_mode;
                // Reset target_mode to prev so set_target_mode sees a real change.
                self.target_mode = prev_mode;
                self.set_target_mode(new_mode);
            }

            // Entry-point and load-base inputs (freestanding only)
            if self.target_mode == TargetMode::Freestanding {
                ui.label("Entry:");
                let ep_response = ui.add(
                    egui::TextEdit::singleline(&mut self.entry_point)
                        .desired_width(70.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("kmain"),
                );
                if ep_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let ep = self.entry_point.trim().to_owned();
                    let ep = if ep.is_empty() {
                        "kmain".to_owned()
                    } else {
                        ep
                    };
                    self.pipeline.entry_point = Some(ep);
                    self.compile();
                }

                ui.label("Base:");
                let lb_response = ui.add(
                    egui::TextEdit::singleline(&mut self.load_base_input)
                        .desired_width(90.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("0x80200000"),
                );
                if lb_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.compile();
                }
            }

            // Overflow actions + view picker in one menu
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("More", |ui| {
                    if ui.button("Reset File").clicked() {
                        if let Some(program) = self.catalog.current_program() {
                            if program.is_custom() {
                                self.catalog
                                    .set_selected_source(blank_custom_program_source());
                            } else {
                                self.catalog.ensure_consistency();
                            }
                        }
                        self.compile();
                        ui.close();
                    }
                    if ui.button("Reset UI Layout").clicked() {
                        self.reset_layout();
                        ui.close();
                    }
                    ui.separator();
                    ui.label(RichText::new("Add View").small().weak());
                    let view_entries: &[(&str, fn() -> Box<dyn CompilerView>)] = &[
                        ("Source", || Box::new(SourceView::default())),
                        ("Tokens", || Box::new(TokensView::default())),
                        ("AST", || Box::new(AstView::default())),
                        ("IR", || Box::new(IrView::default())),
                        ("Assembly", || Box::new(AssemblyView::default())),
                        ("CFG", || Box::new(CfgView::default())),
                        ("Stack", || Box::new(StackView::default())),
                        ("Memory Map", || Box::new(MemoryMapView::default())),
                        ("Execution (QEMU)", || Box::new(ExecutionView::default())),
                        ("VM Output", || Box::new(VmExecutionView::default())),
                    ];
                    for (label, make) in view_entries {
                        if ui.button(*label).clicked() {
                            self.pending_new_view =
                                Some(ViewWrapper::new(make(), &mut self.next_view_id));
                            ui.close();
                        }
                    }
                });
            });

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                // Primary action - To Debugger
                let can_debug = self.compilation_state.assembled.is_some();
                if ui
                    .add_enabled(
                        can_debug,
                        egui::Button::new(RichText::new("To Debugger").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(100.0, 35.0)),
                    )
                    .on_disabled_hover_text("Compile successfully first")
                    .clicked()
                {
                    self.enter_debug_mode();
                }

                ui.separator();

                if let Some(program) = self.catalog.current_program() {
                    let short_name: String = program.name.chars().take(24).collect();
                    ui.label(RichText::new(short_name).strong());
                    ui.label(
                        RichText::new(match program.kind {
                            ProgramKind::Example => "example",
                            ProgramKind::Custom => "custom",
                            ProgramKind::Stdlib => "stdlib",
                        })
                        .weak()
                        .small(),
                    );
                    ui.separator();
                }
                match &self.compilation_state.error_summary.clone() {
                    Some(summary) => {
                        let short: String = summary.chars().take(40).collect();
                        ui.colored_label(theme.error, format!("ERR: {short}"));
                    }
                    None => {
                        ui.colored_label(theme.success, "OK");
                    }
                }
            });
        });
    }

    fn debug_top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = ui_theme();
        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Reset").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.reset();
                }
            }

            ui.separator();

            ui.label("N:");
            ui.add(
                egui::TextEdit::singleline(&mut self.step_n_input)
                    .desired_width(36.0)
                    .font(egui::TextStyle::Monospace),
            );
            let n: u64 = self.step_n_input.trim().parse().unwrap_or(1).max(1);

            let session = self.compilation_state.debug_session.as_ref();
            let is_running = session
                .map(|s| s.status == SessionStatus::Running)
                .unwrap_or(false);

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Insn").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text(format!("Retire {n} instruction(s) through the pipeline"))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n_instructions(n);
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Cycle").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text(format!("Advance {n} pipeline cycle(s)"))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n(n);
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new("Step Fn").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text("Run until the PC enters a different function")
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_to_next_function();
                }
            }

            if ui
                .add_enabled(
                    is_running,
                    egui::Button::new(RichText::new("Run").strong())
                        .fill(Color32::from_rgb(30, 110, 60))
                        .min_size(egui::vec2(100.0, 35.0)),
                )
                .on_hover_text("Run until halt or error")
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n(u64::MAX);
                }
            }

            ui.separator();

            // Follow PC toggle, shared with the disassembly view.
            let follow = &mut self.compilation_state.disasm_follow_pc;
            let follow_color = if *follow {
                Color32::from_rgb(90, 200, 120)
            } else {
                Color32::from_rgb(80, 88, 108)
            };
            if ui
                .add(
                    egui::Button::new(RichText::new("Follow PC").color(follow_color))
                        .selected(*follow)
                        .min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text("Keep the disassembly view scrolled to the current PC")
                .clicked()
            {
                *follow = !*follow;
            }

            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("More", |ui| {
                    if ui.button("Reset layout").clicked() {
                        self.reset_debug_layout();
                        ui.close();
                    }
                    ui.separator();
                    ui.label(RichText::new("Add View").small().weak());
                    let entries: &[(&str, fn() -> Box<dyn CompilerView>)] = &[
                        ("CPU State", || Box::new(CpuStateView::default())),
                        ("Pipeline", || Box::new(PipelineView::default())),
                        ("Memory", || Box::new(MemoryView::default())),
                        ("IO", || Box::new(IoView::default())),
                        ("Cache", || Box::new(CacheView::default())),
                        ("Framebuffer", || Box::new(FramebufferView::default())),
                    ];
                    for (label, make) in entries {
                        if ui.button(*label).clicked() {
                            self.pending_new_view =
                                Some(ViewWrapper::new(make(), &mut self.next_view_id));
                            ui.close();
                        }
                    }
                });
            });

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                // To IDE button
                if ui
                    .add(
                        egui::Button::new(RichText::new("To IDE").strong())
                            .fill(theme.accent)
                            .min_size(egui::vec2(100.0, 35.0)),
                    )
                    .clicked()
                {
                    self.exit_debug_mode();
                    return;
                }

                ui.separator();

                let Some(session) = &self.compilation_state.debug_session else {
                    return;
                };

                let (status_text, status_color) = match &session.status {
                    SessionStatus::Running => ("Running", theme.success),
                    SessionStatus::Halted(0) => ("Halted OK", theme.success),
                    SessionStatus::Halted(_) => ("Halted (err)", theme.error),
                    SessionStatus::Error(_) => ("Error", theme.error),
                };
                ui.colored_label(status_color, status_text);
                ui.separator();
                ui.label(
                    RichText::new(format!("{} steps", session.step_count))
                        .monospace()
                        .weak(),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("PC {:#010x}", session.snapshot.cpu.pc))
                        .monospace()
                        .strong(),
                );
            });
        });
    }
}

// ------------------------------------------------------------
// DockTabViewer
// ------------------------------------------------------------
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

// ------------------------------------------------------------
// Internal VM runner
// ------------------------------------------------------------

/// Parse a number that may be a `0x…` hex literal or a plain decimal integer.
fn parse_hex_or_dec(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

fn run_in_vm(
    assembled: &crate::assembly_language::assembler::output::AssembledOutput,
    entry_symbol: &str,
    load_base: u64,
) -> VmExecutionResult {
    use crate::virtual_machine::cpu::StepOutcome;
    use crate::virtual_machine::virtual_machine::VirtualMachine;

    const MAX_STEPS: u64 = 5_000_000;
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
    let result = vm.run(MAX_STEPS);

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
