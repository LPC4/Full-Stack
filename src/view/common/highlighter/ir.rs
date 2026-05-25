// IR (Intermediate Representation) highlighting

use crate::view::ui_theme;
use egui::text::LayoutJob;

/// Highlights IR (Intermediate Representation) output with IR-specific keywords.
pub fn highlight_ir(theme: &egui::Style, code: &str) -> LayoutJob {
    let palette = ui_theme().syntax;
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
        "const",
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
                    color: palette.comment,
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

            // String literals: c"..." or "..."
            if bytes[start] == b'c' && start + 1 < len && bytes[start + 1] == b'"' {
                end = start + 2;
                while end < len && bytes[end] != b'"' {
                    if bytes[end] == b'\\' && end + 1 < len {
                        end += 2; // skip escape
                    } else {
                        end += 1;
                    }
                }
                if end < len {
                    end += 1; // include closing quote
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.string,
                        ..Default::default()
                    },
                );
                start = end;
                continue;
            }
            if bytes[start] == b'"' {
                end = start + 1;
                while end < len && bytes[end] != b'"' {
                    if bytes[end] == b'\\' && end + 1 < len {
                        end += 2; // skip escape
                    } else {
                        end += 1;
                    }
                }
                if end < len {
                    end += 1; // include closing quote
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.string,
                        ..Default::default()
                    },
                );
                start = end;
                continue;
            }

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
                            color: palette.label,
                            ..Default::default()
                        },
                    );
                } else {
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    let word = &line[start..end];
                    let color = if ir_keywords.contains(&word) {
                        palette.keyword
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
                        color: palette.number,
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
                        color: palette.register,
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
