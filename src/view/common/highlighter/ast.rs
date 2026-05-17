// AST and token output highlighting

use crate::view::ui_theme;
use egui::text::LayoutJob;

/// Highlights AST and token output.
pub fn highlight_ast(theme: &egui::Style, code: &str) -> LayoutJob {
    let palette = ui_theme().syntax;
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
                    color: palette.identifier,
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
                    color: palette.number,
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
                    color: palette.bracket,
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
                    color: palette.string,
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
