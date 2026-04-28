use crate::high_level_language::compilation_pipeline::CompilationPipeline;
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::token::Token;
use crate::view::{
    AssemblyView, AstView, CompilationState, CompilerView, IrView, ProgramCatalog, ProgramKind,
    SourceView, TokensView, blank_custom_program_source, StackView,
};
use egui_dock::{DockState, NodeIndex};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateApp {
    catalog: ProgramCatalog,
    #[serde(skip)]
    dock: DockState<Box<dyn CompilerView>>,
    #[serde(skip)]
    compilation_state: CompilationState,
    #[serde(skip)]
    pipeline: CompilationPipeline,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let catalog = ProgramCatalog::default();
        let mut app = Self {
            catalog,
            dock: DockState::new(vec![]),
            compilation_state: CompilationState::default(),
            pipeline: CompilationPipeline::new(),
        };
        app.reset_layout();
        app
    }
}

impl TemplateApp {
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
        let source: Box<dyn CompilerView> = Box::new(SourceView);
        let tokens: Box<dyn CompilerView> = Box::new(TokensView);
        let ast: Box<dyn CompilerView> = Box::new(AstView);
        let ir: Box<dyn CompilerView> = Box::new(IrView);
        let asm: Box<dyn CompilerView> = Box::new(AssemblyView);
        let stack: Box<dyn CompilerView> = Box::new(StackView::default());

        let mut dock = DockState::new(vec![source]);
        let surface = dock.main_surface_mut();

        let [_left_node, right_node] =
            surface.split_right(NodeIndex::root(), 0.5, vec![ir, tokens, ast]);

        surface.split_below(right_node, 0.5, vec![asm, stack]);

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
                self.compilation_state.error = Some(format!("Lexer error: {message}"));
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
                self.compilation_state.asm =
                    self.pipeline.compile_ir_to_assembly(&result.ir_program);
                self.compilation_state.error = None;
                self.compilation_state.just_compiled = true;
            }
            Err(error) => {
                self.compilation_state.error = Some(error.to_string());
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
                    self.compile();
                }

                if ui.button("Duplicate").clicked() {
                    self.catalog.duplicate_current_program();
                    self.compile();
                }
            });

            ui.add_space(8.0);
            self.render_program_section(ui, ProgramKind::Example, "Examples");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Custom, "Your programs");
            ui.separator();

            if let Some(program) = self.catalog.current_program() {
                ui.label(egui::RichText::new(format!("Current: {}", program.name)).strong());
                ui.small(match program.kind {
                    ProgramKind::Example => "Example program",
                    ProgramKind::Custom => "Your program",
                });
            }

            let is_custom = self
                .catalog
                .current_program()
                .map(|program| program.is_custom())
                .unwrap_or(false);

            if is_custom {
                ui.add_space(8.0);
                ui.label("Rename:");

                if let Some(program) = self.catalog.current_program_mut() {
                    ui.text_edit_singleline(&mut program.name);
                }

                if ui.button("Delete").clicked() {
                    self.catalog.delete_current_custom_program();
                    self.compile();
                }
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
                    let selected = *id == self.catalog.selected_program_id;
                    let response = ui.selectable_label(selected, name);

                    if response.clicked() {
                        self.catalog.select_program(id);
                        self.compile();
                    }
                }
            });
    }

    fn save_state(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl eframe::App for TemplateApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

                    ui.separator();

                    if let Some(program) = self.catalog.current_program() {
                        ui.label(egui::RichText::new(&program.name).strong());
                        ui.label(match program.kind {
                            ProgramKind::Example => "Example",
                            ProgramKind::Custom => "Custom",
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match &self.compilation_state.error {
                            Some(error) => {
                                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                            }
                            None => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(100, 200, 120),
                                    "Compilation successful",
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
                .show_close_buttons(false)
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

struct DockTabViewer<'a> {
    state: &'a mut CompilationState,
    catalog: &'a mut ProgramCatalog,
}

impl egui_dock::TabViewer for DockTabViewer<'_> {
    type Tab = Box<dyn CompilerView>;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let ctx = ui.ctx().clone();
        tab.ui(ui, &ctx, self.state, self.catalog);
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }
}
