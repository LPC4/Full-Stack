// HLL source, including the inline `asm { ... }` sub-dialect.

use super::{Class, Out};
use std::collections::HashSet;

// Keep in sync with the lexer's keyword table (crates/hll-to-ir/src/lexer.rs).
const HLL_KEYWORDS: &[&str] = &[
    "type",
    "struct",
    "impl",
    "interface",
    "dyn",
    "const",
    "external",
    "if",
    "else",
    "while",
    "for",
    "in",
    "enum",
    "match",
    "break",
    "continue",
    "return",
    "defer",
    "new",
    "free",
    "assert",
    "asm",
    "and",
    "or",
    "not",
    "true",
    "false",
    "null",
    "fn",
    "as",
    "import",
    "export",
    "main",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "f32",
    "f64",
    "bool",
    "Self",
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

// `base` is where `code_part` starts in the whole source, so spans stay absolute.
fn push_hll_code_part(out: &mut Out, base: usize, code_part: &str, type_names: &HashSet<String>) {
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
            out.push(base + start, base + end, Class::String);
            start = end;
            continue;
        }

        // identifier / keyword
        if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let word = &code_part[start..end];
            let class = if HLL_KEYWORDS.contains(&word) {
                Class::Keyword
            } else if type_names.contains(word) {
                Class::Identifier
            } else {
                Class::Text
            };
            out.push(base + start, base + end, class);
            start = end;
            continue;
        }

        // number (decimal, hex 0x..., binary 0b..., octal 0o...)
        if bytes[start].is_ascii_digit() {
            end = start + 1;
            if bytes[start] == b'0' && end < len {
                match bytes[end] {
                    b'x' | b'X' => {
                        end += 1;
                        while end < len && (bytes[end].is_ascii_hexdigit() || bytes[end] == b'_') {
                            end += 1;
                        }
                    }
                    b'b' | b'B' => {
                        end += 1;
                        while end < len && matches!(bytes[end], b'0' | b'1' | b'_') {
                            end += 1;
                        }
                    }
                    b'o' | b'O' => {
                        end += 1;
                        while end < len
                            && ((b'0'..=b'7').contains(&bytes[end]) || bytes[end] == b'_')
                        {
                            end += 1;
                        }
                    }
                    _ => {
                        while end < len && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                            end += 1;
                        }
                    }
                }
            } else {
                while end < len && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                    end += 1;
                }
            }
            out.push(base + start, base + end, Class::Number);
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
        out.push(base + start, base + end, Class::Text);
        start = end;
    }
}

// Collect nominal declarations and generic parameters before rendering. This keeps
// highlighting contextual: `T` is a type, while an unrelated capitalized value is not.
fn collect_type_names(code: &str) -> HashSet<String> {
    let mut tokens = Vec::new();
    for line in code.lines() {
        let code = find_comment_start_outside_string(line).map_or(line, |at| &line[..at]);
        let bytes = code.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'"' || bytes[index] == b'\'' {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' && index + 1 < bytes.len() {
                        index += 2;
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push(code[start..index].to_owned());
            } else if !bytes[index].is_ascii_whitespace() {
                tokens.push((bytes[index] as char).to_string());
                index += 1;
            } else {
                index += 1;
            }
        }
    }

    let mut names = ["Option", "Result", "Str"]
        .into_iter()
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            token.as_str(),
            "struct" | "enum" | "interface" | "type" | "impl" | "dyn"
        ) && let Some(name) = tokens.get(index + 1)
        {
            names.insert(name.clone());
        }
    }

    for index in 0..tokens.len() {
        if tokens[index] != "<" {
            continue;
        }
        let starts_params =
            index > 0 && (tokens[index - 1] == ":" || names.contains(&tokens[index - 1]));
        if !starts_params {
            continue;
        }
        let mut cursor = index + 1;
        let mut expect_param = true;
        while cursor < tokens.len() && tokens[cursor] != ">" {
            let token = &tokens[cursor];
            if expect_param
                && token
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
            {
                names.insert(token.clone());
                expect_param = false;
            } else if token == "," {
                expect_param = true;
            }
            cursor += 1;
        }
    }
    names
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

fn push_inline_asm_code_part(out: &mut Out, base: usize, code_part: &str) {
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
            out.push(base + start, base + end, Class::Text);
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
            let class = if is_label {
                Class::Label
            } else if is_register_word(word) {
                Class::Register
            } else if word.starts_with('.') {
                Class::Directive
            } else if !seen_mnemonic {
                Class::Keyword
            } else {
                Class::Text
            };
            if !is_label && !is_register_word(word) && !word.starts_with('.') {
                seen_mnemonic = true;
            }
            out.push(base + start, base + end, class);
            start = end;
            continue;
        }

        if b.is_ascii_digit() || (b == b'-' && start + 1 < len && bytes[start + 1].is_ascii_digit())
        {
            end += 1;
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'x') {
                end += 1;
            }
            out.push(base + start, base + end, Class::Number);
            start = end;
            continue;
        }

        let class = if b == b'{' || b == b'}' {
            Class::Bracket
        } else {
            Class::Text
        };
        out.push(base + start, base + start + 1, class);
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

        if &bytes[i..i + 3] == b"asm" {
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

pub(crate) fn highlight(code: &str, out: &mut Out) {
    let type_names = collect_type_names(code);
    let mut in_inline_asm_block = false;
    let mut base = 0;

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = match segment.strip_suffix('\n') {
            Some(stripped) => (stripped, true),
            None => (segment, false),
        };

        let comment_start = find_comment_start_outside_string(line);

        // split into code part and optional comment part
        let (code_part, comment_part) = match comment_start {
            Some(idx) => (&line[..idx], Some(idx)),
            None => (line, None),
        };

        if in_inline_asm_block {
            if let Some(close_idx) = code_part.find('}') {
                push_inline_asm_code_part(out, base, &code_part[..close_idx]);
                out.push(base + close_idx, base + close_idx + 1, Class::Bracket);
                push_hll_code_part(
                    out,
                    base + close_idx + 1,
                    &code_part[close_idx + 1..],
                    &type_names,
                );
                in_inline_asm_block = false;
            } else {
                push_inline_asm_code_part(out, base, code_part);
            }
        } else if let Some((asm_idx, open_idx)) = find_inline_asm_open(code_part) {
            push_hll_code_part(out, base, &code_part[..asm_idx], &type_names);
            out.push(base + asm_idx, base + asm_idx + 3, Class::Keyword);
            if open_idx > asm_idx + 3 {
                push_hll_code_part(
                    out,
                    base + asm_idx + 3,
                    &code_part[asm_idx + 3..open_idx],
                    &type_names,
                );
            }
            out.push(base + open_idx, base + open_idx + 1, Class::Bracket);

            let after_base = base + open_idx + 1;
            let after_open = &code_part[open_idx + 1..];
            if let Some(close_rel) = after_open.find('}') {
                push_inline_asm_code_part(out, after_base, &after_open[..close_rel]);
                out.push(
                    after_base + close_rel,
                    after_base + close_rel + 1,
                    Class::Bracket,
                );
                push_hll_code_part(
                    out,
                    after_base + close_rel + 1,
                    &after_open[close_rel + 1..],
                    &type_names,
                );
            } else {
                push_inline_asm_code_part(out, after_base, after_open);
                in_inline_asm_block = true;
            }
        } else {
            push_hll_code_part(out, base, code_part, &type_names);
        }

        if let Some(idx) = comment_part {
            out.push(base + idx, base + line.len(), Class::Comment);
        }

        if has_newline {
            out.push(base + line.len(), base + line.len() + 1, Class::Plain);
        }
        base += segment.len();
    }
}
