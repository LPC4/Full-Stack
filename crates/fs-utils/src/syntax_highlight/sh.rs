// Shell command lines: the command word, its options, quoting and expansions.

use super::{Class, Out};

// Words that start a new command, so the word after them is a command too.
const CONTROL: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "select", "function", "time", "sudo", "env", "exec", "nohup", "command",
];

// Characters that end a bare word.
fn is_break(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'|' | b'&' | b';' | b'(' | b')' | b'<' | b'>')
}

// After these, the next word names a command again rather than an argument.
fn resets_command(byte: u8) -> bool {
    matches!(byte, b'|' | b'&' | b';' | b'(')
}

// Walks past a quoted run, or to the end of the line if it is unterminated.
fn quote_end(bytes: &[u8], from: usize, quote: u8) -> usize {
    let len = bytes.len();
    let mut end = from;
    while end < len && bytes[end] != quote {
        // Only double quotes take backslash escapes; inside '...' a backslash is literal.
        end += if quote == b'"' && bytes[end] == b'\\' {
            2
        } else {
            1
        };
    }
    (end + 1).min(len)
}

pub(crate) fn highlight(code: &str, out: &mut Out) {
    let mut base = 0;

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = match segment.strip_suffix('\n') {
            Some(stripped) => (stripped, true),
            None => (segment, false),
        };

        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut start = 0;
        let mut at_command = true;

        while start < len {
            let mut end = start;

            // A `#` only opens a comment where a word could have started.
            if bytes[start] == b'#' && (start == 0 || bytes[start - 1].is_ascii_whitespace()) {
                out.push(base + start, base + len, Class::Comment);
                break;
            }

            if bytes[start].is_ascii_whitespace() {
                while end < len && bytes[end].is_ascii_whitespace() {
                    end += 1;
                }
                out.push(base + start, base + end, Class::Plain);
                start = end;
                continue;
            }

            if bytes[start] == b'"' || bytes[start] == b'\'' {
                end = quote_end(bytes, start + 1, bytes[start]);
                out.push(base + start, base + end, Class::String);
                start = end;
                at_command = false;
                continue;
            }

            // `$name`, `${name}` and `$(command)`; the parentheses fall out below.
            if bytes[start] == b'$' {
                end += 1;
                if end < len && bytes[end] == b'{' {
                    while end < len && bytes[end] != b'}' {
                        end += 1;
                    }
                    end = (end + 1).min(len);
                } else {
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                }
                out.push(base + start, base + end, Class::Register);
                start = end;
                at_command = false;
                continue;
            }

            if is_break(bytes[start]) {
                at_command = resets_command(bytes[start]);
                out.push(base + start, base + start + 1, Class::Text);
                start += 1;
                continue;
            }

            while end < len && !is_break(bytes[end]) && bytes[end] != b'"' && bytes[end] != b'\'' {
                end += 1;
            }
            let word = &line[start..end];

            let class = if word.starts_with('-') {
                Class::Directive
            } else if at_command || CONTROL.contains(&word) {
                Class::Keyword
            } else if word.bytes().all(|b| b.is_ascii_digit()) {
                Class::Number
            } else {
                Class::Plain
            };
            // `sudo cmd` and the block words keep the next word in command position.
            at_command = CONTROL.contains(&word);
            out.push(base + start, base + end, class);
            start = end;
        }

        if has_newline {
            out.push(base + len, base + len + 1, Class::Plain);
        }
        base += segment.len();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Lang, highlight};
    use super::*;

    fn classes(src: &str) -> Vec<(&str, Class)> {
        highlight(Lang::Sh, src)
            .into_iter()
            .map(|span| (&src[span.start..span.end], span.class))
            .filter(|(text, _)| !text.trim().is_empty())
            .collect()
    }

    #[test]
    fn the_command_word_and_its_options_are_marked() {
        let spans = classes("riscv64-linux-gnu-gcc -nostdlib -T link.ld\n");
        assert_eq!(spans[0], ("riscv64-linux-gnu-gcc", Class::Keyword));
        assert_eq!(spans[1], ("-nostdlib", Class::Directive));
        assert_eq!(spans[2], ("-T", Class::Directive));
        assert_eq!(spans[3], ("link.ld", Class::Plain));
    }

    #[test]
    fn sudo_passes_command_position_to_the_next_word() {
        let spans = classes("sudo apt install qemu\n");
        assert_eq!(spans[0], ("sudo", Class::Keyword));
        assert_eq!(spans[1], ("apt", Class::Keyword));
        assert_eq!(spans[2], ("install", Class::Plain));
    }

    #[test]
    fn quotes_expansions_and_comments_are_separated() {
        let spans = classes("echo \"$HOME/x\" # note\n");
        assert_eq!(spans[1], ("\"$HOME/x\"", Class::String));
        assert_eq!(spans[2], ("# note", Class::Comment));
    }

    // A `#` inside a word is part of the word, not the start of a comment.
    #[test]
    fn a_hash_inside_a_word_is_not_a_comment() {
        let spans = classes("cat file#1\n");
        assert_eq!(spans[1], ("file#1", Class::Plain));
    }

    #[test]
    fn a_pipe_starts_a_new_command() {
        let spans = classes("ls -l | grep x\n");
        assert_eq!(spans[3], ("grep", Class::Keyword));
    }
}
