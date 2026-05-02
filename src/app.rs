use crate::high_level_language::compilation_pipeline::CompilationPipeline;
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::token::Token;
use crate::view::debug::{DebugSession, SessionStatus};
use crate::view::views::vm_execution_view::VmExecutionResult;
use crate::view::{
    AssemblyView, AstView, CacheView, CfgView, CompilationState, CompilerView, CpuStateView,
    ExecutionView, FramebufferView, IoView, IrView, MemoryMapView, MemoryView, PipelineView,
    ProgramCatalog, ProgramKind, SourceView, StackView, TokensView, VmExecutionView,
    blank_custom_program_source,
};
use egui::{Color32, Layout, RichText};
use egui_dock::{DockState, NodeIndex};
use std::fmt;

#[derive(Default, Clone, PartialEq, Eq)]
enum AppMode {
    #[default]
    Ide,
    Debug,
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
    pending_new_view: Option<ViewWrapper>,
    #[serde(skip)]
    next_view_id: u64,
    #[serde(skip)]
    mode: AppMode,
    #[serde(skip)]
    step_n_input: String,
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
            pending_new_view: None,
            next_view_id: 0,
            mode: AppMode::Ide,
            step_n_input: "1".to_owned(),
        };
        app.reset_layout();
        app.reset_debug_layout();
        app
    }
}

impl FullStackApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: Self = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        app.catalog.ensure_consistency();
        app.reset_layout();
        app.compile();
        app
    }

    fn reset_layout(&mut self) {
        let views = vec![
            ViewWrapper::new(Box::new(SourceView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(TokensView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(AstView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(IrView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(AssemblyView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(CfgView), &mut self.next_view_id),
            ViewWrapper::new(Box::new(StackView::default()), &mut self.next_view_id),
            ViewWrapper::new(Box::new(MemoryMapView::default()), &mut self.next_view_id),
            ViewWrapper::new(Box::new(ExecutionView::default()), &mut self.next_view_id),
            ViewWrapper::new(Box::new(VmExecutionView), &mut self.next_view_id),
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
        let cpu = ViewWrapper::new(Box::new(CpuStateView::default()), &mut self.next_view_id);
        let pipeline = ViewWrapper::new(Box::new(PipelineView::default()), &mut self.next_view_id);
        let cache = ViewWrapper::new(Box::new(CacheView::default()), &mut self.next_view_id);
        let fb = ViewWrapper::new(Box::new(FramebufferView::default()), &mut self.next_view_id);
        let mem = ViewWrapper::new(Box::new(MemoryView::default()), &mut self.next_view_id);
        let io = ViewWrapper::new(Box::new(IoView::default()), &mut self.next_view_id);

        // Two equal columns (50% each)
        let mut dock = DockState::new(vec![cpu, pipeline, cache]);
        let surface = dock.main_surface_mut();

        // Split root into left (50%) and right (50%)
        let [left, _right] = surface.split_right(NodeIndex::root(), 0.5, vec![mem, io]);

        // Split left column vertically: top 50% for CPU views, bottom 50% for Framebuffer
        surface.split_below(left, 0.5, vec![fb]);

        self.debug_dock = dock;
    }

    fn enter_debug_mode(&mut self) {
        if let Some(assembled) = &self.compilation_state.assembled {
            self.compilation_state.debug_session = Some(DebugSession::new(assembled));
            self.reset_debug_layout();
            self.mode = AppMode::Debug;
        }
    }

    fn exit_debug_mode(&mut self) {
        self.compilation_state.debug_session = None;
        self.mode = AppMode::Ide;
    }

    fn compile(&mut self) {
        let source = self.catalog.get_selected_source();
        let mut lexer = Lexer::new(&source);
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

        match self.pipeline.compile(&source) {
            Ok(result) => {
                self.compilation_state.ast = format!("{:#?}", result.ast);
                self.compilation_state.ir = result.ir_program.to_string();
                let (asm_text, asm_tokens) = self
                    .pipeline
                    .compile_ir_to_assembly_with_tokens(&result.ir_program);
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

            ui.add_space(8.0);
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
        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            // ── Left: Compile and Run actions ────────────────────────────────
            if ui
                .add(egui::Button::new("Compile").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                self.compile();
            }
            if ui
                .add(egui::Button::new("Run in VM").min_size(egui::vec2(100.0, 35.0)))
                .clicked()
            {
                if let Some(assembled) = &self.compilation_state.assembled {
                    self.compilation_state.vm_result = Some(run_in_vm(assembled));
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

            // ── Right: status + program name + To Debugger button ────────────
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                // Primary action - To Debugger
                let can_debug = self.compilation_state.assembled.is_some();
                if ui
                    .add_enabled(
                        can_debug,
                        egui::Button::new(RichText::new("To Debugger").strong())
                            .fill(Color32::from_rgb(45, 85, 175))
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
                        })
                        .weak()
                        .small(),
                    );
                    ui.separator();
                }
                match &self.compilation_state.error_summary.clone() {
                    Some(summary) => {
                        let short: String = summary.chars().take(40).collect();
                        ui.colored_label(Color32::from_rgb(220, 80, 80), format!("ERR: {short}"));
                    }
                    None => {
                        ui.colored_label(Color32::from_rgb(90, 200, 100), "OK");
                    }
                }
            });
        });
    }

    fn debug_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(egui::vec2(ui.available_width(), ui.available_height()));
        ui.horizontal(|ui| {
            // ── Left: Debug controls ─────────────────────────────────────────
            if ui
                .add(egui::Button::new("Reset").min_size(egui::vec2(80.0, 35.0)))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.reset();
                }
            }

            ui.separator();

            ui.label("Step");
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
                    egui::Button::new("Step").min_size(egui::vec2(80.0, 35.0)),
                )
                .on_hover_text(format!("Step {n} instruction(s)"))
                .clicked()
            {
                if let Some(s) = self.compilation_state.debug_session.as_mut() {
                    s.step_n(n);
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

            // ── Right: PC / steps / status + To IDE button ───────────────────
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);

                // To IDE button
                if ui
                    .add(
                        egui::Button::new(RichText::new("To IDE").strong())
                            .fill(Color32::from_rgb(45, 85, 175))
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
                    SessionStatus::Running => ("Running", Color32::from_rgb(80, 200, 80)),
                    SessionStatus::Halted(0) => ("Halted OK", Color32::from_rgb(80, 200, 80)),
                    SessionStatus::Halted(_) => ("Halted (err)", Color32::from_rgb(220, 80, 80)),
                    SessionStatus::Error(_) => ("Error", Color32::from_rgb(220, 80, 80)),
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

fn run_in_vm(
    assembled: &crate::assembly_language::assembler::output::AssembledOutput,
) -> VmExecutionResult {
    use crate::virtual_machine::cpu::StepOutcome;
    use crate::virtual_machine::virtual_machine::VirtualMachine;

    const MAX_STEPS: u64 = 5_000_000;
    let mut vm = VirtualMachine::new(assembled);
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
