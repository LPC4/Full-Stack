// x86-64, Intel syntax. Only enough of it to set a contrast piece next to RISC-V.

use crate::{Class, Out};

const INSTRUCTIONS: &[&str] = &[
    "mov", "movzx", "movsx", "movsxd", "lea", "push", "pop", "xchg", "add", "adc", "sub", "sbb",
    "inc", "dec", "neg", "cmp", "test", "and", "or", "xor", "not", "shl", "shr", "sal", "sar",
    "rol", "ror", "mul", "imul", "div", "idiv", "jmp", "je", "jne", "jz", "jnz", "jg", "jge", "jl",
    "jle", "ja", "jae", "jb", "jbe", "call", "ret", "leave", "enter", "nop", "hlt", "syscall",
    "int", "cdq", "cqo", "setz", "setnz", "sete", "setne",
];

// The 64/32/16/8-bit names, plus the size keywords that qualify a memory operand.
const REGISTERS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "r8d", "r9d", "r10d",
    "r11d", "r12d", "r13d", "r14d", "r15d", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "al",
    "bl", "cl", "dl", "ah", "bh", "ch", "dh", "sil", "dil", "rip",
];

const SIZES: &[&str] = &["byte", "word", "dword", "qword", "ptr", "xmmword"];

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

        while start < len {
            let mut end = start;

            // `#` and `;` both open a comment; the book's listings use `#`.
            if bytes[start] == b'#' || bytes[start] == b';' {
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

            if bytes[start].is_ascii_digit() {
                while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                out.push(base + start, base + end, Class::Number);
                start = end;
                continue;
            }

            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' || bytes[start] == b'.' {
                while end < len
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'.')
                {
                    end += 1;
                }
                let word = &line[start..end];
                let lowered = word.to_ascii_lowercase();

                if end < len && bytes[end] == b':' {
                    out.push(base + start, base + end, Class::Label);
                    out.push(base + end, base + end + 1, Class::Text);
                    start = end + 1;
                    continue;
                }
                let class = if INSTRUCTIONS.contains(&lowered.as_str()) {
                    Class::Keyword
                } else if REGISTERS.contains(&lowered.as_str()) {
                    Class::Register
                } else if SIZES.contains(&lowered.as_str()) {
                    Class::Directive
                } else {
                    Class::Plain
                };
                out.push(base + start, base + end, class);
                start = end;
                continue;
            }

            // Whole code points, so multi-byte text cannot split a char.
            let char_len = line[start..]
                .chars()
                .next()
                .expect("start is within the non-empty source line")
                .len_utf8();
            out.push(base + start, base + start + char_len, Class::Plain);
            start += char_len;
        }

        if has_newline {
            out.push(base + len, base + len + 1, Class::Plain);
        }
        base += segment.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lang, highlight};

    fn classes(src: &str) -> Vec<(&str, Class)> {
        highlight(Lang::X86, src)
            .into_iter()
            .map(|span| (&src[span.start..span.end], span.class))
            .filter(|(text, _)| !text.trim().is_empty())
            .collect()
    }

    #[test]
    fn mnemonics_registers_and_immediates_are_distinct() {
        let spans = classes("mov  rax, 0x123456789    # 10 bytes\n");
        assert_eq!(spans[0], ("mov", Class::Keyword));
        assert_eq!(spans[1], ("rax", Class::Register));
        assert_eq!(spans[3], ("0x123456789", Class::Number));
        assert_eq!(spans[4], ("# 10 bytes", Class::Comment));
    }

    // `push rax` and `add eax, ebx` are the contrast the book draws with RISC-V.
    #[test]
    fn the_narrow_and_wide_register_names_both_count() {
        let spans = classes("push rax\nadd  eax, ebx\n");
        assert_eq!(spans[1], ("rax", Class::Register));
        assert_eq!(spans[3], ("eax", Class::Register));
        assert_eq!(spans[5], ("ebx", Class::Register));
    }

    #[test]
    fn operand_size_keywords_read_as_directives() {
        let spans = classes("mov qword ptr [rbp], rax\n");
        assert_eq!(spans[1], ("qword", Class::Directive));
        assert_eq!(spans[2], ("ptr", Class::Directive));
    }
}
