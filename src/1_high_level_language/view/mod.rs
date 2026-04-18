use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::Parser;
use crate::high_level_language::token::Token;
use egui::Color32;
use egui::text::LayoutJob;

#[derive(serde::Deserialize, serde::Serialize, PartialEq)]
enum Tab {
    Source,
    Tokens,
    AST,
}

/// High-level language visualization state and UI.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct HighLevelLanguageView {
    source_code: String,

    #[serde(skip)]
    current_tab: Tab,
    #[serde(skip)]
    tokens_output: String,
    #[serde(skip)]
    ast_output: String,
    #[serde(skip)]
    compile_error: Option<String>,
}

impl Default for HighLevelLanguageView {
    fn default() -> Self {
        // Embed the startup example so web builds don't depend on runtime filesystem access.
        let default_source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/programs/test/high_level_language/test2.hll"
        ))
        .to_owned();

        Self {
            source_code: default_source,
            current_tab: Tab::Source,
            tokens_output: String::new(),
            ast_output: String::new(),
            compile_error: None,
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

impl HighLevelLanguageView {
    pub fn compile(&mut self) {
        let mut lexer = Lexer::new(&self.source_code);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
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
                self.compile_error = None;
            }
            Err(e) => {
                self.ast_output = String::new();
                self.compile_error = Some(format!("Parser error at pos {}: {}", e.pos, e.message));
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("high_level_language_top_panel").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label("Compiler Visualization");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Source, "Source Code");
                ui.selectable_value(&mut self.current_tab, Tab::Tokens, "Lexer Tokens");
                ui.selectable_value(&mut self.current_tab, Tab::AST, "Parser AST");

                if ui.button("Compile").clicked() {
                    self.compile();
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(err) = &self.compile_error {
                ui.colored_label(egui::Color32::RED, err);
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.current_tab {
                    Tab::Source => {
                        let mut layouter =
                            |ui: &egui::Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                                let mut layout_job = highlight_code(ui.style(), string.as_str());
                                layout_job.wrap.max_width = wrap_width;
                                ui.ctx().fonts_mut(|f| f.layout_job(layout_job))
                            };

                        ui.add(
                            egui::TextEdit::multiline(&mut self.source_code)
                                .font(egui::TextStyle::Monospace) // for cursor height
                                .code_editor()
                                .desired_rows(30)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter),
                        );
                    }
                    Tab::Tokens => {
                        let mut layouter =
                            |ui: &egui::Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                                let mut layout_job = highlight_ast(ui.style(), string.as_str());
                                layout_job.wrap.max_width = wrap_width;
                                ui.ctx().fonts_mut(|f| f.layout_job(layout_job))
                            };

                        ui.add(
                            egui::TextEdit::multiline(&mut self.tokens_output)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false)
                                .layouter(&mut layouter),
                        );
                    }
                    Tab::AST => {
                        let mut layouter =
                            |ui: &egui::Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
                                let mut layout_job = highlight_ast(ui.style(), string.as_str());
                                layout_job.wrap.max_width = wrap_width;
                                ui.ctx().fonts_mut(|f| f.layout_job(layout_job))
                            };

                        ui.add(
                            egui::TextEdit::multiline(&mut self.ast_output)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false)
                                .layouter(&mut layouter),
                        );
                    }
                }
            });
        });
    }
}
