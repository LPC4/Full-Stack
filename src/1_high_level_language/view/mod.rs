use crate::high_level_language::compiler::{HighLevelCompiler, SemanticAnalyzer};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::Parser;
use crate::high_level_language::token::Token;
use egui::Color32;
use egui::text::LayoutJob;

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
        // Embed the startup example so web builds don't depend on runtime filesystem access.
        let default_source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/programs/example/example_program.hll"
        ))
        .to_owned();

        Self {
            source_code: default_source,
            show_source: true,
            show_tokens: false,
            show_ast: false,
            show_ir: false,
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
        "f32", "f64", "bool", "Str",
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
        "load",
        "store",
        "stack_alloc",
        "heap_alloc",
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
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
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
    pub fn compile(&mut self) {
        let mut lexer = Lexer::new(&self.source_code);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            if let Token::Error(ref msg) = token {
                let error_msg = msg.clone();
                tokens.push(token);
                self.tokens_output = format!("{:#?}\n\nLEXER ERROR: {}", tokens, error_msg);
                self.ast_output = String::from("Did not parse due to lexer error.");
                self.ir_output = String::from("Did not compile due to lexer error.");
                self.compile_error = Some(format!("Lexer error: {}", error_msg));
                self.compile_success_until = None;
                return;
            }
            let is_eof = matches!(token, Token::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        self.tokens_output = format!("{tokens:#?}");

        let mut parser = Parser::new(tokens);
        match parser.parse_program() {
            Ok(ast) => {
                self.ast_output = format!("{ast:#?}");

                // Semantic analysis pass
                let mut semantic_analyzer = SemanticAnalyzer::new();
                match semantic_analyzer.analyze_program(&ast) {
                    Ok(_) => {
                        // Compile to IR
                        let mut compiler = HighLevelCompiler::new();
                        match compiler.compile_program(&ast) {
                            Ok(ir_program) => {
                                // Check diagnostics for any semantic errors
                                let errors: Vec<_> = compiler
                                    .diagnostics()
                                    .iter()
                                    .filter(|d| {
                                        matches!(
                                            d.level,
                                            crate::high_level_language::compiler::DiagnosticLevel::Error
                                        )
                                    })
                                    .map(|d| d.message.clone())
                                    .collect();

                                self.ir_output = format!("{}", ir_program);

                                if errors.is_empty() {
                                    self.compile_error = None;
                                    self.just_compiled_successfully = true;
                                } else {
                                    self.ir_output.push_str(&format!(
                                        "\n\nSEMANTIC ERRORS:\n- {}",
                                        errors.join("\n- ")
                                    ));
                                    self.compile_error =
                                        Some(format!("Semantic errors:\n- {}", errors.join("\n- ")));
                                    self.compile_success_until = None;
                                    self.just_compiled_successfully = false;
                                }
                            }
                            Err(e) => {
                                self.ir_output = format!("Compiler internal error: {:?}", e);
                                self.compile_error = Some(format!("Compiler error: {:?}", e));
                                self.compile_success_until = None;
                                self.just_compiled_successfully = false;
                            }
                        }
                    }
                    Err(_) => {
                        // Semantic analysis failed
                        let semantic_errors: Vec<_> = semantic_analyzer
                            .diagnostics()
                            .iter()
                            .map(|d| d.message.clone())
                            .collect();
                        self.ir_output = format!("Did not compile due to semantic errors.");
                        self.compile_error =
                            Some(format!("Semantic analysis failed:\n- {}", semantic_errors.join("\n- ")));
                        self.compile_success_until = None;
                        self.just_compiled_successfully = false;
                    }
                }
            }
            Err(e) => {
                self.ast_output = format!("Parser Error at pos {}: {}", e.pos, e.message);
                self.ir_output = String::from("Did not compile due to parser error.");
                self.compile_error = Some(format!("Parser error at pos {}: {}", e.pos, e.message));
                self.compile_success_until = None;
                self.just_compiled_successfully = false;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Trigger compile on Ctrl+S
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.compile();
        }

        if self.just_compiled_successfully {
            self.just_compiled_successfully = false;
            self.compile_success_until = Some(ui.input(|i| i.time) + 2.0);
        }

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
                    ViewType::Source => "source_id",
                    ViewType::Tokens => "tokens_id",
                    ViewType::AST => "ast_id",
                    ViewType::IR => "ir_id",
                };

                egui::Frame::window(child_ui.style()).show(&mut child_ui, |ui| {
                    // Make the frame clickable to optionally grab focus
                    let response = ui.interact(
                        ui.max_rect(),
                        ui.id().with("frame_interact"),
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
                                ui.add_sized(
                                    ui.available_size(),
                                    egui::TextEdit::multiline(&mut self.source_code)
                                        .font(egui::TextStyle::Monospace)
                                        .code_editor()
                                        .lock_focus(true)
                                        .layouter(&mut layouter),
                                );
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
                                ui.add_sized(
                                    ui.available_size(),
                                    egui::TextEdit::multiline(&mut self.tokens_output)
                                        .font(egui::TextStyle::Monospace)
                                        .layouter(&mut layouter),
                                );
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
                                ui.add_sized(
                                    ui.available_size(),
                                    egui::TextEdit::multiline(&mut self.ast_output)
                                        .font(egui::TextStyle::Monospace)
                                        .layouter(&mut layouter),
                                );
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
                                ui.add_sized(
                                    ui.available_size(),
                                    egui::TextEdit::multiline(&mut self.ir_output)
                                        .font(egui::TextStyle::Monospace)
                                        .layouter(&mut layouter),
                                );
                            }
                        });
                });
            }
        });
    }
}
