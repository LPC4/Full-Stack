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
    /// Null-terminated string `.asciz "..."` / `.string "..."`.
    Asciz(String),
    /// Zero-fill `.space N` or `.zero N`.
    Space(u64),
    /// `.equ name, value`
    Equ(String, i64),
    Unknown(String),
}

impl Directive {
    /// Try to parse a raw directive string (stripped of leading whitespace).
    /// Returns `None` if the string doesn't start with `.`.
    pub fn parse(raw: &str) -> Option<Self> {
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
            ".section" => Some(Self::Section(rest.to_owned())),
            ".text" => Some(Self::Section(".text".to_owned())),
            ".data" => Some(Self::Section(".data".to_owned())),
            ".rodata" => Some(Self::Section(".rodata".to_owned())),
            ".bss" => Some(Self::Section(".bss".to_owned())),
            ".globl" | ".global" => Some(Self::Globl(rest.to_owned())),
            ".align" => rest.trim().parse().ok().map(Self::Align),
            ".balign" => rest.trim().parse().ok().map(Self::Balign),
            ".byte" => rest.trim().parse().ok().map(Self::Byte),
            ".half" | ".short" => rest.trim().parse().ok().map(Self::Half),
            ".word" | ".long" => rest.trim().parse().ok().map(Self::Word),
            ".dword" | ".quad" => rest.trim().parse().ok().map(Self::Dword),
            ".asciz" | ".string" => Some(Self::Asciz(parse_quoted_string(rest))),
            ".space" | ".zero" => rest.trim().parse().ok().map(Self::Space),
            ".equ" | ".set" => {
                let mut parts = rest.splitn(2, ',');
                let name = parts.next()?.trim().to_owned();
                let val: i64 = parts.next()?.trim().parse().ok()?;
                Some(Self::Equ(name, val))
            }
            _ => Some(Self::Unknown(s.to_owned())),
        }
    }
}

/// Strip the surrounding `"..."` from a quoted string directive argument.
fn parse_quoted_string(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        // Unescape basic escape sequences.
        inner
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_owned()
    }
}
