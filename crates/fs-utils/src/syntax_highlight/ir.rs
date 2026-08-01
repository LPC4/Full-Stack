// IR (Intermediate Representation) dumps.

use super::{Class, Out};

const IR_KEYWORDS: &[&str] = &[
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

    // Labels and functions
    if bytes[start] == b'@' {
        end += 1;
        while end < len
            && (bytes[end].is_ascii_alphanumeric()
                || matches!(bytes[end], b'_' | b'.' | b'<' | b'>'))
        {
            end += 1;
        }
        return (end, Class::Label);
    }

    if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
        while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let class = if IR_KEYWORDS.contains(&&line[start..end]) {
            Class::Keyword
        } else {
            Class::Text
        };
        return (end, class);
    }

    if bytes[start].is_ascii_digit() {
        while end < len && bytes[end].is_ascii_digit() {
            end += 1;
        }
        return (end, Class::Number);
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
        && !matches!(bytes[end], b'_' | b'$' | b'@')
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
