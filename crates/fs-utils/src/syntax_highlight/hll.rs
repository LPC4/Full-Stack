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
];

const HLL_BUILTIN_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool",
];

#[derive(Default)]
struct HllSymbols {
    nominal_types: HashSet<String>,
    generic_types: HashSet<String>,
    enum_variants: HashSet<String>,
    function_names: HashSet<String>,
}

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
fn push_hll_code_part(out: &mut Out, base: usize, code_part: &str, symbols: &HllSymbols) {
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
            } else if HLL_BUILTIN_TYPES.contains(&word) {
                Class::BuiltinType
            } else if symbols.generic_types.contains(word) {
                Class::Generic
            } else if symbols.nominal_types.contains(word) {
                Class::Type
            } else if symbols.enum_variants.contains(word) {
                Class::Variant
            } else if symbols.function_names.contains(word) || followed_by_call(code_part, end) {
                Class::Function
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

fn followed_by_call(code: &str, mut index: usize) -> bool {
    let bytes = code.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if bytes.get(index) == Some(&b'(') {
        return true;
    }
    if bytes.get(index) != Some(&b'<') {
        return false;
    }

    let mut depth = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
            }
            _ => {}
        }
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    bytes.get(index) == Some(&b'(')
}

fn collect_generic_params(tokens: &[String], open: usize, symbols: &mut HllSymbols) {
    let mut index = open + 1;
    let mut expect_param = true;
    while index < tokens.len() && tokens[index] != ">" {
        let token = &tokens[index];
        if expect_param
            && token
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            symbols.generic_types.insert(token.clone());
            expect_param = false;
        } else if token == "," {
            expect_param = true;
        }
        index += 1;
    }
}

fn is_symbol_identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

fn matching_delimiter(tokens: &[String], open: usize, left: &str, right: &str) -> Option<usize> {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token == left {
            depth += 1;
        } else if token == right {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn function_signature_open(tokens: &[String], name: usize) -> Option<usize> {
    if tokens.get(name + 1).is_none_or(|token| token != ":") {
        return None;
    }
    let mut cursor = name + 2;
    if tokens.get(cursor).is_some_and(|token| token == "<") {
        cursor = matching_delimiter(tokens, cursor, "<", ">")? + 1;
    }
    tokens
        .get(cursor)
        .is_some_and(|token| token == "(")
        .then_some(cursor)
}

fn function_signature_has_body(tokens: &[String], open: usize) -> bool {
    let Some(close) = matching_delimiter(tokens, open, "(", ")") else {
        return false;
    };
    for token in &tokens[close + 1..] {
        match token.as_str() {
            "{" => return true,
            "=" | "," | ")" | "]" | "}" => return false,
            _ => {}
        }
    }
    false
}

fn collect_interface_methods(tokens: &[String], interface: usize, symbols: &mut HllSymbols) {
    let Some(open) = tokens
        .iter()
        .enumerate()
        .skip(interface + 2)
        .find_map(|(index, token)| (token == "{").then_some(index))
    else {
        return;
    };
    let Some(close) = matching_delimiter(tokens, open, "{", "}") else {
        return;
    };
    let mut paren_depth = 0;
    for index in open + 1..close {
        if paren_depth == 0
            && is_symbol_identifier(&tokens[index])
            && function_signature_open(tokens, index).is_some()
        {
            symbols.function_names.insert(tokens[index].clone());
        }
        match tokens[index].as_str() {
            "(" => paren_depth += 1,
            ")" => paren_depth -= 1,
            _ => {}
        }
    }
}

fn collect_enum_variants(tokens: &[String], enum_index: usize, symbols: &mut HllSymbols) {
    let Some(mut cursor) = tokens
        .iter()
        .enumerate()
        .skip(enum_index + 2)
        .find_map(|(index, token)| (token == "{").then_some(index + 1))
    else {
        return;
    };
    let mut brace_depth = 1;
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut angle_depth = 0;
    let mut expect_variant = true;

    while cursor < tokens.len() && brace_depth > 0 {
        let token = &tokens[cursor];
        if brace_depth == 1 && paren_depth == 0 && bracket_depth == 0 && angle_depth == 0 {
            if expect_variant && is_symbol_identifier(token) {
                symbols.enum_variants.insert(token.clone());
                expect_variant = false;
            } else if token == "," {
                expect_variant = true;
            }
        }
        match token.as_str() {
            "{" => brace_depth += 1,
            "}" => brace_depth -= 1,
            "(" => paren_depth += 1,
            ")" => paren_depth -= 1,
            "[" => bracket_depth += 1,
            "]" => bracket_depth -= 1,
            "<" => angle_depth += 1,
            ">" if angle_depth > 0 => angle_depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
}

// Collect declarations before rendering so unrelated capitalized values stay plain.
fn collect_symbols(code: &str) -> HllSymbols {
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

    let mut symbols = HllSymbols {
        nominal_types: ["Option", "Result", "Str"]
            .into_iter()
            .map(str::to_owned)
            .collect::<HashSet<_>>(),
        ..Default::default()
    };
    symbols.generic_types.insert("Self".to_owned());
    symbols.enum_variants = ["Some", "None", "Ok", "Err"]
        .into_iter()
        .map(str::to_owned)
        .collect();

    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            token.as_str(),
            "struct" | "enum" | "interface" | "type" | "impl" | "dyn"
        ) && let Some(name) = tokens.get(index + 1)
        {
            symbols.nominal_types.insert(name.clone());
            if tokens.get(index + 2).is_some_and(|token| token == "<") && token != "dyn" {
                collect_generic_params(&tokens, index + 2, &mut symbols);
            }
            if token == "enum" {
                collect_enum_variants(&tokens, index, &mut symbols);
            } else if token == "interface" {
                collect_interface_methods(&tokens, index, &mut symbols);
            }
        }
    }

    for index in 0..tokens.len() {
        if index > 0 && tokens[index] == "<" && tokens[index - 1] == ":" {
            collect_generic_params(&tokens, index, &mut symbols);
        }
        if is_symbol_identifier(&tokens[index])
            && let Some(open) = function_signature_open(&tokens, index)
            && (function_signature_has_body(&tokens, open)
                || index > 0 && tokens[index - 1] == "external")
        {
            symbols.function_names.insert(tokens[index].clone());
        }
    }
    symbols
}

fn is_register_word(word: &str) -> bool {
    asm_to_binary::isa::is_register_name(word)
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
    let symbols = collect_symbols(code);
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
                    &symbols,
                );
                in_inline_asm_block = false;
            } else {
                push_inline_asm_code_part(out, base, code_part);
            }
        } else if let Some((asm_idx, open_idx)) = find_inline_asm_open(code_part) {
            push_hll_code_part(out, base, &code_part[..asm_idx], &symbols);
            out.push(base + asm_idx, base + asm_idx + 3, Class::Keyword);
            if open_idx > asm_idx + 3 {
                push_hll_code_part(
                    out,
                    base + asm_idx + 3,
                    &code_part[asm_idx + 3..open_idx],
                    &symbols,
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
                    &symbols,
                );
            } else {
                push_inline_asm_code_part(out, after_base, after_open);
                in_inline_asm_block = true;
            }
        } else {
            push_hll_code_part(out, base, code_part, &symbols);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn classes_for(source: &str, word: &str) -> Vec<Class> {
        let mut out = Out::default();
        highlight(source, &mut out);
        out.spans
            .iter()
            .filter_map(|span| (&source[span.start..span.end] == word).then_some(span.class))
            .collect()
    }

    #[test]
    fn semantic_names_get_their_own_classes() {
        let source = r#"
interface Measured { measure: (self: Self*, callback: (i32) -> i32) -> i32 }
struct Box<T> { value: T, callback: (i32) -> i32 }
enum Shape { Unit, Circle(i32), Wrapped(Box<i32>) }
impl Box<T> { get: (self: Box<T>*) -> T { return self.value } }
max: <U: Measured>(left: U, right: U) -> U { return left }
main: () -> i32 {
    result: Box<i32> = factory.make<T>(value)
    other := max<U>(result, value)
    shape: Shape = Shape.Circle(1)
    empty := Shape.Unit
    optional := Option.Some(result)
    free(other)
    return 0
}
"#;

        assert!(
            classes_for(source, "Box")
                .iter()
                .all(|class| *class == Class::Type)
        );
        assert!(
            classes_for(source, "Measured")
                .iter()
                .all(|class| *class == Class::Type)
        );
        for generic in ["Self", "T", "U"] {
            assert!(
                classes_for(source, generic)
                    .iter()
                    .all(|class| *class == Class::Generic),
                "generic `{generic}`"
            );
        }
        assert!(
            classes_for(source, "i32")
                .iter()
                .all(|class| *class == Class::BuiltinType)
        );
        for function in ["measure", "get", "main", "make", "max", "free"] {
            assert!(
                classes_for(source, function)
                    .iter()
                    .all(|class| *class == Class::Function),
                "function `{function}`"
            );
        }
        for variant in ["Unit", "Circle", "Wrapped", "Some"] {
            assert!(
                classes_for(source, variant)
                    .iter()
                    .all(|class| *class == Class::Variant),
                "variant `{variant}`"
            );
        }
        for variable in [
            "result", "factory", "value", "other", "shape", "empty", "optional", "callback",
        ] {
            assert!(
                classes_for(source, variable)
                    .iter()
                    .all(|class| *class == Class::Text),
                "variable `{variable}`"
            );
        }
        assert!(!classes_for(source, "Circle").contains(&Class::Function));
    }

    #[test]
    fn concrete_type_arguments_do_not_become_generic_parameters() {
        let source = "struct Box<T> { value: T }\nitem: Box<Widget>\n";
        assert_eq!(classes_for(source, "T"), [Class::Generic, Class::Generic]);
        assert_eq!(classes_for(source, "Widget"), [Class::Text]);
    }
}
