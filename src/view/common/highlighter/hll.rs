// HLL source code highlighting with inline asm block detection

use crate::view::ui_theme;
use egui::text::LayoutJob;

const HLL_KEYWORDS: &[&str] = &[
    "type", "const", "if", "else", "while", "return", "defer", "new", "free", "and", "or", "true",
    "false", "null", "main", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    "bool", "asm", "external",
];

fn find_comment_start_outside_string(line: &str) -> Option<usize> {
    let mut comment_start = None;
    let mut in_string = false;
    let mut string_quote = b'\0';
    let mut escape = false;

    for (i, &b) in line.as_bytes().iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape = true;
            } else if b == string_quote {
                in_string = false;
            }
        } else if b == b';' {
            comment_start = Some(i);
            break;
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
        }
    }

    comment_start
}

fn append_hll_code_part(job: &mut LayoutJob, theme: &egui::Style, code_part: &str) {
    let palette = ui_theme().syntax;
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut start = 0;
    let bytes = code_part.as_bytes();
    let len = bytes.len();

    while start < len {
        let mut end = start;

        // string literal
        if bytes[start] == b'"' || bytes[start] == b'\'' {
            let quote = bytes[start];
            end = start + 1;
            while end < len {
                if bytes[end] == b'\\' && end + 1 < len {
                    end += 2; // skip escaped char
                } else if bytes[end] == quote {
                    end += 1;
                    break;
                } else {
                    end += 1;
                }
            }
            job.append(
                &code_part[start..end],
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

        // identifier / keyword
        if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let word = &code_part[start..end];
            let color = if HLL_KEYWORDS.contains(&word) {
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
            start = end;
            continue;
        }

        // number (decimal, hex 0x..., binary 0b..., octal 0o...)
        if bytes[start].is_ascii_digit() {
            end = start + 1;
            // detect hex, binary, or octal prefix
            if bytes[start] == b'0' && end < len {
                match bytes[end] {
                    b'x' | b'X' => {
                        end += 1;
                        while end < len && bytes[end].is_ascii_hexdigit() {
                            end += 1;
                        }
                    }
                    b'b' | b'B' => {
                        end += 1;
                        while end < len && (bytes[end] == b'0' || bytes[end] == b'1') {
                            end += 1;
                        }
                    }
                    b'o' | b'O' => {
                        end += 1;
                        while end < len && (b'0'..=b'7').contains(&bytes[end]) {
                            end += 1;
                        }
                    }
                    _ => {
                        while end < len && bytes[end].is_ascii_digit() {
                            end += 1;
                        }
                    }
                }
            } else {
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.number,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        // other symbols (operators, whitespace, etc.)
        while end < len
            && !bytes[end].is_ascii_alphanumeric()
            && bytes[end] != b'_'
            && bytes[end] != b'"'
            && bytes[end] != b'\''
        {
            end += 1;
        }
        job.append(
            &code_part[start..end],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: theme.visuals.text_color(),
                ..Default::default()
            },
        );
        start = end;
    }
}

fn is_register_word(word: &str) -> bool {
    const ABI_INT_REGS: &[&str] = &[
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "s0", "s1", "s2",
        "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "a0", "a1", "a2", "a3", "a4", "a5",
        "a6", "a7",
    ];

    const ABI_FP_REGS: &[&str] = &[
        "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fs2", "fs3", "fs4",
        "fs5", "fs6", "fs7", "fs8", "fs9", "fs10", "fs11", "fa0", "fa1", "fa2", "fa3", "fa4",
        "fa5", "fa6", "fa7",
    ];

    ABI_INT_REGS.contains(&word)
        || ABI_FP_REGS.contains(&word)
        || (word.starts_with('x') && word[1..].chars().all(|c| c.is_ascii_digit()))
        || (word.starts_with('f') && word[1..].chars().all(|c| c.is_ascii_digit()))
}

fn append_inline_asm_code_part(job: &mut LayoutJob, theme: &egui::Style, code_part: &str) {
    let palette = ui_theme().syntax;
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut start = 0;
    let bytes = code_part.as_bytes();
    let len = bytes.len();
    let mut seen_mnemonic = false;

    while start < len {
        let mut end = start;
        let b = bytes[start];

        if b.is_ascii_whitespace() {
            while end < len && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: theme.visuals.text_color(),
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' || b == b'.' {
            while end < len
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'.')
            {
                end += 1;
            }
            let word = &code_part[start..end];
            let is_label = end < len && bytes[end] == b':';
            let color = if is_label {
                palette.label
            } else if is_register_word(word) {
                palette.register
            } else if word.starts_with('.') {
                palette.directive
            } else if !seen_mnemonic {
                palette.keyword
            } else {
                theme.visuals.text_color()
            };
            if !is_label && !is_register_word(word) && !word.starts_with('.') {
                seen_mnemonic = true;
            }
            job.append(
                word,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        if b.is_ascii_digit() || (b == b'-' && start + 1 < len && bytes[start + 1].is_ascii_digit())
        {
            end += 1;
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'x') {
                end += 1;
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.number,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        let color = if b == b'{' || b == b'}' {
            palette.bracket
        } else {
            theme.visuals.text_color()
        };
        job.append(
            &code_part[start..start + 1],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                ..Default::default()
            },
        );
        start += 1;
    }
}

fn find_inline_asm_open(code_part: &str) -> Option<(usize, usize)> {
    let bytes = code_part.as_bytes();
    if bytes.len() < 4 {
        return None;
    }

    let mut i = 0;
    let mut in_string = false;
    let mut string_quote = b'\0';
    let mut escape = false;

    while i + 3 <= bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape = true;
            } else if b == string_quote {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
            i += 1;
            continue;
        }

        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"asm" {
            let left_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let right_ok = i + 3 == bytes.len()
                || !(bytes[i + 3].is_ascii_alphanumeric() || bytes[i + 3] == b'_');
            if left_ok && right_ok {
                let mut j = i + 3;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    return Some((i, j));
                }
            }
        }
        i += 1;
    }

    None
}

/// Highlights source code with basic syntax highlighting for the HLL.
pub fn highlight_code(theme: &egui::Style, code: &str) -> LayoutJob {
    let palette = ui_theme().syntax;
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut in_inline_asm_block = false;

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        let comment_start = find_comment_start_outside_string(line);

        // split into code part and optional comment part
        let (code_part, comment_part) = if let Some(idx) = comment_start {
            (&line[..idx], Some(&line[idx..]))
        } else {
            (line, None)
        };

        if in_inline_asm_block {
            if let Some(close_idx) = code_part.find('}') {
                let asm_part = &code_part[..close_idx];
                append_inline_asm_code_part(&mut job, theme, asm_part);
                job.append(
                    "}",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.bracket,
                        ..Default::default()
                    },
                );
                append_hll_code_part(&mut job, theme, &code_part[close_idx + 1..]);
                in_inline_asm_block = false;
            } else {
                append_inline_asm_code_part(&mut job, theme, code_part);
            }
        } else if let Some((asm_idx, open_idx)) = find_inline_asm_open(code_part) {
            append_hll_code_part(&mut job, theme, &code_part[..asm_idx]);
            job.append(
                "asm",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.keyword,
                    ..Default::default()
                },
            );
            if open_idx > asm_idx + 3 {
                append_hll_code_part(&mut job, theme, &code_part[asm_idx + 3..open_idx]);
            }
            job.append(
                "{",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.bracket,
                    ..Default::default()
                },
            );

            let after_open = &code_part[open_idx + 1..];
            if let Some(close_rel) = after_open.find('}') {
                append_inline_asm_code_part(&mut job, theme, &after_open[..close_rel]);
                job.append(
                    "}",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.bracket,
                        ..Default::default()
                    },
                );
                append_hll_code_part(&mut job, theme, &after_open[close_rel + 1..]);
            } else {
                append_inline_asm_code_part(&mut job, theme, after_open);
                in_inline_asm_block = true;
            }
        } else {
            append_hll_code_part(&mut job, theme, code_part);
        }

        // ---------- append the comment part (if any) ----------
        if let Some(comment) = comment_part {
            job.append(
                comment,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.comment,
                    ..Default::default()
                },
            );
        }

        // append newline
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
