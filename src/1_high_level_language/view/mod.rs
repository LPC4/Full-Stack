use crate::high_level_language::compilation_pipeline::{CompilationError, CompilationPipeline};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::token::Token;
use egui::Color32;
use egui::RichText;
use egui::text::LayoutJob;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ProgramKind {
    Example,
    Custom,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct ProgramFile {
    id: String,
    name: String,
    kind: ProgramKind,
    source: String,
    #[serde(skip)]
    description: String,
}

impl ProgramFile {
    fn example(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Example,
            source: source.to_owned(),
            description: description.to_owned(),
        }
    }

    fn custom(id: String, name: String, source: String) -> Self {
        Self {
            id,
            name,
            kind: ProgramKind::Custom,
            source,
            description: String::from("Your personal in-memory program."),
        }
    }

    fn is_custom(&self) -> bool {
        matches!(self.kind, ProgramKind::Custom)
    }
}

fn built_in_programs() -> Vec<ProgramFile> {
    vec![
        ProgramFile::example(
            "example-showcase",
            "Showcase",
            "A full tour of structs, arrays, pointers, loops, and defer.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/example_program.hll"
            )),
        ),
        ProgramFile::example(
            "example-debug-pointers",
            "Debug Pointers",
            "A compact pointer and cleanup demo.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/debug/debug.hll"
            )),
        ),
        ProgramFile::example(
            "example-struct-destructuring",
            "Struct Destructuring",
            "Nested records and destructuring assignments.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/test/integration/struct_destructuring_test.hll"
            )),
        ),
        ProgramFile::example(
            "example-generic-types",
            "Generic Types",
            "A generic record specialized with multiple concrete types.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/test/integration/generic_types_test.hll"
            )),
        ),
        ProgramFile::example(
            "example-pointer-flow",
            "Pointer Flow",
            "Chained pointer and array writes in a small program.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/test/integration/pointer_heavy_flow_test.hll"
            )),
        ),
        ProgramFile::example(
            "example-function-syntax",
            "Function Syntax",
            "A minimal function example showing the current declaration style.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/test/integration/new_function_syntax_test.hll"
            )),
        ),
    ]
}

fn blank_custom_program_source() -> String {
    ["main: () -> i32 {", "    return 0", "}", ""].join("\n")
}

#[derive(Clone, Copy)]
enum ViewType {
    Source,
    Tokens,
    AST,
    IR,
}

/// High-level language visualization state and UI.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct HighLevelLanguageView {
    programs: Vec<ProgramFile>,
    selected_program_id: String,
    next_custom_program_id: u32,

    #[serde(skip)]
    source_code: String,

    #[serde(skip)]
    show_source: bool,
    #[serde(skip)]
    show_tokens: bool,
    #[serde(skip)]
    show_ast: bool,
    #[serde(skip)]
    show_ir: bool,

    #[serde(skip)]
    tokens_output: String,
    #[serde(skip)]
    ast_output: String,
    #[serde(skip)]
    ir_output: String,
    #[serde(skip)]
    compile_error: Option<String>,
    #[serde(skip)]
    just_compiled_successfully: bool,
    #[serde(skip)]
    compile_success_until: Option<f64>,
}

impl Default for HighLevelLanguageView {
    fn default() -> Self {
        let programs = built_in_programs();
        let selected_program_id = programs
            .first()
            .map(|program| program.id.clone())
            .unwrap_or_default();
        let source_code = programs
            .first()
            .map(|program| program.source.clone())
            .unwrap_or_default();

        Self {
            programs,
            selected_program_id,
            next_custom_program_id: 1,
            source_code,
            show_source: true,
            show_tokens: false,
            show_ast: false,
            show_ir: true,
            tokens_output: String::new(),
            ast_output: String::new(),
            ir_output: String::new(),
            compile_error: None,
            just_compiled_successfully: false,
            compile_success_until: None,
        }
    }
}

// Helper to add simple syntax highlighting
fn highlight_code(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let keywords = [
        "type", "const", "if", "else", "while", "return", "defer", "new", "free", "and", "or",
        "true", "false", "null", "main", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
        "f32", "f64", "bool",
    ];

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 100),
                    ..Default::default()
                },
            );
            if has_newline {
                job.append(
                    "\n",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
            }
            continue;
        }

        let mut start = 0;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while start < len {
            let mut end = start;
            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
                while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                let word = &line[start..end];
                let color = if keywords.contains(&word) {
                    Color32::from_rgb(200, 100, 200) // matched keywords (purple)
                } else {
                    theme.visuals.text_color() // generic
                };

                job.append(
                    word,
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color,
                        ..Default::default()
                    },
                );
            } else if bytes[start].is_ascii_digit() {
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(100, 150, 200), // numbers (blue)
                        ..Default::default()
                    },
                );
            } else {
                while end < len && !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_' {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: theme.visuals.text_color(),
                        ..Default::default()
                    },
                );
            }
            start = end;
        }
        if has_newline {
            job.append(
                "\n",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
        }
    }
    job
}

// Helper for AST and token highlighting
fn highlight_ast(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let mut start = 0;
    let bytes = code.as_bytes();
    let len = bytes.len();

    while start < len {
        let mut end = start;
        let c = bytes[start];

        if c.is_ascii_alphabetic() || c == b'_' {
            // Identifiers/Types/Enums
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(180, 220, 180), // pale green
                    ..Default::default()
                },
            );
        } else if c.is_ascii_digit() {
            // Numbers
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 200), // blue
                    ..Default::default()
                },
            );
        } else if c == b'{' || c == b'}' || c == b'[' || c == b']' || c == b'(' || c == b')' {
            // Brackets
            end += 1;
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(220, 200, 100), // yellow
                    ..Default::default()
                },
            );
        } else if c == b'"' || c == b'\'' {
            // Strings
            end += 1;
            while end < len && bytes[end] != c {
                if bytes[end] == b'\\' && end + 1 < len {
                    end += 2;
                } else {
                    end += 1;
                }
            }
            if end < len {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(200, 150, 100), // orange
                    ..Default::default()
                },
            );
        } else {
            // Symbols/Spaces
            while end < len
                && !bytes[end].is_ascii_alphanumeric()
                && bytes[end] != b'_'
                && bytes[end] != b'{'
                && bytes[end] != b'}'
                && bytes[end] != b'['
                && bytes[end] != b']'
                && bytes[end] != b'('
                && bytes[end] != b')'
                && bytes[end] != b'"'
                && bytes[end] != b'\''
            {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: theme.visuals.text_color(),
                    ..Default::default()
                },
            );
        }
        start = end;
    }
    job
}

// Helper for IR highlighting
fn highlight_ir(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let ir_keywords = [
        "type",
        "define",
        "entry",
        "branch",
        "jump",
        "ret",
        "call",
        "read",
        "write",
        "index",
        "stack_alloc",
        "heap_alloc",
        "heap_free",
        "offset",
        "math",
        "cmp",
        "cast",
        "unary",
    ];

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        // Comments
        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 100),
                    ..Default::default()
                },
            );
            if has_newline {
                job.append(
                    "\n",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
            }
            continue;
        }

        let mut start = 0;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while start < len {
            let mut end = start;
            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' || bytes[start] == b'@' {
                if bytes[start] == b'@' {
                    end += 1;
                    while end < len
                        && (bytes[end].is_ascii_alphanumeric()
                            || bytes[end] == b'_'
                            || bytes[end] == b'.'
                            || bytes[end] == b'<'
                            || bytes[end] == b'>')
                    {
                        end += 1;
                    }
                    // Labels/functions
                    job.append(
                        &line[start..end],
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(150, 200, 255), // light blue
                            ..Default::default()
                        },
                    );
                } else {
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    let word = &line[start..end];
                    let color = if ir_keywords.contains(&word) {
                        Color32::from_rgb(200, 100, 200) // keywords (purple)
                    } else {
                        theme.visuals.text_color()
                    };

                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color,
                            ..Default::default()
                        },
                    );
                }
            } else if bytes[start].is_ascii_digit() {
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(100, 200, 100), // green
                        ..Default::default()
                    },
                );
            } else if bytes[start] == b'$' {
                // Registers
                end += 1;
                while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(255, 200, 100), // orange
                        ..Default::default()
                    },
                );
            } else {
                while end < len
                    && !bytes[end].is_ascii_alphanumeric()
                    && bytes[end] != b'_'
                    && bytes[end] != b'$'
                    && bytes[end] != b'@'
                {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: theme.visuals.text_color(),
                        ..Default::default()
                    },
                );
            }
            start = end;
        }
        if has_newline {
            job.append(
                "\n",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
        }
    }
    job
}

impl HighLevelLanguageView {
    fn ensure_program_catalog(&mut self) {
        let mut merged_programs = Vec::with_capacity(self.programs.len().max(1) + 8);

        for built_in in built_in_programs() {
            if let Some(existing) = self
                .programs
                .iter()
                .find(|program| program.id == built_in.id)
            {
                let mut updated = existing.clone();
                updated.name = built_in.name;
                updated.kind = ProgramKind::Example;
                updated.description = built_in.description;
                merged_programs.push(updated);
            } else {
                merged_programs.push(built_in);
            }
        }

        merged_programs.extend(
            self.programs
                .iter()
                .filter(|program| program.is_custom())
                .cloned(),
        );

        self.programs = merged_programs;

        if self.next_custom_program_id == 0 {
            self.next_custom_program_id = 1;
        }

        if self.selected_program_id.is_empty()
            || !self
                .programs
                .iter()
                .any(|program| program.id == self.selected_program_id)
        {
            self.selected_program_id = self
                .programs
                .first()
                .map(|program| program.id.clone())
                .unwrap_or_default();
        }
    }

    fn current_program_index(&self) -> Option<usize> {
        self.programs
            .iter()
            .position(|program| program.id == self.selected_program_id)
    }

    fn current_program(&self) -> Option<&ProgramFile> {
        self.current_program_index()
            .map(|index| &self.programs[index])
    }

    fn current_program_mut(&mut self) -> Option<&mut ProgramFile> {
        let index = self.current_program_index()?;
        self.programs.get_mut(index)
    }

    fn sync_current_program_source(&mut self) {
        let source = self.source_code.clone();

        if let Some(program) = self.current_program_mut() {
            program.source = source;
        }
    }

    fn load_selected_program_source(&mut self) {
        if let Some(source) = self.current_program().map(|program| program.source.clone()) {
            self.source_code = source;
        }
    }

    pub fn post_load(&mut self) {
        self.ensure_program_catalog();
        self.load_selected_program_source();
    }

    pub fn prepare_for_save(&mut self) {
        self.sync_current_program_source();
    }

    fn select_program(&mut self, program_id: &str) {
        if self.selected_program_id == program_id {
            return;
        }

        self.sync_current_program_source();
        self.selected_program_id = program_id.to_owned();
        self.load_selected_program_source();
        self.compile();
    }

    fn create_custom_program(&mut self, source: String, name: String) {
        self.sync_current_program_source();

        let program_id = format!("custom-{}", self.next_custom_program_id);
        self.next_custom_program_id = self
            .next_custom_program_id
            .checked_add(1)
            .unwrap_or(self.next_custom_program_id);

        self.programs.push(ProgramFile::custom(
            program_id.clone(),
            name,
            source.clone(),
        ));
        self.selected_program_id = program_id;
        self.source_code = source;
        self.compile();
    }

    fn create_blank_program(&mut self) {
        let name = format!("Untitled {}", self.next_custom_program_id);
        self.create_custom_program(blank_custom_program_source(), name);
    }

    fn duplicate_current_program(&mut self) {
        let duplicate_name = self
            .current_program()
            .map(|program| format!("Copy of {}", program.name))
            .unwrap_or_else(|| String::from("Copy of current file"));

        self.create_custom_program(self.source_code.clone(), duplicate_name);
    }

    fn delete_current_custom_program(&mut self) {
        let Some(current) = self.current_program().cloned() else {
            return;
        };

        if !current.is_custom() {
            return;
        }

        self.programs.retain(|program| program.id != current.id);

        if let Some(next_program) = self.programs.first() {
            self.selected_program_id = next_program.id.clone();
            self.load_selected_program_source();
            self.compile();
        } else {
            self.selected_program_id.clear();
            self.source_code = blank_custom_program_source();
        }
    }

    fn render_program_section(&mut self, ui: &mut egui::Ui, kind: ProgramKind, title: &str) {
        let entries: Vec<_> = self
            .programs
            .iter()
            .filter(|program| program.kind == kind)
            .map(|program| {
                (
                    program.id.clone(),
                    program.name.clone(),
                    program.description.clone(),
                    program.id == self.selected_program_id,
                )
            })
            .collect();

        egui::CollapsingHeader::new(title)
            .default_open(true)
            .show(ui, |ui| {
                if entries.is_empty() {
                    ui.weak("No files yet.");
                    return;
                }

                for (id, name, description, selected) in entries {
                    ui.horizontal(|ui| {
                        ui.label("O"); // placeholder for potential icon

                        let response = ui.selectable_label(selected, name);
                        let response = if description.is_empty() {
                            response
                        } else {
                            response.on_hover_text(description)
                        };

                        if response.clicked() {
                            self.select_program(&id);
                        }
                    });
                }
            });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Files");
            });

            ui.add_space(6.0);
            ui.small(
                "Examples are embedded in the app; your own files stay in memory and app storage.",
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("New File").clicked() {
                    self.create_blank_program();
                }

                if ui.button("Duplicate").clicked() {
                    self.duplicate_current_program();
                }
            });

            ui.add_space(8.0);
            self.render_program_section(ui, ProgramKind::Example, "Examples");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Custom, "Your programs");

            ui.separator();
            if let Some(program) = self.current_program() {
                ui.label(RichText::new(format!("Current file: {}", program.name)).strong());
                ui.small(match program.kind {
                    ProgramKind::Example => "Embedded example program.",
                    ProgramKind::Custom => "Your personal in-memory program.",
                });

                if program.kind == ProgramKind::Example {
                    ui.small("Duplicate it if you want to keep a personal copy.");
                }
            }

            if let Some(program) = self.current_program_mut() {
                if program.is_custom() {
                    ui.add_space(8.0);
                    ui.label("Rename file:");
                    ui.text_edit_singleline(&mut program.name);

                    if ui.button("Delete file").clicked() {
                        self.delete_current_custom_program();
                    }
                }
            }
        });
    }
}

impl HighLevelLanguageView {
    pub fn compile(&mut self) {
        self.ensure_program_catalog();
        self.sync_current_program_source();

        let pipeline = CompilationPipeline::new();

        // First, tokenize for display
        let mut lexer = Lexer::new(&self.source_code);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            if let Token::Error(ref msg) = token {
                self.tokens_output = format!("LEXER ERROR: {msg}");
                self.ast_output = String::from("Did not parse due to lexer error.");
                self.ir_output = String::from("Did not compile due to lexer error.");
                self.compile_error = Some(format!("Lexer error: {msg}"));
                self.compile_success_until = None;
                self.just_compiled_successfully = false;
                return;
            }
            let is_eof = matches!(token, Token::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        self.tokens_output = format!("{tokens:#?}");

        // Then compile full pipeline
        match pipeline.compile(&self.source_code) {
            Ok(result) => {
                // Update outputs
                self.ast_output = format!("{:#?}", result.ast);
                self.ir_output = format!("{}", result.ir_program);

                // Check for diagnostics
                let errors: Vec<_> = result
                    .diagnostics
                    .iter()
                    .filter(|d| {
                        matches!(
                            d.level,
                            crate::high_level_language::compiler::DiagnosticLevel::Error
                        )
                    })
                    .map(|d| d.message.clone())
                    .collect();

                if errors.is_empty() {
                    self.compile_error = None;
                    self.just_compiled_successfully = true;
                } else {
                    self.compile_error =
                        Some(format!("Semantic errors:\n- {}", errors.join("\n- ")));
                    self.compile_success_until = None;
                    self.just_compiled_successfully = false;
                }
            }
            Err(err) => {
                // Handle compilation errors (excluding lexer which we handled above)
                match &err {
                    CompilationError::ParseError(parse_err) => {
                        self.ast_output = format!(
                            "Parser Error at pos {}: {}",
                            parse_err.pos, parse_err.message
                        );
                        self.ir_output = String::from("Did not compile due to parser error.");
                    }
                    CompilationError::CompilerError(compiler_err) => {
                        self.ir_output = format!("Compiler internal error: {compiler_err:?}");
                    }
                    CompilationError::SemanticErrors(errors) => {
                        self.ir_output = format!(
                            "Did not compile due to semantic errors.\n\nSEMANTIC ERRORS:\n- {}",
                            errors.join("\n- ")
                        );
                    }
                    _ => unreachable!("Lexer errors already handled"),
                }
                self.compile_error = Some(format!("{err}"));
                self.compile_success_until = None;
                self.just_compiled_successfully = false;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ensure_program_catalog();

        // Trigger compile on Ctrl+S
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.compile();
        }

        if self.just_compiled_successfully {
            self.just_compiled_successfully = false;
            self.compile_success_until = Some(ui.input(|i| i.time) + 2.0);
        }

        egui::Panel::left("high_level_language_files_panel")
            .resizable(true)
            .size_range(220.0..=320.0)
            .show_inside(ui, |ui| {
                self.render_sidebar(ui);
            });

        egui::Panel::top("high_level_language_top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Compiler:");
                ui.separator();
                ui.toggle_value(&mut self.show_source, "Source Code");
                ui.toggle_value(&mut self.show_tokens, "Lexer Tokens");
                ui.toggle_value(&mut self.show_ast, "Parser AST");
                ui.toggle_value(&mut self.show_ir, "Intermediate Repr.");
                ui.separator();

                if ui.button("Compile").clicked() {
                    self.compile();
                }

                if let Some(until) = self.compile_success_until {
                    let now = ui.input(|i| i.time);
                    if now < until {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            "Compiled successfully",
                        );
                        let duration = std::time::Duration::from_secs_f64(until - now);
                        ui.ctx().request_repaint_after(duration);
                    } else {
                        self.compile_success_until = None;
                    }
                }
            });
        });

        if let Some(err) = &self.compile_error {
            egui::Panel::bottom("compile_error_panel").show_inside(ui, |ui| {
                ui.colored_label(egui::Color32::RED, err);
            });
        }

        let ctx = ui.ctx().clone();

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut active_views = Vec::new();
            if self.show_source {
                active_views.push(ViewType::Source);
            }
            if self.show_tokens {
                active_views.push(ViewType::Tokens);
            }
            if self.show_ast {
                active_views.push(ViewType::AST);
            }
            if self.show_ir {
                active_views.push(ViewType::IR);
            }

            if active_views.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No views selected.");
                });
                return;
            }

            let rect = ui.available_rect_before_wrap();
            let rects = match active_views.len() {
                1 => vec![rect],
                2 => vec![
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y)),
                    egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max),
                ],
                3 => vec![
                    egui::Rect::from_min_max(rect.min, rect.center()),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.center().x, rect.min.y),
                        egui::pos2(rect.max.x, rect.center().y),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.center().y),
                        egui::pos2(rect.center().x, rect.max.y),
                    ),
                ],
                _ => vec![
                    egui::Rect::from_min_max(rect.min, rect.center()),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.center().x, rect.min.y),
                        egui::pos2(rect.max.x, rect.center().y),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.center().y),
                        egui::pos2(rect.center().x, rect.max.y),
                    ),
                    egui::Rect::from_min_max(rect.center(), rect.max),
                ],
            };

            for (view_type, view_rect) in active_views.iter().zip(rects.iter()) {
                // Slight padding for visual separation
                let padded_rect = view_rect.shrink(4.0);

                let child_builder = egui::UiBuilder::new()
                    .max_rect(padded_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min));

                let mut child_ui = ui.new_child(child_builder);

                // Ensure clipping so scrolling doesn't visually bleed
                child_ui.set_clip_rect(padded_rect);

                let id_salt = match view_type {
                    ViewType::Source => ui.id().with("source_view"),
                    ViewType::Tokens => ui.id().with("tokens_view"),
                    ViewType::AST => ui.id().with("ast_view"),
                    ViewType::IR => ui.id().with("ir_view"),
                };

                egui::Frame::window(child_ui.style()).show(&mut child_ui, |ui| {
                    // Make the frame clickable to optionally grab focus
                    let response = ui.interact(
                        ui.max_rect(),
                        id_salt.with("frame_interact"),
                        egui::Sense::click(),
                    );
                    if response.clicked() {
                        ui.memory_mut(|mem| mem.request_focus(ui.id()));
                    }

                    ui.horizontal(|ui| {
                        match view_type {
                            ViewType::Source => ui.strong("Source Code"),
                            ViewType::Tokens => ui.strong("Lexer Tokens"),
                            ViewType::AST => ui.strong("Parser AST"),
                            ViewType::IR => ui.strong("Intermediate Repr."),
                        };
                    });
                    ui.separator();

                    egui::ScrollArea::both()
                        .id_salt(id_salt)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match view_type {
                            ViewType::Source => {
                                let mut layouter =
                                    |ui: &egui::Ui,
                                     string: &dyn egui::TextBuffer,
                                     _wrap_width: f32| {
                                        let mut layout_job =
                                            highlight_code(ui.style(), string.as_str());
                                        layout_job.wrap.max_width = f32::INFINITY;
                                        ctx.fonts_mut(|f| f.layout_job(layout_job))
                                    };
                                ui.push_id(id_salt.with("source_editor"), |ui| {
                                    ui.add_sized(
                                        ui.available_size(),
                                        egui::TextEdit::multiline(&mut self.source_code)
                                            .font(egui::TextStyle::Monospace)
                                            .code_editor()
                                            .lock_focus(true)
                                            .layouter(&mut layouter),
                                    );
                                });
                            }
                            ViewType::Tokens => {
                                let mut layouter =
                                    |ui: &egui::Ui,
                                     string: &dyn egui::TextBuffer,
                                     _wrap_width: f32| {
                                        let mut layout_job =
                                            highlight_ast(ui.style(), string.as_str());
                                        layout_job.wrap.max_width = f32::INFINITY;
                                        ctx.fonts_mut(|f| f.layout_job(layout_job))
                                    };
                                ui.push_id(id_salt.with("tokens_editor"), |ui| {
                                    ui.add_sized(
                                        ui.available_size(),
                                        egui::TextEdit::multiline(&mut self.tokens_output)
                                            .font(egui::TextStyle::Monospace)
                                            .layouter(&mut layouter),
                                    );
                                });
                            }
                            ViewType::AST => {
                                let mut layouter =
                                    |ui: &egui::Ui,
                                     string: &dyn egui::TextBuffer,
                                     _wrap_width: f32| {
                                        let mut layout_job =
                                            highlight_ast(ui.style(), string.as_str());
                                        layout_job.wrap.max_width = f32::INFINITY;
                                        ctx.fonts_mut(|f| f.layout_job(layout_job))
                                    };
                                ui.push_id(id_salt.with("ast_editor"), |ui| {
                                    ui.add_sized(
                                        ui.available_size(),
                                        egui::TextEdit::multiline(&mut self.ast_output)
                                            .font(egui::TextStyle::Monospace)
                                            .layouter(&mut layouter),
                                    );
                                });
                            }
                            ViewType::IR => {
                                let mut layouter =
                                    |ui: &egui::Ui,
                                     string: &dyn egui::TextBuffer,
                                     _wrap_width: f32| {
                                        let mut layout_job =
                                            highlight_ir(ui.style(), string.as_str());
                                        layout_job.wrap.max_width = f32::INFINITY;
                                        ctx.fonts_mut(|f| f.layout_job(layout_job))
                                    };
                                ui.push_id(id_salt.with("ir_editor"), |ui| {
                                    ui.add_sized(
                                        ui.available_size(),
                                        egui::TextEdit::multiline(&mut self.ir_output)
                                            .font(egui::TextStyle::Monospace)
                                            .layouter(&mut layouter),
                                    );
                                });
                            }
                        });
                });
            }
        });
    }
}
