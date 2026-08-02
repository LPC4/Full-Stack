// IR (Intermediate Representation) dumps.

use super::{Class, Out};

const IR_KEYWORDS: &[&str] = &[
    "type",
    "export",
    "extern",
    "define",
    "global",
    "zero",
    "bytes",
    "fn",
    "entry",
    "branch",
    "jump",
    "ret",
    "trap",
    "call",
    "indirect_call",
    "function_addr",
    "global_ref",
    "load",
    "store",
    "index",
    "stack_alloc",
    "heap_alloc",
    "heap_free",
    "offset",
    "add",
    "sub",
    "mul",
    "div",
    "sdiv",
    "udiv",
    "mod",
    "umod",
    "shl",
    "shr",
    "and",
    "or",
    "xor",
    "cmp",
    "neg",
    "not",
    "bitnot",
    "trunc",
    "zext",
    "sext",
    "bitcast",
    "f2i",
    "i2f",
    "to",
    "phi",
    "inline_asm",
    "read_reg",
    "const",
];

// Walks to just past the closing quote, or to the end of the line if unterminated.
fn string_end(bytes: &[u8], from: usize) -> usize {
    let len = bytes.len();
    let mut end = from;
    while end < len && bytes[end] != b'"' {
        if bytes[end] == b'\\' && end + 1 < len {
            end += 2; // skip escape
        } else {
            end += 1;
        }
    }
    if end < len { end + 1 } else { end }
}

fn scan_token(line: &str, start: usize) -> (usize, Class) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut end = start;

    // String literals: c"..." or "..."
    if bytes[start] == b'c' && start + 1 < len && bytes[start + 1] == b'"' {
        return (string_end(bytes, start + 2), Class::String);
    }
    if bytes[start] == b'"' {
        return (string_end(bytes, start + 1), Class::String);
    }

    if bytes[start] == b'-' && start + 3 < len && &bytes[start + 1..start + 4] == b"inf" {
        return (start + 4, Class::Number);
    }

    let digit_start =
        if bytes[start] == b'-' && start + 1 < len && bytes[start + 1].is_ascii_digit() {
            start + 1
        } else {
            start
        };
    if bytes[digit_start].is_ascii_digit() {
        end = digit_start;
        if bytes[end] == b'0' && end + 1 < len && matches!(bytes[end + 1], b'x' | b'X') {
            end += 2;
            while end < len && bytes[end].is_ascii_hexdigit() {
                end += 1;
            }
        } else {
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end < len && bytes[end] == b'.' {
                end += 1;
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            if end < len && matches!(bytes[end], b'e' | b'E') {
                end += 1;
                if end < len && matches!(bytes[end], b'+' | b'-') {
                    end += 1;
                }
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
        }
        return (end, Class::Number);
    }

    if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
        while end < len
            && (bytes[end].is_ascii_alphanumeric()
                || matches!(bytes[end], b'_' | b'.' | b'<' | b'>'))
        {
            end += 1;
        }
        let token = &line[start..end];
        let class = if matches!(token, "inf" | "nan") {
            Class::Number
        } else if IR_KEYWORDS.contains(&token) {
            Class::Keyword
        } else {
            Class::Text
        };
        return (end, class);
    }

    if bytes[start] == b'$' {
        end += 1;
        while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        return (end, Class::Register);
    }

    while end < len
        && !bytes[end].is_ascii_alphanumeric()
        && !matches!(bytes[end], b'_' | b'$')
        && !(bytes[end] == b'-'
            && end + 1 < len
            && (bytes[end + 1].is_ascii_digit()
                || (end + 3 < len && &bytes[end + 1..end + 4] == b"inf")))
    {
        end += 1;
    }
    (end, Class::Text)
}

pub(crate) fn highlight(code: &str, out: &mut Out) {
    let mut base = 0;

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = match segment.strip_suffix('\n') {
            Some(stripped) => (stripped, true),
            None => (segment, false),
        };
        let len = line.len();

        if line.trim_start().starts_with(';') {
            out.push(base, base + len, Class::Comment);
        } else {
            let mut start = 0;
            while start < len {
                let (end, class) = scan_token(line, start);
                out.push(base + start, base + end, class);
                start = end;
            }
        }

        if has_newline {
            out.push(base + len, base + len + 1, Class::Plain);
        }
        base += segment.len();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Class, Lang, highlight};

    #[test]
    fn recognizes_ir_2_keywords_and_registers() {
        let source = "export define add_one($value: i32) -> i32 {\n\
                      entry:\n\
                          $result = add i32 $value, 1\n\
                          store i32 $result, $value\n\
                          ret $result\n\
                      }\n";
        let spans = highlight(Lang::Ir, source);
        let keywords: Vec<_> = spans
            .iter()
            .filter(|span| span.class == Class::Keyword)
            .map(|span| &source[span.start..span.end])
            .collect();
        let registers = spans
            .iter()
            .filter(|span| span.class == Class::Register)
            .count();

        assert!(keywords.contains(&"export"));
        assert!(keywords.contains(&"define"));
        assert!(keywords.contains(&"add"));
        assert!(keywords.contains(&"store"));
        assert!(keywords.contains(&"ret"));
        assert_eq!(registers, 6);
    }

    #[test]
    fn recognizes_ir_2_numeric_forms() {
        let source = "bytes[0xff; 4] -2.5 1e-9 inf -inf nan";
        let spans = highlight(Lang::Ir, source);
        let numbers: Vec<_> = spans
            .iter()
            .filter(|span| span.class == Class::Number)
            .map(|span| &source[span.start..span.end])
            .collect();

        assert_eq!(numbers, ["0xff", "4", "-2.5", "1e-9", "inf", "-inf", "nan"]);
    }
}
