// GNU linker scripts: the commands, the section names and the location counter.

use crate::{Class, Out};

const COMMANDS: &[&str] = &[
    "ENTRY",
    "SECTIONS",
    "MEMORY",
    "PHDRS",
    "OVERLAY",
    "OUTPUT",
    "OUTPUT_ARCH",
    "OUTPUT_FORMAT",
    "INPUT",
    "GROUP",
    "INCLUDE",
    "SEARCH_DIR",
    "STARTUP",
    "TARGET",
    "EXTERN",
    "ASSERT",
    "PROVIDE",
    "PROVIDE_HIDDEN",
    "HIDDEN",
    "KEEP",
    "SORT",
    "SORT_BY_NAME",
    "SORT_BY_ALIGNMENT",
    "ALIGN",
    "ALIGNOF",
    "SUBALIGN",
    "ADDR",
    "LOADADDR",
    "SIZEOF",
    "SIZEOF_HEADERS",
    "ORIGIN",
    "LENGTH",
    "ABSOLUTE",
    "DEFINED",
    "SEGMENT_START",
    "MAX",
    "MIN",
    "FILL",
    "BYTE",
    "SHORT",
    "LONG",
    "QUAD",
    "SQUAD",
    "AT",
    "NOLOAD",
    "COPY",
    "INFO",
    "OVERLAY_END",
];

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'*' | b'?' | b'[' | b']')
}

// The whole source at once, because `/* ... */` is free to cross line ends.
pub(crate) fn highlight(code: &str, out: &mut Out) {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut start = 0;

    while start < len {
        let mut end = start;

        if bytes[start] == b'/' && start + 1 < len && bytes[start + 1] == b'*' {
            end = start + 2;
            while end + 1 < len && !(bytes[end] == b'*' && bytes[end + 1] == b'/') {
                end += 1;
            }
            end = (end + 2).min(len);
            out.push(start, end, Class::Comment);
            start = end;
            continue;
        }

        if bytes[start].is_ascii_whitespace() {
            while end < len && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            out.push(start, end, Class::Plain);
            start = end;
            continue;
        }

        if bytes[start] == b'"' {
            end += 1;
            while end < len && bytes[end] != b'"' {
                end += 1;
            }
            end = (end + 1).min(len);
            out.push(start, end, Class::String);
            start = end;
            continue;
        }

        if bytes[start] == b'{' || bytes[start] == b'}' {
            out.push(start, start + 1, Class::Bracket);
            start += 1;
            continue;
        }

        // Numbers, including hex and the K/M size suffixes.
        if bytes[start].is_ascii_digit() {
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            out.push(start, end, Class::Number);
            start = end;
            continue;
        }

        if is_word_byte(bytes[start]) {
            while end < len && is_word_byte(bytes[end]) {
                end += 1;
            }
            let word = &code[start..end];
            let class = if COMMANDS.contains(&word) {
                Class::Keyword
            } else if word.starts_with('.') {
                // A section name, or the bare `.` location counter.
                Class::Directive
            } else if word.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
                // Conventionally a section class such as COMMON, not a symbol.
                Class::Identifier
            } else {
                Class::Plain
            };
            out.push(start, end, class);
            start = end;
            continue;
        }

        // Fallthrough by whole code points, so multi-byte text cannot split a char.
        let char_len = code[start..]
            .chars()
            .next()
            .expect("start is within the non-empty source")
            .len_utf8();
        out.push(start, start + char_len, Class::Text);
        start += char_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lang, highlight};

    fn classes(src: &str) -> Vec<(&str, Class)> {
        highlight(Lang::Ld, src)
            .into_iter()
            .map(|span| (&src[span.start..span.end], span.class))
            .filter(|(text, _)| !text.trim().is_empty())
            .collect()
    }

    #[test]
    fn commands_sections_and_the_location_counter_are_distinct() {
        let spans =
            classes("ENTRY(_start)\nSECTIONS\n{\n  . = 0x80000000;\n  .text : { *(.text*) }\n}\n");
        assert!(spans.contains(&("ENTRY", Class::Keyword)));
        assert!(spans.contains(&("SECTIONS", Class::Keyword)));
        assert!(spans.contains(&(".", Class::Directive)));
        assert!(spans.contains(&(".text", Class::Directive)));
        assert!(spans.contains(&("0x80000000", Class::Number)));
        assert!(spans.contains(&("{", Class::Bracket)));
    }

    #[test]
    fn common_reads_as_a_section_class_not_a_symbol() {
        let spans = classes(".bss : { *(.bss*) *(COMMON) }\n");
        assert!(spans.contains(&("COMMON", Class::Identifier)));
    }

    #[test]
    fn block_comments_may_cross_lines() {
        let src = "/* one\n   two */\nENTRY(_start)\n";
        let spans = classes(src);
        assert_eq!(spans[0], ("/* one\n   two */", Class::Comment));
        assert_eq!(spans[1], ("ENTRY", Class::Keyword));
    }
}
