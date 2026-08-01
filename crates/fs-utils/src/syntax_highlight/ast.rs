// AST and token dumps.

use super::{Class, Out};

pub(crate) fn highlight(code: &str, out: &mut Out) {
    let mut start = 0;
    let bytes = code.as_bytes();
    let len = bytes.len();

    while start < len {
        let mut end = start;
        let c = bytes[start];

        if c.is_ascii_alphabetic() || c == b'_' {
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            out.push(start, end, Class::Identifier);
        } else if c.is_ascii_digit() {
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            out.push(start, end, Class::Number);
        } else if matches!(c, b'{' | b'}' | b'[' | b']' | b'(' | b')') {
            end += 1;
            out.push(start, end, Class::Bracket);
        } else if c == b'"' || c == b'\'' {
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
            out.push(start, end, Class::String);
        } else {
            while end < len
                && !bytes[end].is_ascii_alphanumeric()
                && !matches!(
                    bytes[end],
                    b'_' | b'{' | b'}' | b'[' | b']' | b'(' | b')' | b'"' | b'\''
                )
            {
                end += 1;
            }
            out.push(start, end, Class::Text);
        }
        start = end;
    }
}
