// file: src/app.rs
use crate::high_level_language::compilation_pipeline::CompilationPipeline;
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::token::Token;
use crate::view::{
    AssemblyView, AstView, CfgView, CompilationState, CompilerView, ExecutionView, IrView,
    MemoryMapView, ProgramCatalog, ProgramKind, SourceView, StackView, TokensView, VmExecutionView,
    blank_custom_program_source,
};
use egui_dock::{DockState, NodeIndex};
use std::fmt;

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

// egui_dock requires Display for the tab title
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
}

impl Default for FullStackApp {
    fn default() -> Self {
        let catalog = ProgramCatalog::default();
        let mut app = Self {
            catalog,
            dock: DockState::new(vec![]),
            compilation_state: CompilationState::default(),
            pipeline: CompilationPipeline::new(),
            rename_id: None,
            rename_buffer: String::new(),
            pending_new_view: None,
            next_view_id: 0,
        };
        app.reset_layout();
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
            ViewWrapper::new(Box::new(SourceView), &mut self.next_view_id), // 0
            ViewWrapper::new(Box::new(TokensView), &mut self.next_view_id), // 1
            ViewWrapper::new(Box::new(AstView), &mut self.next_view_id),    // 2
            ViewWrapper::new(Box::new(IrView), &mut self.next_view_id),     // 3
            ViewWrapper::new(Box::new(AssemblyView), &mut self.next_view_id), // 4
            ViewWrapper::new(Box::new(CfgView), &mut self.next_view_id),    // 5
            ViewWrapper::new(Box::new(StackView::default()), &mut self.next_view_id), // 6
            ViewWrapper::new(Box::new(MemoryMapView::default()), &mut self.next_view_id), // 7
            ViewWrapper::new(Box::new(ExecutionView::default()), &mut self.next_view_id),             // 8
            ViewWrapper::new(Box::new(VmExecutionView), &mut self.next_view_id),           // 9
        ];

        // Source is the first view → root
        let mut dock = DockState::new(vec![views[0].clone()]);
        let surface = dock.main_surface_mut();

        // Right side: IR (index 3), Tokens (1), AST (2)
        let [_left, right] = surface.split_right(
            NodeIndex::root(),
            0.5,
            vec![views[3].clone(), views[1].clone(), views[2].clone()],
        );

        // Bottom side: Assembly (4), CFG (5), Stack (6), Execution/QEMU (8), VM Output (9)
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

                // Don't auto-run VM - user must click "Run in VM" button
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
            .map(|program| (program.id.clone(), program.name.clone()))
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

impl eframe::App for FullStackApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // --- Process a pending "Add View" request (deferred from menu) ---
        if let Some(view) = self.pending_new_view.take() {
            self.dock.main_surface_mut().push_to_focused_leaf(view);
        }

        egui::Panel::left("left_panel")
            .resizable(true)
            .default_size(250.0)
            .show_inside(ui, |ui| self.catalog_ui(ui));

        egui::Panel::top("top_panel")
            .resizable(false)
            .exact_size(44.0)
            .show_inside(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([110.0, 30.0], egui::Button::new("Compile"))
                        .clicked()
                    {
                        self.compile();
                    }

                    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
                    if ui
                        .add_sized([110.0, 30.0], egui::Button::new("Run in VM"))
                        .clicked()
                    {
                        // Re-run the VM to refresh output
                        if let Some(assembled) = &self.compilation_state.assembled {
                            self.compilation_state.vm_output = run_in_vm(assembled);
                        }
                    }

                    if ui
                        .add_sized([110.0, 30.0], egui::Button::new("Reset File"))
                        .clicked()
                    {
                        if let Some(program) = self.catalog.current_program() {
                            if program.is_custom() {
                                self.catalog
                                    .set_selected_source(blank_custom_program_source());
                            } else {
                                self.catalog.ensure_consistency();
                            }
                        }
                        self.compile();
                    }

                    if ui
                        .add_sized([110.0, 30.0], egui::Button::new("Reset UI"))
                        .clicked()
                    {
                        self.reset_layout();
                    }

                    // "Add View" dropdown
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button("Add View", |ui| {
                            let entries: Vec<(&str, Box<dyn CompilerView>)> = vec![
                                ("Source",          Box::new(SourceView::default())),
                                ("Tokens",          Box::new(TokensView::default())),
                                ("AST",             Box::new(AstView::default())),
                                ("IR",              Box::new(IrView::default())),
                                ("Assembly",        Box::new(AssemblyView::default())),
                                ("CFG",             Box::new(CfgView::default())),
                                ("Stack",           Box::new(StackView::default())),
                                ("Memory Map",      Box::new(MemoryMapView::default())),
                                ("Execution (QEMU)", Box::new(ExecutionView::default())),
                                ("VM Output",       Box::new(VmExecutionView::default())),
                            ];
                            for (label, proto) in &entries {
                                if ui.button(*label).clicked() {
                                    self.pending_new_view = Some(ViewWrapper::new(
                                        proto.clone_box(),
                                        &mut self.next_view_id,
                                    ));
                                    ui.close();
                                }
                            }
                        });
                    });

                    ui.separator();

                    if let Some(program) = self.catalog.current_program() {
                        ui.label(egui::RichText::new(&program.name).strong());
                        ui.label(match program.kind {
                            ProgramKind::Example => "Example",
                            ProgramKind::Custom => "Custom",
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match &self.compilation_state.error_summary {
                            Some(summary) => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(220, 80, 80),
                                    format!("✖ {summary}"),
                                );
                            }
                            None => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(100, 200, 120),
                                    "✔ Compilation successful",
                                );
                            }
                        }
                    });
                });
                ui.add_space(4.0);
            });

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

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
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

// ---------------------------------------------------------------------------
// Internal VM runner
// ---------------------------------------------------------------------------

fn run_in_vm(assembled: &crate::assembly_language::assembler::output::AssembledOutput) -> String {
    use crate::virtual_machine::cpu::StepOutcome;
    use crate::virtual_machine::virtual_machine::VirtualMachine;

    const MAX_STEPS: u64 = 5_000_000;

    let mut vm = VirtualMachine::new(assembled);
    let result = vm.run(MAX_STEPS);

    let mut out = String::new();

    out.push_str("╔══════════════════════════════════════╗\n");
    out.push_str("║       VM Execution Output            ║\n");
    out.push_str("╚══════════════════════════════════════╝\n\n");

    if !result.uart_output.is_empty() {
        out.push_str(&result.uart_output);
        if !result.uart_output.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push_str("(No output)\n\n");
    }

    // Add execution summary with status indicator
    out.push_str("\n");
    out.push_str("┌─────────────────────────────────────┐\n");
    out.push_str("│ Execution Summary                   │\n");
    out.push_str("├─────────────────────────────────────┤\n");
    
    match result.outcome {
        StepOutcome::Halted(0) => {
            out.push_str("│ +  Ran Successfully                 │\n");
        }
        StepOutcome::Halted(code) => {
            let text = format!("Exited with code {}", code);
            out.push_str(&format!("│ -  {:<33}│\n", text));
        }
        StepOutcome::Continue => {
            let text = format!("Reached step limit ({})", MAX_STEPS);
            out.push_str(&format!("│ !  {:<33}│\n", text));
        }
    }
    
    out.push_str(&format!("│ Steps: {:<29}│\n", result.steps.to_string()));
    out.push_str("└─────────────────────────────────────┘\n");
    
    out
}
