/// A parsed assembler directive line (e.g. `.section .text`, `.globl main`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    Section(String),
    Globl(String),
    Align(u64),
    Balign(u64),
    Byte(u8),
    Half(u16),
    Word(u32),
    Dword(u64),
    Asciz(String),
    Space(u64),
    Equ(String, i64),
    Unknown(String),
}

impl Directive {
    /// Try to parse a raw directive string. Returns None if it doesn't start with `.`.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::parse_with_constants(raw, &HashMap::new())
    }

    pub(crate) fn parse_with_constants(
        raw: &str,
        constants: &HashMap<String, i64>,
    ) -> Option<Self> {
        let s = raw.trim();
        if !s.starts_with('.') {
            return None;
        }

        let (mnemonic, rest) = s
            .splitn(2, |c: char| c.is_whitespace())
            .collect::<Vec<_>>()
            .split_first()
            .map(|(&m, r)| (m, r.first().copied().unwrap_or("").trim()))
            .unwrap_or((s, ""));

        match mnemonic {
            ".section" if !rest.is_empty() => Some(Self::Section(rest.to_owned())),
            ".text" if rest.is_empty() => Some(Self::Section(".text".to_owned())),
            ".data" if rest.is_empty() => Some(Self::Section(".data".to_owned())),
            ".rodata" if rest.is_empty() => Some(Self::Section(".rodata".to_owned())),
            ".bss" if rest.is_empty() => Some(Self::Section(".bss".to_owned())),
            ".globl" | ".global" if !rest.is_empty() => Some(Self::Globl(rest.to_owned())),
            ".align" => parse_u64(rest, constants).map(Self::Align),
            ".balign" => parse_u64(rest, constants).map(Self::Balign),
            ".byte" => parse_u64(rest, constants)
                .and_then(|value| value.try_into().ok())
                .map(Self::Byte),
            ".half" | ".short" => parse_u64(rest, constants)
                .and_then(|value| value.try_into().ok())
                .map(Self::Half),
            ".word" | ".long" => parse_u64(rest, constants)
                .and_then(|value| value.try_into().ok())
                .map(Self::Word),
            ".dword" | ".quad" => parse_u64(rest, constants).map(Self::Dword),
            ".asciz" | ".string" => parse_quoted_string(rest).map(Self::Asciz),
            ".space" | ".zero" => parse_u64(rest, constants).map(Self::Space),
            ".equ" | ".set" => {
                let mut parts = rest.splitn(2, ',');
                let name = parts.next()?.trim();
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    return None;
                }
                let val: i64 = parts.next()?.trim().parse().ok()?;
                Some(Self::Equ(name.to_owned(), val))
            }
            _ if [
                ".section", ".text", ".data", ".rodata", ".bss", ".globl", ".global", ".align",
                ".balign", ".byte", ".half", ".short", ".word", ".long", ".dword", ".quad",
                ".asciz", ".string", ".space", ".zero", ".equ", ".set",
            ]
            .contains(&mnemonic) =>
            {
                None
            }
            _ => Some(Self::Unknown(s.to_owned())),
        }
    }
}

// Strip the surrounding `"..."` from a quoted string directive argument.
// Anything after the closing quote is a trailing comment and is discarded.
fn parse_quoted_string(s: &str) -> Option<String> {
    let s = s.trim();
    let body = s.strip_prefix('"')?;
    let close = find_closing_quote(body)?;
    Some(
        body[..close]
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
}

fn parse_u64(value: &str, constants: &HashMap<String, i64>) -> Option<u64> {
    if let Some(value) = constants.get(value.trim()) {
        (*value).try_into().ok()
    } else {
        value.trim().parse().ok()
    }
}

// Byte index of the first unescaped `"` in an already-opened string literal.
fn find_closing_quote(body: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i),
            _ => {}
        }
    }
    None
}
use std::collections::HashMap;
